//! Capability runner implementations.
//!
//! The ABI, registry and host remain authoritative in the parent module. This
//! module owns execution adapters only: in-process native functions, controlled
//! subprocess workers, and MCP worker bridging.

use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::abi::{
    CapabilityExitState, CapabilityJob, CapabilityProgress, CapabilityResult,
    CAPABILITY_ABI_VERSION,
};
use super::registry::{CapabilityControl, CapabilityRunner};

type NativeHandler =
    dyn Fn(&CapabilityJob, &CapabilityControl) -> CapabilityResult + Send + Sync + 'static;

/// In-process capability runner for small, dependency-free trusted operations.
pub struct NativeCapabilityRunner {
    handler: Arc<NativeHandler>,
}

impl NativeCapabilityRunner {
    pub fn new<F>(handler: F) -> Self
    where
        F: Fn(&CapabilityJob, &CapabilityControl) -> CapabilityResult + Send + Sync + 'static,
    {
        Self {
            handler: Arc::new(handler),
        }
    }
}

impl CapabilityRunner for NativeCapabilityRunner {
    fn run(&self, job: &CapabilityJob, control: &CapabilityControl) -> CapabilityResult {
        if control.is_cancelled() {
            return CapabilityResult::cancelled(&job.job_id);
        }
        (self.handler)(job, control)
    }
}

/// Process-backed runner for Python, PowerShell, shell, and other controlled
/// workers. One ABI job is written to stdin. Stdout may contain progress frames
/// followed by exactly one typed CapabilityResult. The Runtime owns
/// timeout/cancellation and force-terminates the child when either fires.
pub struct WorkerProcessRunner {
    program: PathBuf,
    arguments: Vec<String>,
    environment: BTreeMap<String, String>,
}

impl WorkerProcessRunner {
    pub fn new(program: impl Into<PathBuf>, arguments: Vec<String>) -> Self {
        Self {
            program: program.into(),
            arguments,
            environment: BTreeMap::new(),
        }
    }

    pub fn with_environment(mut self, environment: BTreeMap<String, String>) -> Self {
        self.environment = environment;
        self
    }

    pub fn python(program: impl Into<PathBuf>, script: impl Into<String>) -> Self {
        Self::new(program, vec![script.into()])
    }

    pub fn powershell(program: impl Into<PathBuf>, script: impl Into<String>) -> Self {
        Self::new(
            program,
            vec![
                "-NoLogo".to_string(),
                "-NoProfile".to_string(),
                "-NonInteractive".to_string(),
                "-File".to_string(),
                script.into(),
            ],
        )
    }

    pub fn shell(program: impl Into<PathBuf>, script: impl Into<String>) -> Self {
        Self::new(program, vec![script.into()])
    }
}

pub(super) enum WorkerLine {
    Stdout(String),
    Stderr(String),
}

impl CapabilityRunner for WorkerProcessRunner {
    fn run(&self, job: &CapabilityJob, control: &CapabilityControl) -> CapabilityResult {
        if control.is_cancelled() {
            return CapabilityResult::cancelled(&job.job_id);
        }
        if job.abi_version != CAPABILITY_ABI_VERSION {
            return CapabilityResult::failed(
                &job.job_id,
                "worker ABI version mismatch",
                Some("abi_mismatch"),
            );
        }
        // Defensive secondary grant validation. The primary check is
        // in CapabilityHost::execute() before dispatch; this is a fail-closed
        // guard against direct runner calls bypassing the Host.
        if let Err(grant_error) = job.boundary.validate_execution_grant() {
            return CapabilityResult::failed(
                &job.job_id,
                &grant_error.to_string(),
                Some(grant_error.code()),
            );
        }
        let mut command = Command::new(&self.program);
        command
            .args(&self.arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear()
            .envs(&self.environment)
            .env("DELTA_CAPABILITY_ABI", CAPABILITY_ABI_VERSION.to_string());
        // A clean environment is the secret boundary. Only operating-system
        // bootstrap variables are inherited; provider credentials and the
        // parent process environment are never exposed implicitly. Granted
        // secrets are delivered via the ephemeral stdin payload,
        // never as environment variables — fail-closed guard against leak.
        for name in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        // Explicit no-secret-in-env guarantee: none of the worker's explicit
        // environment values may shadow a granted secret key. Secrets are
        // delivered through the ephemeral stdin payload, never
        // through the process environment.
        let secret_keys: HashSet<&str> = job.boundary.secrets.iter().map(String::as_str).collect();
        for key in self.environment.keys() {
            if secret_keys.contains(key.as_str()) {
                return CapabilityResult::failed(
                    &job.job_id,
                    &format!("worker environment must not contain a granted secret key: {key}"),
                    Some("secret_in_env"),
                );
            }
        }
        if let Some(workspace) = &job.workspace {
            command.current_dir(workspace);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                return CapabilityResult::failed(
                    &job.job_id,
                    &format!("worker start failed: {error}"),
                    Some("worker_start"),
                )
            }
        };
        if let Some(mut stdin) = child.stdin.take() {
            match serde_json::to_vec(job) {
                Ok(payload) => {
                    if stdin.write_all(&payload).is_err() || stdin.write_all(b"\n").is_err() {
                        let _ = child.kill();
                        return CapabilityResult::failed(
                            &job.job_id,
                            "worker input failed",
                            Some("worker_stdin"),
                        );
                    }
                    // Deliver ephemeral secret values as a separate
                    // stdin payload after the job JSON. Secrets are never
                    // in the serialized job (serde(skip)), never in
                    // environment variables, never persisted. Workers read
                    // the job line first, then the secrets line. The pipe
                    // closes after delivery, revoking access.
                    let secret_payload =
                        serde_json::to_vec(&job.secret_values).unwrap_or_else(|_| b"{}".to_vec());
                    if stdin.write_all(&secret_payload).is_err() || stdin.write_all(b"\n").is_err()
                    {
                        let _ = child.kill();
                        return CapabilityResult::failed(
                            &job.job_id,
                            "worker secret delivery failed",
                            Some("worker_stdin"),
                        );
                    }
                }
                Err(error) => {
                    let _ = child.kill();
                    return CapabilityResult::failed(
                        &job.job_id,
                        &error.to_string(),
                        Some("job_serialize"),
                    );
                }
            }
        }
        let (lines_tx, lines_rx) = mpsc::channel();
        let stdout_worker = child.stdout.take().map(|stdout| {
            let sender = lines_tx.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    let _ = sender.send(WorkerLine::Stdout(line));
                }
            })
        });
        let stderr_worker = child.stderr.take().map(|stderr| {
            let sender = lines_tx;
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let _ = sender.send(WorkerLine::Stderr(line));
                }
            })
        });
        let started = Instant::now();
        let timeout = (job.timeout_secs > 0).then(|| Duration::from_secs(job.timeout_secs));
        let mut stdout_lines = Vec::new();
        let mut stderr_lines = Vec::new();
        let mut result = None;
        let forced_state = loop {
            drain_worker_lines(
                &lines_rx,
                control,
                &mut stdout_lines,
                &mut stderr_lines,
                &mut result,
            );
            if control.is_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                break Some(CapabilityExitState::Cancelled);
            }
            if timeout.is_some_and(|timeout| started.elapsed() >= timeout) {
                let _ = child.kill();
                let _ = child.wait();
                break Some(CapabilityExitState::TimedOut);
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() && result.is_none() {
                        result = Some(CapabilityResult::failed(
                            &job.job_id,
                            &format!("worker exited with {status}"),
                            Some("worker_exit"),
                        ));
                    }
                    break None;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(error) => {
                    let _ = child.kill();
                    result = Some(CapabilityResult::failed(
                        &job.job_id,
                        &format!("worker wait failed: {error}"),
                        Some("worker_wait"),
                    ));
                    break None;
                }
            }
        };
        if let Some(worker) = stdout_worker {
            let _ = worker.join();
        }
        if let Some(worker) = stderr_worker {
            let _ = worker.join();
        }
        drain_worker_lines(
            &lines_rx,
            control,
            &mut stdout_lines,
            &mut stderr_lines,
            &mut result,
        );
        let mut result = match forced_state {
            Some(CapabilityExitState::Cancelled) => CapabilityResult::cancelled(&job.job_id),
            Some(CapabilityExitState::TimedOut) => CapabilityResult::timed_out(&job.job_id),
            _ => result.unwrap_or_else(|| {
                let message = if stdout_lines.is_empty() {
                    "worker returned no typed result".to_string()
                } else {
                    format!(
                        "worker returned no typed result ({} untyped stdout line(s))",
                        stdout_lines.len()
                    )
                };
                CapabilityResult::failed(&job.job_id, &message, Some("worker_protocol"))
            }),
        };
        if !stderr_lines.is_empty() {
            result = result.with_stderr(stderr_lines);
        }
        if result.abi_version != CAPABILITY_ABI_VERSION || result.job_id != job.job_id {
            return CapabilityResult::failed(
                &job.job_id,
                "worker result did not match the requested ABI job",
                Some("worker_protocol"),
            )
            .with_stderr(
                result
                    .diagnostics
                    .map(|diagnostics| diagnostics.stderr_lines)
                    .unwrap_or_default(),
            );
        }
        result
    }
}

pub(super) fn drain_worker_lines(
    receiver: &mpsc::Receiver<WorkerLine>,
    control: &CapabilityControl,
    stdout_lines: &mut Vec<String>,
    stderr_lines: &mut Vec<String>,
    result: &mut Option<CapabilityResult>,
) {
    while let Ok(line) = receiver.try_recv() {
        match line {
            WorkerLine::Stderr(line) => stderr_lines.push(line),
            WorkerLine::Stdout(line) => {
                let parsed = serde_json::from_str::<Value>(&line).ok();
                if let Some(frame) = parsed.as_ref().filter(|value| value["type"] == "progress") {
                    if let Ok(progress) = serde_json::from_value::<CapabilityProgress>(
                        frame.get("data").cloned().unwrap_or_else(|| frame.clone()),
                    ) {
                        control.emit_progress(progress);
                    }
                } else if let Some(frame) = parsed {
                    let payload = if frame["type"] == "result" {
                        frame.get("data").cloned().unwrap_or(Value::Null)
                    } else {
                        frame
                    };
                    if let Ok(typed) = serde_json::from_value::<CapabilityResult>(payload) {
                        if result.is_some() {
                            *result = Some(CapabilityResult::failed(
                                &typed.job_id,
                                "worker emitted multiple terminal results",
                                Some("worker_protocol"),
                            ));
                        } else {
                            *result = Some(typed);
                        }
                    } else {
                        stdout_lines.push(line);
                    }
                } else {
                    stdout_lines.push(line);
                }
            }
        }
    }
}

/// MCP runner adapter. MCP servers remain workers; the adapter wraps a model
/// tool call in a typed `mcp.call` job consumed by a configured MCP bridge.
pub struct McpCapabilityRunner {
    server_id: String,
    worker: WorkerProcessRunner,
}

impl McpCapabilityRunner {
    pub fn new(server_id: impl Into<String>, worker: WorkerProcessRunner) -> Self {
        Self {
            server_id: server_id.into(),
            worker,
        }
    }
}

impl CapabilityRunner for McpCapabilityRunner {
    fn run(&self, job: &CapabilityJob, control: &CapabilityControl) -> CapabilityResult {
        let mut bridge_job = job.clone();
        bridge_job.capability_id = "mcp.call".to_string();
        bridge_job.arguments = serde_json::json!({
            "server_id": self.server_id,
            "tool": job.capability_id,
            "arguments": job.arguments,
        });
        self.worker.run(&bridge_job, control)
    }
}
