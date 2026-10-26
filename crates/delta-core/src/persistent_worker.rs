//! Persistent process-backed capability runner.
//!
//! Used for stateful optional workers such as browser automation. Rust owns the
//! child lifecycle, timeout and cancellation. The worker receives one ABI job
//! and one grant-scoped secret payload per request and emits one terminal result.

use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::capability::process_io::{spawn_line_reader, DropWhenFull};
use crate::capability::process_tree::ProcessTree;
use crate::capability::CapturedLines;
use crate::capability::{
    CapabilityControl, CapabilityJob, CapabilityProgress, CapabilityResult, CapabilityRunner,
    CAPABILITY_ABI_VERSION,
};

/// Longest line of a worker's output that is kept; a typed result is one line.
const MAX_WORKER_LINE_BYTES: usize = 32 * 1024 * 1024;
/// Lines a worker's output may queue before the worker is made to wait (results) or the newest
/// line is dropped (diagnostics).
const QUEUED_LINES: usize = 4096;

struct WorkerSession {
    child: Child,
    tree: ProcessTree,
    /// Jobs to write to the worker. A thread of its own writes them, so a worker that is not
    /// reading cannot hold a call past its timeout or a cancel.
    input: Sender<Vec<u8>>,
    stdout: Receiver<String>,
    stderr: Receiver<String>,
}

impl WorkerSession {
    fn stop(&mut self) {
        self.tree.kill(&mut self.child);
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
        ProcessTree::prepare(&mut command);

        let mut child = command
            .spawn()
            .map_err(|error| format!("persistent worker start failed: {error}"))?;
        let tree = ProcessTree::attach(&child);
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

        let (stdout_tx, stdout_rx) = mpsc::sync_channel(QUEUED_LINES);
        spawn_line_reader(
            "delta-persistent-worker-stdout",
            stdout,
            MAX_WORKER_LINE_BYTES,
            stdout_tx,
            |line| line,
        )
        .map_err(|error| format!("persistent worker stdout reader failed: {error}"))?;

        let (stderr_tx, stderr_rx) = mpsc::sync_channel(QUEUED_LINES);
        spawn_line_reader(
            "delta-persistent-worker-stderr",
            stderr,
            MAX_WORKER_LINE_BYTES,
            DropWhenFull(stderr_tx),
            |line| line,
        )
        .map_err(|error| format!("persistent worker stderr reader failed: {error}"))?;

        let (input_tx, input_rx) = mpsc::channel::<Vec<u8>>();
        std::thread::Builder::new()
            .name("delta-persistent-worker-stdin".to_string())
            .spawn(move || {
                let mut stdin = stdin;
                for message in input_rx {
                    if stdin
                        .write_all(&message)
                        .and_then(|()| stdin.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|error| format!("persistent worker stdin writer failed: {error}"))?;

        Ok(WorkerSession {
            child,
            tree,
            input: input_tx,
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

        let mut message = payload;
        message.push(b'\n');
        message.extend_from_slice(&secrets);
        message.push(b'\n');
        if worker.input.send(message).is_err() {
            Self::reset(&mut guard);
            return CapabilityResult::failed(
                &job.job_id,
                "persistent worker input failed",
                Some("worker_stdin"),
            );
        }

        let deadline = Instant::now() + Duration::from_secs(job.timeout_secs.max(1));
        let mut stderr_lines = CapturedLines::new();
        loop {
            while let Ok(line) = worker.stderr.try_recv() {
                stderr_lines.push(line);
            }

            if control.is_cancelled() {
                Self::reset(&mut guard);
                return CapabilityResult::cancelled(&job.job_id)
                    .with_stderr(stderr_lines.into_vec());
            }
            if Instant::now() >= deadline {
                Self::reset(&mut guard);
                return CapabilityResult::timed_out(&job.job_id)
                    .with_stderr(stderr_lines.into_vec());
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
                        .with_stderr(stderr_lines.into_vec());
                    }
                    return result.with_stderr(stderr_lines.into_vec());
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    Self::reset(&mut guard);
                    return CapabilityResult::failed(
                        &job.job_id,
                        "persistent worker exited before returning a result",
                        Some("worker_exit"),
                    )
                    .with_stderr(stderr_lines.into_vec());
                }
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn typed_result() -> String {
        serde_json::to_string(&CapabilityResult::completed(
            "job-x",
            serde_json::json!({"ok": true}),
        ))
        .unwrap()
    }

    fn job(
        timeout_secs: u64,
        arguments: serde_json::Value,
        dir: &std::path::Path,
    ) -> CapabilityJob {
        let mut job = CapabilityJob::new("worker.test", "job-x", arguments);
        job.workspace = Some(dir.to_string_lossy().to_string());
        job.timeout_secs = timeout_secs;
        job
    }

    /// Run `job` on `runner` with a watchdog: a runner that never returns must fail the test.
    fn run_watched(
        runner: &Arc<PersistentWorkerProcessRunner>,
        job: CapabilityJob,
    ) -> (CapabilityResult, Duration) {
        let runner = runner.clone();
        let started = Instant::now();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let control =
                CapabilityControl::new(Arc::new(AtomicBool::new(false)), Arc::new(|_| {}));
            let _ = tx.send(runner.run(&job, &control));
        });
        let result = rx
            .recv_timeout(Duration::from_secs(20))
            .expect("the runner did not return within 20 seconds");
        (result, started.elapsed())
    }

    fn runner(script: &str) -> Arc<PersistentWorkerProcessRunner> {
        Arc::new(PersistentWorkerProcessRunner::new(
            "/bin/sh",
            vec!["-c".to_string(), script.to_string()],
        ))
    }

    fn answering_script(noise: &str) -> String {
        format!(
            "while IFS= read -r JOB; do IFS= read -r SECRETS; {noise} printf '%s\\n' '{}'; done",
            typed_result()
        )
    }

    #[test]
    fn one_worker_process_answers_job_after_job() {
        let dir = tempfile::tempdir().unwrap();
        let runner = runner(&answering_script(""));
        for _ in 0..2 {
            let (result, _) = run_watched(&runner, job(10, serde_json::json!({}), dir.path()));
            assert_eq!(
                result.state,
                crate::capability::CapabilityExitState::Completed
            );
        }
    }

    #[test]
    fn a_line_that_is_not_utf8_does_not_lose_the_result_after_it() {
        let dir = tempfile::tempdir().unwrap();
        let runner = runner(&answering_script("printf '\\377 noise\\n';"));
        let (result, _) = run_watched(&runner, job(10, serde_json::json!({}), dir.path()));
        assert_eq!(
            result.state,
            crate::capability::CapabilityExitState::Completed
        );
    }

    #[test]
    fn a_worker_that_never_reads_its_input_is_stopped_by_the_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let runner = runner("sleep 30");
        let big = serde_json::json!({"payload": "x".repeat(2 * 1024 * 1024)});
        let (result, took) = run_watched(&runner, job(1, big, dir.path()));
        assert_eq!(
            result.state,
            crate::capability::CapabilityExitState::TimedOut
        );
        assert!(took < Duration::from_secs(5), "the timeout took {took:?}");
    }

    #[test]
    fn a_timeout_ends_the_processes_the_worker_started() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let runner = runner(&format!(
            "IFS= read -r JOB; sleep 30 & echo $! > {}; sleep 30",
            pid_file.display()
        ));
        let (result, _) = run_watched(&runner, job(1, serde_json::json!({}), dir.path()));
        assert_eq!(
            result.state,
            crate::capability::CapabilityExitState::TimedOut
        );
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
}
