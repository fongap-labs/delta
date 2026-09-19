//! Persistent process-backed capability runner.
//!
//! Used for stateful optional workers such as browser automation. Rust owns the
//! child lifecycle, timeout and cancellation. The worker receives one ABI job
//! and one grant-scoped secret payload per request and emits one terminal result.

use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::capability::{
    CapabilityControl, CapabilityJob, CapabilityProgress, CapabilityResult, CapabilityRunner,
    CAPABILITY_ABI_VERSION,
};

struct WorkerSession {
    child: Child,
    stdin: ChildStdin,
    stdout: Receiver<String>,
    stderr: Receiver<String>,
}

impl WorkerSession {
    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for WorkerSession {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct PersistentWorkerProcessRunner {
    program: PathBuf,
    arguments: Vec<String>,
    environment: BTreeMap<String, String>,
    session: Mutex<Option<WorkerSession>>,
}

impl PersistentWorkerProcessRunner {
    pub fn new(program: impl Into<PathBuf>, arguments: Vec<String>) -> Self {
        Self {
            program: program.into(),
            arguments,
            environment: BTreeMap::new(),
            session: Mutex::new(None),
        }
    }

    pub fn with_environment(mut self, environment: BTreeMap<String, String>) -> Self {
        self.environment = environment;
        self
    }

    fn spawn(&self, secret_keys: &HashSet<&str>) -> Result<WorkerSession, String> {
        for key in self.environment.keys() {
            if secret_keys.contains(key.as_str()) {
                return Err(format!(
                    "persistent worker environment must not contain a granted secret key: {key}"
                ));
            }
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

        for name in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }

        let mut child = command
            .spawn()
            .map_err(|error| format!("persistent worker start failed: {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "persistent worker stdin unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "persistent worker stdout unavailable".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "persistent worker stderr unavailable".to_string())?;

        let (stdout_tx, stdout_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("delta-persistent-worker-stdout".to_string())
            .spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if stdout_tx.send(line).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| format!("persistent worker stdout reader failed: {error}"))?;

        let (stderr_tx, stderr_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("delta-persistent-worker-stderr".to_string())
            .spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if stderr_tx.send(line).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| format!("persistent worker stderr reader failed: {error}"))?;

        Ok(WorkerSession {
            child,
            stdin,
            stdout: stdout_rx,
            stderr: stderr_rx,
        })
    }

    fn reset(session: &mut Option<WorkerSession>) {
        if let Some(mut worker) = session.take() {
            worker.stop();
        }
    }
}

impl CapabilityRunner for PersistentWorkerProcessRunner {
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
        if let Err(error) = job.boundary.validate_execution_grant() {
            return CapabilityResult::failed(&job.job_id, &error.to_string(), Some(error.code()));
        }

        let secret_keys: HashSet<&str> = job.boundary.secrets.iter().map(String::as_str).collect();
        let mut guard = match self.session.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return CapabilityResult::failed(
                    &job.job_id,
                    "persistent worker lock poisoned",
                    Some("worker_lock"),
                )
            }
        };

        let restart = guard
            .as_mut()
            .and_then(|session| session.child.try_wait().ok().flatten())
            .is_some();
        if restart {
            Self::reset(&mut guard);
        }
        if guard.is_none() {
            match self.spawn(&secret_keys) {
                Ok(session) => *guard = Some(session),
                Err(error) => {
                    return CapabilityResult::failed(&job.job_id, &error, Some("worker_start"))
                }
            }
        }

        let worker = guard
            .as_mut()
            .expect("persistent worker session initialized");
        let payload = match serde_json::to_vec(job) {
            Ok(payload) => payload,
            Err(error) => {
                return CapabilityResult::failed(
                    &job.job_id,
                    &format!("serialize worker job: {error}"),
                    Some("worker_input"),
                )
            }
        };
        let secrets = serde_json::to_vec(&job.secret_values).unwrap_or_else(|_| b"{}".to_vec());

        let write_failed = worker.stdin.write_all(&payload).is_err()
            || worker.stdin.write_all(b"\n").is_err()
            || worker.stdin.write_all(&secrets).is_err()
            || worker.stdin.write_all(b"\n").is_err()
            || worker.stdin.flush().is_err();
        if write_failed {
            Self::reset(&mut guard);
            return CapabilityResult::failed(
                &job.job_id,
                "persistent worker input failed",
                Some("worker_stdin"),
            );
        }

        let deadline = Instant::now() + Duration::from_secs(job.timeout_secs.max(1));
        let mut stderr_lines = Vec::new();
        loop {
            while let Ok(line) = worker.stderr.try_recv() {
                stderr_lines.push(line);
            }

            if control.is_cancelled() {
                Self::reset(&mut guard);
                return CapabilityResult::cancelled(&job.job_id).with_stderr(stderr_lines);
            }
            if Instant::now() >= deadline {
                Self::reset(&mut guard);
                return CapabilityResult::timed_out(&job.job_id).with_stderr(stderr_lines);
            }

            match worker.stdout.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => {
                    let parsed = serde_json::from_str::<Value>(&line).ok();
                    if let Some(frame) = parsed.as_ref().filter(|value| value["type"] == "progress")
                    {
                        if let Ok(progress) = serde_json::from_value::<CapabilityProgress>(
                            frame.get("data").cloned().unwrap_or_else(|| frame.clone()),
                        ) {
                            if progress.job_id == job.job_id {
                                control.emit_progress(progress);
                            }
                        }
                        continue;
                    }

                    let Some(frame) = parsed else {
                        continue;
                    };
                    let payload = if frame["type"] == "result" {
                        frame.get("data").cloned().unwrap_or(Value::Null)
                    } else {
                        frame
                    };
                    let Ok(result) = serde_json::from_value::<CapabilityResult>(payload) else {
                        continue;
                    };
                    if result.job_id != job.job_id {
                        Self::reset(&mut guard);
                        return CapabilityResult::failed(
                            &job.job_id,
                            "persistent worker returned a result for the wrong job",
                            Some("worker_protocol"),
                        )
                        .with_stderr(stderr_lines);
                    }
                    return result.with_stderr(stderr_lines);
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    Self::reset(&mut guard);
                    return CapabilityResult::failed(
                        &job.job_id,
                        "persistent worker exited before returning a result",
                        Some("worker_exit"),
                    )
                    .with_stderr(stderr_lines);
                }
            }
        }
    }
}
