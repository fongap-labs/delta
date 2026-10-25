//! The model-facing shell tool: one approved command, one captured result.
//!
//! The tool executes a single command line through the platform shell
//! (Windows PowerShell, `/bin/sh` elsewhere) and reports exit code, stdout and
//! stderr. It shares the controlled-worker trust model: a cleared environment
//! with operating-system bootstrap variables only, no console window, and the
//! runtime driving cancellation and the hard timeout (the timeout arrives as
//! `job.timeout_secs` from the tool execution context).
//!
//! Security rests on the layers upstream, which stay in force:
//! - the Policy layer classifies `run_shell` as high risk, so every command
//!   requires an explicit approval before dispatch;
//! - an "always allow" decision for this tool only ever stores the exact
//!   command string (standing approver `AlwaysCommand`), never the tool;
//! - the runner passes no secrets through the environment and never shadows a
//!   granted secret key.

use std::io::BufRead;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::abi::{
    CapabilityExitState, CapabilityGrants, CapabilityJob, CapabilityProgress, CapabilityResult,
};
use super::registry::{CapabilityControl, CapabilityRegistration};
use super::runners::NativeCapabilityRunner;

const TOOL_NAME: &str = "run_shell";
const CAPABILITY_ID: &str = "shell.execute";
/// One output stream never records more than this many characters; the rest is
/// dropped once the flag is set (the child keeps being drained so it cannot
/// block on a full pipe, the excess just never ends up in the result).
const MAX_OUTPUT_CHARS: usize = 16_000;
/// How often the output channel, the cancellation flag and the process state
/// are polled while the command runs.
const POLL_MILLIS: u64 = 10;

pub fn shell_registration() -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: CAPABILITY_ID.to_string(),
        tool_name: TOOL_NAME.to_string(),
        description: "Run one shell command locally and capture its exit code, stdout and stderr. Every command needs explicit approval; \"always allow\" applies to the exact command only.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {"type": "string", "description": "The command line to run."},
                "workdir": {
                    "type": "string",
                    "description": "Directory to run in (workspace-relative or absolute under a read grant). Defaults to the workspace."
                }
            }
        }),
        metadata: serde_json::json!({
            "risk_level": "high", "requires_approval": true,
            "category": "shell", "capabilities": [CAPABILITY_ID]
        }),
        // A shell command can change anything on the machine and is not
        // reversible from here, whatever it actually does this time.
        side_effect: crate::skills::SideEffectClass::Destructive,
        // The platform shell is not jailed by these roots; native runners treat
        // the boundary as an auditable statement of what the tool is there for,
        // not a sandbox. The exec grant in the boundary is why the Policy layer
        // keeps the tool behind an approval.
        grants: CapabilityGrants {
            exec: true,
            ..CapabilityGrants::default()
        },
        workspace_write: true,
        runner: Arc::new(NativeCapabilityRunner::new(run_shell_job)),
    }
}

/// Run one `run_shell` job: parse arguments, spawn the platform shell, capture
/// both output streams while the runtime clock runs, and report a typed result.
fn run_shell_job(job: &CapabilityJob, control: &CapabilityControl) -> CapabilityResult {
    let Some(command) = command_argument(job) else {
        return CapabilityResult::failed(&job.job_id, "command is required", Some("arguments"));
    };
    let workdir = match resolve_workdir(job) {
        Ok(workdir) => workdir,
        Err(result) => return result,
    };
    control.emit_progress(CapabilityProgress {
        job_id: job.job_id.clone(),
        fraction: 0.0,
        stage: Some("running".to_string()),
        message: None,
    });

    let mut process = match spawn_shell(&command, workdir.as_deref(), &job.job_id) {
        Ok(process) => process,
        Err(result) => return result,
    };
    let (lines_tx, lines_rx) = mpsc::channel::<(bool, String)>();
    let readers = start_readers(&mut process, lines_tx);

    let started = Instant::now();
    let timeout = (job.timeout_secs > 0).then(|| Duration::from_secs(job.timeout_secs));
    let mut stdout = CapturedOutput::default();
    let mut stderr = CapturedOutput::default();
    let mut forced_state = None;
    let mut exit_code: Option<i32> = None;
    loop {
        drain_lines(&lines_rx, &mut stdout, &mut stderr);
        if control.is_cancelled() {
            let _ = process.kill();
            let _ = process.wait();
            forced_state = Some(CapabilityExitState::Cancelled);
            break;
        }
        if timeout.is_some_and(|timeout| started.elapsed() >= timeout) {
            let _ = process.kill();
            let _ = process.wait();
            forced_state = Some(CapabilityExitState::TimedOut);
            break;
        }
        match process.try_wait() {
            Ok(Some(status)) => {
                exit_code = status.code();
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(POLL_MILLIS)),
            Err(error) => {
                let _ = process.kill();
                let _ = process.wait();
                for reader in readers {
                    let _ = reader.join();
                }
                return CapabilityResult::failed(
                    &job.job_id,
                    &format!("shell wait failed: {error}"),
                    Some("shell_wait"),
                );
            }
        }
    }
    for reader in readers {
        let _ = reader.join();
    }
    drain_lines(&lines_rx, &mut stdout, &mut stderr);

    if let Some(state) = forced_state {
        // The killed child's remaining output is gone; the runtime's own
        // terminal state stands, as it does for the controlled workers under
        // the same outcomes.
        return match state {
            CapabilityExitState::Cancelled => CapabilityResult::cancelled(&job.job_id),
            _ => CapabilityResult::timed_out(&job.job_id),
        };
    }
    CapabilityResult::completed(
        &job.job_id,
        json!({
            "ok": true,
            "exit_code": unwrap_exit_code(exit_code),
            "stdout": stdout.text,
            "stderr": stderr.text,
            "truncated_stdout": stdout.truncated,
            "truncated_stderr": stderr.truncated,
        }),
    )
}

/// The command the model asks to run: a non-empty JSON string.
fn command_argument(job: &CapabilityJob) -> Option<String> {
    let argument = job.arguments.get("command")?.as_str()?.trim().to_string();
    (!argument.is_empty()).then_some(argument)
}

/// The working directory the command runs in: the optional `workdir` argument
/// (which must stay inside the job's read grants) or the job's workspace.
#[allow(clippy::result_large_err)]
fn resolve_workdir(job: &CapabilityJob) -> Result<Option<PathBuf>, CapabilityResult> {
    let default = job.workspace.as_ref().map(PathBuf::from);
    let Some(raw) = job.arguments.get("workdir").and_then(Value::as_str) else {
        return Ok(default);
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(default);
    }
    match super::host::resolve_read_path(job, raw) {
        Ok(path) if path.is_dir() => Ok(Some(path)),
        Ok(_) => Err(CapabilityResult::failed(
            &job.job_id,
            "workdir is not a directory",
            Some("arguments"),
        )),
        Err(("grant_denied", message)) => Err(CapabilityResult::failed(
            &job.job_id,
            &message,
            Some("grant_denied"),
        )),
        Err(("read_failed", message)) => Err(CapabilityResult::failed(
            &job.job_id,
            &message,
            Some("arguments"),
        )),
        Err((code, message)) => Err(CapabilityResult::failed(&job.job_id, &message, Some(code))),
    }
}

/// Spawn the platform shell for one command line. The environment is cleared
/// down to operating-system bootstrap variables; `PATH` stays inherited
/// because the shell's whole job is to resolve tools by name (`git`, `uv`) and
/// it names directories, not secret values. Nothing in the explicit set may
/// shadow a granted secret key.
#[allow(clippy::result_large_err)]
fn spawn_shell(
    command: &str,
    workdir: Option<&std::path::Path>,
    job_id: &str,
) -> Result<std::process::Child, CapabilityResult> {
    let mut process = platform_shell_command(command);
    process
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();
    for name in bootstrap_environment_names() {
        if let Some(value) = std::env::var_os(name) {
            process.env(name, value);
        }
    }
    apply_platform_tuning(&mut process);
    if let Some(workdir) = workdir {
        process.current_dir(workdir);
    }
    process.spawn().map_err(|error| {
        CapabilityResult::failed(
            job_id,
            &format!("shell start failed: {error}"),
            Some("shell_start"),
        )
    })
}

/// The program and arguments that run one command line on this platform.
#[cfg(windows)]
fn platform_shell_command(command: &str) -> Command {
    // PowerShell resolves from the system directory when the absolute path
    // exists, so the child's cleared environment cannot influence where the
    // interpreter itself comes from.
    let program = std::env::var_os("SystemRoot")
        .map(|system_root| {
            PathBuf::from(system_root)
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
        })
        .filter(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from("powershell.exe"));
    let mut process = Command::new(program);
    process.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        command,
    ]);
    process
}

#[cfg(not(windows))]
fn platform_shell_command(command: &str) -> Command {
    let mut process = Command::new("/bin/sh");
    process.args(["-c", command]);
    process
}

/// Operating-system bootstrap variables the child inherits. Secrets and the
/// parent's general environment never cross the boundary, matching the
/// controlled-worker rule.
#[cfg(windows)]
fn bootstrap_environment_names() -> &'static [&'static str] {
    &[
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "PATH",
        "PATHEXT",
        "COMSPEC",
        "USERPROFILE",
    ]
}

#[cfg(not(windows))]
fn bootstrap_environment_names() -> &'static [&'static str] {
    &["PATH", "HOME"]
}

#[cfg(windows)]
fn apply_platform_tuning(process: &mut Command) {
    // CREATE_NO_WINDOW: a command that does not flash a console while the
    // desktop app is in the foreground.
    use std::os::windows::process::CommandExt;
    process.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn apply_platform_tuning(_process: &mut Command) {}

/// Move both pipes into reader threads so a chatty command cannot deadlock the
/// supervising loop. Each line carries its stream (`true` = stdout).
fn start_readers(
    process: &mut std::process::Child,
    lines_tx: mpsc::Sender<(bool, String)>,
) -> Vec<std::thread::JoinHandle<()>> {
    let stdout = process
        .stdout
        .take()
        .map(|stream| stream_reader(stream, lines_tx.clone(), true));
    let stderr = process
        .stderr
        .take()
        .map(|stream| stream_reader(stream, lines_tx, false));
    stdout.into_iter().chain(stderr).collect()
}

fn stream_reader(
    stream: impl std::io::Read + Send + 'static,
    lines_tx: mpsc::Sender<(bool, String)>,
    is_stdout: bool,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stream)
            .lines()
            .map_while(Result::ok)
        {
            if lines_tx.send((is_stdout, line)).is_err() {
                break;
            }
        }
    })
}

/// Pull everything the reader threads have produced so far into the captured
/// output, dropping lines once a stream is at its character cap.
fn drain_lines(
    lines_rx: &mpsc::Receiver<(bool, String)>,
    stdout: &mut CapturedOutput,
    stderr: &mut CapturedOutput,
) {
    while let Ok((is_stdout, line)) = lines_rx.try_recv() {
        if is_stdout {
            stdout.push(&line);
        } else {
            stderr.push(&line);
        }
    }
}

/// The output captured for one stream: characters joined with newlines, capped
/// at [`MAX_OUTPUT_CHARS`], with a flag telling whether anything was dropped.
#[derive(Default)]
struct CapturedOutput {
    text: String,
    truncated: bool,
}

impl CapturedOutput {
    fn push(&mut self, line: &str) {
        if self.text.len() >= MAX_OUTPUT_CHARS {
            self.truncated = true;
            return;
        }
        if !self.text.is_empty() {
            self.text.push('\n');
        }
        let room = MAX_OUTPUT_CHARS - self.text.len();
        if line.len() <= room {
            self.text.push_str(line);
        } else {
            let mut end = room;
            while end > 0 && !line.is_char_boundary(end) {
                end -= 1;
            }
            self.text.push_str(&line[..end]);
            self.truncated = true;
        }
    }
}

/// The exit code the process actually returned; `None` (signals on Unix) turns
/// into `-1` so the JSON contract stays an integer for the model.
fn unwrap_exit_code(code: Option<i32>) -> i32 {
    code.unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::registry::CapabilityRunner;
    use std::sync::atomic::AtomicBool;

    /// A job with a real workspace read grant so the workdir resolution is
    /// exercised the way the CapabilityHost builds jobs.
    fn job_with(arguments: Value, timeout_secs: u64) -> CapabilityJob {
        let workspace = std::env::temp_dir()
            .canonicalize()
            .unwrap_or_else(|_| std::env::temp_dir());
        let mut job = CapabilityJob::new(CAPABILITY_ID, "job-shell-test", arguments)
            .workspace(workspace.to_string_lossy().as_ref())
            .with_timeout(timeout_secs);
        job.grants = CapabilityGrants {
            read_roots: vec![workspace.to_string_lossy().to_string()],
            exec: true,
            ..CapabilityGrants::default()
        };
        job
    }

    fn noop_control() -> CapabilityControl {
        let progress: Arc<dyn Fn(CapabilityProgress) + Send + Sync> = Arc::new(|_| {});
        CapabilityControl::new(Arc::new(AtomicBool::new(false)), progress)
    }

    fn echo_command() -> &'static str {
        #[cfg(windows)]
        {
            "Write-Output hello"
        }
        #[cfg(not(windows))]
        {
            "echo hello"
        }
    }

    fn exit_three() -> &'static str {
        #[cfg(windows)]
        {
            "exit 3"
        }
        #[cfg(not(windows))]
        {
            "exit 3"
        }
    }

    fn slow_command() -> &'static str {
        #[cfg(windows)]
        {
            "Start-Sleep -Seconds 5"
        }
        #[cfg(not(windows))]
        {
            "sleep 5"
        }
    }

    fn large_output_command() -> &'static str {
        #[cfg(windows)]
        {
            "Write-Output ([string]::new('x', 20000))"
        }
        #[cfg(not(windows))]
        {
            "printf 'x%.0s' $(seq 1 20000)"
        }
    }

    fn output_of(result: &CapabilityResult) -> Value {
        result
            .result
            .clone()
            .expect("completed result carries output")
    }

    #[test]
    fn a_completed_command_reports_its_exit_code_and_output() {
        let result = run_shell_job(
            &job_with(json!({"command": echo_command()}), 30),
            &noop_control(),
        );
        assert_eq!(result.state, CapabilityExitState::Completed);
        let output = output_of(&result);
        assert_eq!(output["exit_code"], json!(0));
        assert_eq!(
            output["stdout"].as_str().unwrap_or_default().trim(),
            "hello"
        );
    }

    #[test]
    fn a_nonzero_exit_is_not_a_tool_failure() {
        let result = run_shell_job(
            &job_with(json!({"command": exit_three()}), 30),
            &noop_control(),
        );
        assert_eq!(result.state, CapabilityExitState::Completed);
        let output = output_of(&result);
        assert_eq!(output["exit_code"], json!(3));
    }

    #[test]
    fn a_job_timeout_terminates_the_child() {
        let result = run_shell_job(
            &job_with(json!({"command": slow_command()}), 1),
            &noop_control(),
        );
        assert_eq!(result.state, CapabilityExitState::TimedOut);
    }

    #[test]
    fn a_missing_command_fails_as_an_argument_error() {
        let result = run_shell_job(&job_with(json!({}), 30), &noop_control());
        assert_eq!(result.state, CapabilityExitState::Failed);
        let diagnostics = result.diagnostics.expect("failure carries diagnostics");
        assert_eq!(diagnostics.error_code.as_deref(), Some("arguments"));
    }

    #[test]
    fn verbose_output_is_capped_and_flagged() {
        let result = run_shell_job(
            &job_with(json!({"command": large_output_command()}), 30),
            &noop_control(),
        );
        assert_eq!(result.state, CapabilityExitState::Completed);
        let output = output_of(&result);
        assert_eq!(output["truncated_stdout"], json!(true));
        let text = output["stdout"].as_str().unwrap_or_default();
        assert!(text.len() <= MAX_OUTPUT_CHARS);
    }

    #[test]
    fn a_workdir_outside_read_grants_is_refused() {
        let outside = if cfg!(windows) { "C:\\Windows" } else { "/etc" };
        let result = run_shell_job(
            &job_with(json!({"command": echo_command(), "workdir": outside}), 30),
            &noop_control(),
        );
        assert_eq!(result.state, CapabilityExitState::Failed);
        let diagnostics = result.diagnostics.expect("failure carries diagnostics");
        assert_eq!(diagnostics.error_code.as_deref(), Some("grant_denied"));
    }

    #[test]
    fn a_cancelled_job_never_reaches_the_runner() {
        let runner = NativeCapabilityRunner::new(run_shell_job);
        let progress: Arc<dyn Fn(CapabilityProgress) + Send + Sync> = Arc::new(|_| {});
        let cancelled = CapabilityControl::new(Arc::new(AtomicBool::new(true)), progress);
        let result = runner.run(
            &job_with(json!({"command": echo_command()}), 30),
            &cancelled,
        );
        assert_eq!(result.state, CapabilityExitState::Cancelled);
    }

    #[test]
    fn the_registration_declares_an_approvable_high_risk_tool() {
        let registration = shell_registration();
        assert_eq!(registration.tool_name, TOOL_NAME);
        assert_eq!(registration.capability_id, CAPABILITY_ID);
        assert!(registration.grants.exec);
        assert_eq!(registration.metadata["risk_level"], json!("high"));
        assert_eq!(registration.metadata["requires_approval"], json!(true));
    }
}
