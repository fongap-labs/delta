//! Capability runner implementations.
//!
//! The ABI, registry and host remain authoritative in the parent module. This
//! module owns execution adapters only: in-process native functions, controlled
//! subprocess workers, and MCP worker bridging.

use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::abi::{
    CapabilityExitState, CapabilityJob, CapabilityProgress, CapabilityResult, CapturedLines,
    CAPABILITY_ABI_VERSION,
};
use super::process_io::{join_readers_within, spawn_line_reader};
use super::process_tree::ProcessTree;
use super::registry::{CapabilityControl, CapabilityRunner};

/// Longest line of a worker's output that is kept. A typed result is one line, so this is also the
/// largest result a worker can return; a longer line is cut and the call fails as untyped output
/// instead of growing memory without bound.
const MAX_WORKER_LINE_BYTES: usize = 32 * 1024 * 1024;

/// How long a worker that stopped reading its input may keep running before the call is failed.
const INPUT_FAILURE_GRACE: Duration = Duration::from_secs(1);

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
        ProcessTree::prepare(&mut command);
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
        let tree = ProcessTree::attach(&child);

        // The job is written on a thread of its own: a worker that has not started reading yet, or
        // never will, must not hold this call past its timeout or a cancel. Ephemeral secret values
        // follow the job as a second line, never in the serialized job (serde(skip)), never in
        // environment variables, never persisted; the pipe closes after delivery, revoking access.
        let payload = match serde_json::to_vec(job) {
            Ok(payload) => payload,
            Err(error) => {
                tree.kill(&mut child);
                let _ = child.wait();
                return CapabilityResult::failed(
                    &job.job_id,
                    &error.to_string(),
                    Some("job_serialize"),
                );
            }
        };
        let secret_payload =
            serde_json::to_vec(&job.secret_values).unwrap_or_else(|_| b"{}".to_vec());
        let (input_tx, input_rx) = mpsc::channel::<std::io::Result<()>>();
        if let Some(mut stdin) = child.stdin.take() {
            std::thread::spawn(move || {
                let outcome = stdin
                    .write_all(&payload)
                    .and_then(|()| stdin.write_all(b"\n"))
                    .and_then(|()| stdin.write_all(&secret_payload))
                    .and_then(|()| stdin.write_all(b"\n"));
                let _ = input_tx.send(outcome);
            });
        }

        let (lines_tx, lines_rx) = mpsc::channel();
        let readers: Vec<_> = [
            child.stdout.take().and_then(|stdout| {
                spawn_line_reader(
                    "delta-worker-stdout",
                    stdout,
                    MAX_WORKER_LINE_BYTES,
                    lines_tx.clone(),
                    WorkerLine::Stdout,
                )
                .ok()
            }),
            child.stderr.take().and_then(|stderr| {
                spawn_line_reader(
                    "delta-worker-stderr",
                    stderr,
                    MAX_WORKER_LINE_BYTES,
                    lines_tx,
                    WorkerLine::Stderr,
                )
                .ok()
            }),
        ]
        .into_iter()
        .flatten()
        .collect();
        let started = Instant::now();
        let timeout = (job.timeout_secs > 0).then(|| Duration::from_secs(job.timeout_secs));
        let mut stdout_lines = CapturedLines::new();
        let mut stderr_lines = CapturedLines::new();
        let mut result = None;
        let mut input_error = None;
        let forced_state = loop {
            drain_worker_lines(
                &lines_rx,
                control,
                &mut stdout_lines,
                &mut stderr_lines,
                &mut result,
            );
            if input_error.is_none() {
                if let Ok(Err(error)) = input_rx.try_recv() {
                    input_error = Some((error.to_string(), Instant::now()));
                }
            }
            // A worker that stopped reading its input but did not exit has a second to finish what
            // it is doing (it may be about to leave a diagnosis on stderr), then it is ended.
            if let Some((error, at)) = &input_error {
                if at.elapsed() >= INPUT_FAILURE_GRACE && result.is_none() {
                    tree.kill(&mut child);
                    let _ = child.wait();
                    result = Some(CapabilityResult::failed(
                        &job.job_id,
                        &format!("worker input failed: {error}"),
                        Some("worker_stdin"),
                    ));
                    break None;
                }
            }
            if control.is_cancelled() {
                tree.kill(&mut child);
                let _ = child.wait();
                break Some(CapabilityExitState::Cancelled);
            }
            if timeout.is_some_and(|timeout| started.elapsed() >= timeout) {
                tree.kill(&mut child);
                let _ = child.wait();
                break Some(CapabilityExitState::TimedOut);
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    tree.release();
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
                    tree.kill(&mut child);
                    result = Some(CapabilityResult::failed(
                        &job.job_id,
                        &format!("worker wait failed: {error}"),
                        Some("worker_wait"),
                    ));
                    break None;
                }
            }
        };
        join_readers_within(readers, &mut || {
            drain_worker_lines(
                &lines_rx,
                control,
                &mut stdout_lines,
                &mut stderr_lines,
                &mut result,
            )
        });
        if result.is_none() && forced_state.is_none() {
            if let Some((error, _)) = input_error {
                result = Some(CapabilityResult::failed(
                    &job.job_id,
                    &format!("worker input failed: {error}"),
                    Some("worker_stdin"),
                ));
            }
        }
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
            result = result.with_stderr(stderr_lines.into_vec());
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
    stdout_lines: &mut CapturedLines,
    stderr_lines: &mut CapturedLines,
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::time::Instant;

    fn run_script(
        script: &str,
        timeout_secs: u64,
        arguments: serde_json::Value,
    ) -> (CapabilityResult, Duration) {
        let dir = tempfile::tempdir().unwrap();
        let runner =
            WorkerProcessRunner::new("/bin/sh", vec!["-c".to_string(), script.to_string()]);
        let mut job = CapabilityJob::new("worker.test", "job-x", arguments);
        job.workspace = Some(dir.path().to_string_lossy().to_string());
        job.timeout_secs = timeout_secs;
        let control = CapabilityControl::new(Arc::new(AtomicBool::new(false)), Arc::new(|_| {}));
        let started = Instant::now();
        // A watchdog: a runner that never returns must fail this test, not hang the suite.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(runner.run(&job, &control));
        });
        let result = rx
            .recv_timeout(Duration::from_secs(20))
            .expect("the runner did not return within 20 seconds");
        (result, started.elapsed())
    }

    fn typed_result() -> String {
        serde_json::to_string(&CapabilityResult::completed(
            "job-x",
            serde_json::json!({"ok": true}),
        ))
        .unwrap()
    }

    #[test]
    fn a_line_that_is_not_utf8_does_not_lose_the_result_after_it() {
        let script = format!(
            "IFS= read -r J; printf '\\377 noise\\n'; printf '%s\\n' '{}'",
            typed_result()
        );
        let (result, _) = run_script(&script, 10, serde_json::json!({}));
        assert_eq!(
            result.state,
            CapabilityExitState::Completed,
            "{:?}",
            result.diagnostics
        );
    }

    #[test]
    fn a_timeout_is_reported_on_time_when_a_background_process_holds_the_pipe() {
        let (result, took) = run_script(
            "IFS= read -r J; sleep 5 & sleep 5",
            1,
            serde_json::json!({}),
        );
        assert_eq!(result.state, CapabilityExitState::TimedOut);
        assert!(took < Duration::from_secs(3), "the timeout took {took:?}");
    }

    #[test]
    fn a_timeout_ends_the_processes_the_worker_started() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let script = format!(
            "IFS= read -r J; sleep 30 & echo $! > {}; sleep 30",
            pid_file.display()
        );
        let (result, _) = run_script(&script, 1, serde_json::json!({}));
        assert_eq!(result.state, CapabilityExitState::TimedOut);
        let pid: u32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let alive = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|stat| {
                !stat
                    .rsplit(')')
                    .next()
                    .unwrap_or("")
                    .trim_start()
                    .starts_with('Z')
            })
            .unwrap_or(false);
        assert!(!alive, "process {pid} outlived the timeout");
    }

    #[test]
    fn a_worker_that_closes_its_input_and_carries_on_is_failed_after_a_moment() {
        // It closes stdin without reading the big job, then just sleeps.
        let big = serde_json::json!({"payload": "x".repeat(2 * 1024 * 1024)});
        let (result, took) = run_script("exec 0<&-; sleep 30", 20, big);
        assert_eq!(result.state, CapabilityExitState::Failed);
        assert_eq!(
            result
                .diagnostics
                .as_ref()
                .and_then(|d| d.error_code.as_deref()),
            Some("worker_stdin")
        );
        assert!(took < Duration::from_secs(5), "took {took:?}");
    }

    #[test]
    fn a_worker_that_never_reads_its_input_is_still_stopped_by_the_timeout() {
        // The job is far larger than a pipe holds, and the worker never reads it.
        let big = serde_json::json!({"payload": "x".repeat(2 * 1024 * 1024)});
        let (result, took) = run_script("sleep 30", 1, big);
        assert_eq!(result.state, CapabilityExitState::TimedOut);
        assert!(took < Duration::from_secs(5), "the timeout took {took:?}");
    }
}
