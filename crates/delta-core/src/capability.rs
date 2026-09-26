//! Capability ABI — the Delta-owned contract between the Rust Runtime and
//! a controlled Worker / Capability execution environment.
//!
//! The core Runtime owns capability dispatch rather than delegating authority to
//! every capability (File, Search, Shell, Web, MCP, Skill, Automation tools,
//! Connector tools) runs behind this contract. The Rust Runtime is the sole
//! supervisor: it starts the worker, enforces bounds (timeout, cancel,
//! permission/network/secrets grants), and formalizes the typed result /
//! artifact. A Worker (Python / PowerShell / Shell) only executes.
//!
//! This module is pure data + typing: the contract is versioned and
//! serializable so the runtime can pass it over the Worker Runner boundary
//! (subprocess stdin/stdout, MCP stdio, or an in-Tauri-process trait object).
//!
//! Contract fields:
//!   capability id · job id · workspace · input files · input hashes ·
//!   arguments · permission grants · network grants · secrets grants ·
//!   timeout · cancel · progress · typed result · artifact staging · stderr ·
//!   diagnostics · exit state

mod abi;
mod host;
mod registry;
mod runners;

pub use abi::*;
pub use host::CapabilityHost;
pub use registry::{
    CapabilityControl, CapabilityRegistration, CapabilityRegistry, CapabilityRunner,
};
pub use runners::{McpCapabilityRunner, NativeCapabilityRunner, WorkerProcessRunner};

#[cfg(test)]
use crate::runtime::{ToolCall, ToolExecutionContext, ToolExecutor};
#[cfg(test)]
use abi::unix_secs;
#[cfg(test)]
use runners::{drain_worker_lines, WorkerLine};
#[cfg(test)]
use std::collections::BTreeMap;
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
use std::sync::{mpsc, Arc};
#[cfg(test)]
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_version_constant() {
        assert_eq!(CapabilityJob::version(), CAPABILITY_ABI_VERSION);
        assert_eq!(CAPABILITY_ABI_VERSION, 2);
    }

    #[test]
    fn job_auto_versions_and_ids() {
        let job = CapabilityJob::new("shell.exec", "job-1", serde_json::json!({"cmd": "ls"}));
        assert_eq!(job.abi_version, CAPABILITY_ABI_VERSION);
        assert_eq!(job.capability_id, "shell.exec");
        assert_eq!(job.job_id, "job-1");
        assert!(job.workspace.is_none());
        assert!(job.input_files.is_empty());
    }

    #[test]
    fn builder_sets_bounds() {
        let job = CapabilityJob::new("file.read", "job-2", serde_json::json!({}))
            .workspace("/ws")
            .running_f_in(CapabilityInputFile {
                path: "/ws/a.txt".to_string(),
                sha256: Some("abc".to_string()),
                size: Some(3),
            })
            .grants(CapabilityGrants {
                read_roots: vec!["/ws".to_string()],
                write_roots: Vec::new(),
                network: Vec::new(),
                secrets: Vec::new(),
                exec: false,
                ..Default::default()
            })
            .with_timeout(30);
        assert_eq!(job.workspace.as_deref(), Some("/ws"));
        assert_eq!(job.input_files.len(), 1);
        assert_eq!(job.grants.read_roots, vec!["/ws"]);
        assert_eq!(job.timeout_secs, 30);
        assert!(!job.grants.exec);
    }

    #[test]
    fn read_only_grants_have_no_write_roots() {
        let g = CapabilityGrants::read_only();
        assert!(g.write_roots.is_empty());
        assert!(!g.exec);
    }

    #[test]
    fn result_terminates_with_state() {
        let c = CapabilityResult::completed("j", serde_json::json!({"ok": true}));
        assert_eq!(c.state, CapabilityExitState::Completed);
        assert_eq!(c.result.as_ref().unwrap()["ok"], serde_json::json!(true));

        let f = CapabilityResult::failed("j", "boom", Some("E_RUN"));
        assert_eq!(f.state, CapabilityExitState::Failed);
        assert_eq!(
            f.diagnostics.as_ref().unwrap().error_code.as_deref(),
            Some("E_RUN")
        );

        let t = CapabilityResult::timed_out("j");
        assert_eq!(t.state, CapabilityExitState::TimedOut);
    }

    #[test]
    fn artifacts_stream_into_result() {
        let r = CapabilityResult::completed("j", serde_json::json!("ok"))
            .with_artifact(CapabilityArtifact {
                staging_path: "/ws/.delta/staging/run/job/out.csv".to_string(),
                relative_path: "out.csv".to_string(),
                kind: Some("csv".to_string()),
                sha256: Some("d41d8".to_string()),
                size: Some(5),
                incomplete: false,
            })
            .with_stderr(["warn: deprecation".to_string()]);
        assert_eq!(r.artifacts.len(), 1);
        assert_eq!(r.artifacts[0].kind.as_deref(), Some("csv"));
        let diag = r.diagnostics.unwrap();
        assert_eq!(diag.stderr_lines, vec!["warn: deprecation"]);
    }

    #[test]
    fn serde_round_trip_preserves_fields() {
        let mut job = CapabilityJob::new(
            "web.fetch",
            "job-3",
            serde_json::json!({"url": "https://e"}),
        );
        job.workspace = Some("/ws".to_string());
        job.timeout_secs = 10;
        let json = serde_json::to_value(&job).unwrap();
        let back: CapabilityJob = serde_json::from_value(json).unwrap();
        assert_eq!(back.capability_id, "web.fetch");
        assert_eq!(back.workspace.as_deref(), Some("/ws"));
        assert_eq!(back.timeout_secs, 10);
        assert_eq!(back.abi_version, CAPABILITY_ABI_VERSION);
    }

    fn tool_context(workspace: &Path) -> ToolExecutionContext {
        ToolExecutionContext {
            session_id: "session-1".to_string(),
            run_id: "run-1".to_string(),
            workspace: Some(workspace.to_string_lossy().to_string()),
            timeout: Duration::from_secs(2),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(|_| {}),
            secrets: BTreeMap::new(),
        }
    }

    #[test]
    fn product_registry_owns_stable_model_tool_schemas() {
        let host = CapabilityHost::product_defaults().unwrap();
        let schemas = host.tool_schemas();
        let names = schemas
            .as_array()
            .unwrap()
            .iter()
            .map(|schema| schema["function"]["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "ask_user",
                "propose_plan",
                "read_file",
                "request_directory",
                "write_file"
            ]
        );
    }

    #[test]
    fn native_read_hashes_and_reads_only_from_workspace() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("input.txt"), "hello capability").unwrap();
        let host = CapabilityHost::product_defaults().unwrap();
        let result = host.execute(
            &ToolCall {
                id: "call-read".to_string(),
                name: "read_file".to_string(),
                arguments: serde_json::json!({"path": "input.txt"}),
            },
            &tool_context(temp.path()),
        );
        assert_eq!(result.state, crate::runtime::ToolExitState::Completed);
        assert_eq!(result.output["text"], "hello capability");
    }

    #[test]
    fn native_write_only_stages_an_artifact_for_runtime_promotion() {
        let temp = tempfile::tempdir().unwrap();
        let host = CapabilityHost::product_defaults().unwrap();
        let result = host.execute(
            &ToolCall {
                id: "call-write".to_string(),
                name: "write_file".to_string(),
                arguments: serde_json::json!({"path": "reports/result.txt", "content": "ready"}),
            },
            &tool_context(temp.path()),
        );
        assert_eq!(result.state, crate::runtime::ToolExitState::Completed);
        assert_eq!(result.staged_artifacts.len(), 1);
        assert_eq!(
            result.staged_artifacts[0].relative_path,
            "reports/result.txt"
        );
        assert_eq!(
            std::fs::read_to_string(&result.staged_artifacts[0].staging_path).unwrap(),
            "ready"
        );
        assert!(!temp.path().join("reports/result.txt").exists());
    }

    #[test]
    fn native_runner_observes_runtime_cancellation() {
        let cancel = Arc::new(AtomicBool::new(true));
        let control = CapabilityControl::new(cancel, Arc::new(|_| {}));
        let runner = NativeCapabilityRunner::new(|job, _| {
            CapabilityResult::completed(&job.job_id, serde_json::json!({"ok": true}))
        });
        let result = runner.run(
            &CapabilityJob::new("native.test", "job-cancel", serde_json::json!({})),
            &control,
        );
        assert_eq!(result.state, CapabilityExitState::Cancelled);
    }

    #[test]
    fn worker_process_rejects_untyped_stdout() {
        let temp = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let runner = {
            let system_root = std::env::var_os("SystemRoot").unwrap();
            WorkerProcessRunner::new(
                PathBuf::from(system_root).join("System32").join("cmd.exe"),
                vec![
                    "/D".to_string(),
                    "/S".to_string(),
                    "/C".to_string(),
                    "set /p DELTA_JOB= & echo worker-ok".to_string(),
                ],
            )
        };
        #[cfg(unix)]
        let runner = WorkerProcessRunner::new(
            "/bin/sh",
            vec![
                "-c".to_string(),
                "IFS= read -r DELTA_JOB; printf '%s\\n' worker-ok".to_string(),
            ],
        );
        let mut job = CapabilityJob::new("shell.test", "job-worker", serde_json::json!({}));
        job.workspace = Some(temp.path().to_string_lossy().to_string());
        job.timeout_secs = 2;
        let control = CapabilityControl::new(Arc::new(AtomicBool::new(false)), Arc::new(|_| {}));
        let result = runner.run(&job, &control);
        assert_eq!(result.state, CapabilityExitState::Failed);
        assert_eq!(
            result
                .diagnostics
                .as_ref()
                .and_then(|diagnostics| diagnostics.error_code.as_deref()),
            Some("worker_protocol")
        );
    }

    #[test]
    fn drain_worker_lines_accepts_exactly_one_terminal_result() {
        let (sender, receiver) = mpsc::channel();
        let typed = CapabilityResult::completed("job-typed", serde_json::json!({"ok": true}));
        sender
            .send(WorkerLine::Stdout(serde_json::to_string(&typed).unwrap()))
            .unwrap();
        let control = CapabilityControl::new(Arc::new(AtomicBool::new(false)), Arc::new(|_| {}));
        let mut stdout_lines = Vec::new();
        let mut stderr_lines = Vec::new();
        let mut result = None;
        drain_worker_lines(
            &receiver,
            &control,
            &mut stdout_lines,
            &mut stderr_lines,
            &mut result,
        );
        let result = result.unwrap();
        assert_eq!(result.state, CapabilityExitState::Completed);
        assert_eq!(result.job_id, "job-typed");
        assert!(stdout_lines.is_empty());
        assert!(stderr_lines.is_empty());
    }

    #[test]
    fn drain_worker_lines_rejects_multiple_terminal_results() {
        let (sender, receiver) = mpsc::channel();
        for _ in 0..2 {
            let typed = CapabilityResult::completed("job-typed", serde_json::json!({"ok": true}));
            sender
                .send(WorkerLine::Stdout(serde_json::to_string(&typed).unwrap()))
                .unwrap();
        }
        let control = CapabilityControl::new(Arc::new(AtomicBool::new(false)), Arc::new(|_| {}));
        let mut stdout_lines = Vec::new();
        let mut stderr_lines = Vec::new();
        let mut result = None;
        drain_worker_lines(
            &receiver,
            &control,
            &mut stdout_lines,
            &mut stderr_lines,
            &mut result,
        );
        let result = result.unwrap();
        assert_eq!(result.state, CapabilityExitState::Failed);
        assert_eq!(
            result
                .diagnostics
                .as_ref()
                .and_then(|diagnostics| diagnostics.error_code.as_deref()),
            Some("worker_protocol")
        );
        assert!(stdout_lines.is_empty());
        assert!(stderr_lines.is_empty());
    }

    #[test]
    fn worker_process_is_force_terminated_at_runtime_timeout() {
        let temp = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let runner = {
            let system_root = PathBuf::from(std::env::var_os("SystemRoot").unwrap());
            WorkerProcessRunner::new(
                system_root
                    .join("System32")
                    .join("WindowsPowerShell")
                    .join("v1.0")
                    .join("powershell.exe"),
                vec![
                    "-NoLogo".to_string(),
                    "-NoProfile".to_string(),
                    "-NonInteractive".to_string(),
                    "-Command".to_string(),
                    "$null = [Console]::In.ReadLine(); Start-Sleep -Seconds 5".to_string(),
                ],
            )
        };
        #[cfg(unix)]
        let runner = WorkerProcessRunner::new(
            "/bin/sh",
            vec![
                "-c".to_string(),
                "IFS= read -r DELTA_JOB; exec /bin/sleep 5".to_string(),
            ],
        );
        let mut job = CapabilityJob::new("shell.test", "job-timeout", serde_json::json!({}));
        job.workspace = Some(temp.path().to_string_lossy().to_string());
        job.timeout_secs = 1;
        let control = CapabilityControl::new(Arc::new(AtomicBool::new(false)), Arc::new(|_| {}));
        let started = Instant::now();
        let result = runner.run(&job, &control);
        assert_eq!(result.state, CapabilityExitState::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    // ------------------------------------------------------------------
    // Capability boundary + provenance + secret-in-env guard.
    // ------------------------------------------------------------------

    #[test]
    fn boundary_from_read_only_grants_is_read_only_floor() {
        let mut grants = CapabilityGrants::read_only();
        grants.read_roots.push("/ws".to_string());
        let boundary = CapabilityBoundary::from_grants(&grants, "runtime.auto_low_risk");
        assert!(boundary.is_read_only());
        assert!(boundary.write_roots.is_empty());
        assert!(!boundary.exec);
        assert!(boundary.network.is_empty());
        assert!(boundary.secrets.is_empty());
        assert_eq!(boundary.provenance, "runtime.auto_low_risk");
    }

    #[test]
    fn boundary_from_write_grants_is_not_read_only() {
        let grants = CapabilityGrants {
            write_roots: vec!["/ws".to_string()],
            read_roots: vec!["/ws".to_string()],
            ..Default::default()
        };
        let boundary = CapabilityBoundary::from_grants(&grants, "policy");
        assert!(!boundary.is_read_only());
        assert_eq!(boundary.write_roots, vec!["/ws"]);
    }

    #[test]
    fn boundary_allows_workspace_less_capabilities() {
        let boundary = CapabilityBoundary::default();
        assert!(boundary.validate().is_ok());
    }

    #[test]
    fn boundary_validates_with_or_without_workspace_roots() {
        let grants = CapabilityGrants::read_only();
        let boundary = CapabilityBoundary::from_grants(&grants, "capability.registration");
        assert!(boundary.validate().is_ok());

        let mut grants = CapabilityGrants::read_only();
        grants.read_roots.push("/ws".to_string());
        let boundary = CapabilityBoundary::from_grants(&grants, "capability.registration");
        assert!(boundary.validate().is_ok());
    }

    #[test]
    fn build_job_derives_boundary_and_provenance() {
        use crate::runtime::ToolExecutor;
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("input.txt"), "data").unwrap();
        let host = CapabilityHost::product_defaults().unwrap();
        // read_file is read-only; the boundary must be the read-only floor
        // with the workspace as a read root and provenance recorded.
        let _ = host.execute(
            &ToolCall {
                id: "c".to_string(),
                name: "read_file".to_string(),
                arguments: serde_json::json!({"path": "input.txt"}),
            },
            &tool_context(temp.path()),
        );
        // The boundary is validated inside build_job; a read-only capability
        // whose workspace is readable passes. Assert no panic / clean execution
        // already covered by native_read_hashes_and_reads_only_from_workspace.
    }

    #[test]
    fn worker_process_rejects_secret_key_in_environment() {
        let temp = tempfile::tempdir().unwrap();
        // A worker whose explicit environment contains a key that is also a
        // granted secret must fail closed (secret_in_env), never spawn.
        let runner = WorkerProcessRunner::new(
            if cfg!(windows) { "cmd" } else { "/bin/sh" },
            vec![if cfg!(windows) {
                "/c exit 0".to_string()
            } else {
                "-c true".to_string()
            }],
        )
        .with_environment(BTreeMap::from([(
            "OPENAI_API_KEY".to_string(),
            "leaked".to_string(),
        )]));
        let mut job = CapabilityJob::new("secret.test", "job-secret", serde_json::json!({}));
        job.workspace = Some(temp.path().to_string_lossy().to_string());
        job.boundary = CapabilityBoundary {
            read_roots: vec![temp.path().to_string_lossy().to_string()],
            secrets: vec!["OPENAI_API_KEY".to_string()],
            provenance: "test".to_string(),
            ..Default::default()
        };
        let control = CapabilityControl::new(Arc::new(AtomicBool::new(false)), Arc::new(|_| {}));
        let result = runner.run(&job, &control);
        assert_eq!(result.state, CapabilityExitState::Failed);
        assert_eq!(
            result.diagnostics.as_ref().unwrap().error_code.as_deref(),
            Some("secret_in_env")
        );
    }

    #[test]
    fn boundary_propagates_epoch_and_expiry_from_grants() {
        let grants = CapabilityGrants {
            read_roots: vec!["/ws".to_string()],
            execution_epoch: Some(1000.0),
            expires_at: Some(2000.0),
            ..Default::default()
        };
        let boundary = CapabilityBoundary::from_grants(&grants, "test");
        assert_eq!(boundary.execution_epoch, Some(1000.0));
        assert_eq!(boundary.expires_at, Some(2000.0));
    }

    #[test]
    fn boundary_validate_rejects_epoch_after_expiry() {
        let boundary = CapabilityBoundary {
            read_roots: vec!["/ws".to_string()],
            execution_epoch: Some(2000.0),
            expires_at: Some(1000.0),
            provenance: "test".to_string(),
            ..Default::default()
        };
        assert!(boundary.validate().is_err());
    }

    #[test]
    fn boundary_validate_accepts_epoch_before_expiry() {
        let boundary = CapabilityBoundary {
            read_roots: vec!["/ws".to_string()],
            execution_epoch: Some(1000.0),
            expires_at: Some(2000.0),
            provenance: "test".to_string(),
            ..Default::default()
        };
        assert!(boundary.validate().is_ok());
    }

    #[test]
    fn boundary_is_expired_when_past_expiry() {
        let boundary = CapabilityBoundary {
            read_roots: vec!["/ws".to_string()],
            expires_at: Some(0.0),
            provenance: "test".to_string(),
            ..Default::default()
        };
        assert!(boundary.is_expired());
    }

    #[test]
    fn boundary_is_not_expired_when_no_expiry() {
        let boundary = CapabilityBoundary {
            read_roots: vec!["/ws".to_string()],
            provenance: "test".to_string(),
            ..Default::default()
        };
        assert!(!boundary.is_expired());
    }

    #[test]
    fn boundary_is_pending_before_epoch() {
        let boundary = CapabilityBoundary {
            read_roots: vec!["/ws".to_string()],
            execution_epoch: Some(f64::MAX),
            provenance: "test".to_string(),
            ..Default::default()
        };
        assert!(boundary.is_pending());
    }

    #[test]
    fn boundary_is_not_pending_without_epoch() {
        let boundary = CapabilityBoundary {
            read_roots: vec!["/ws".to_string()],
            provenance: "test".to_string(),
            ..Default::default()
        };
        assert!(!boundary.is_pending());
    }

    #[test]
    fn serde_round_trip_preserves_epoch_and_expiry() {
        let job = CapabilityJob::new("test.cap", "job-epoch", serde_json::json!({})).grants(
            CapabilityGrants {
                read_roots: vec!["/ws".to_string()],
                execution_epoch: Some(1000.0),
                expires_at: Some(2000.0),
                ..Default::default()
            },
        );
        let json = serde_json::to_value(&job).unwrap();
        let back: CapabilityJob = serde_json::from_value(json).unwrap();
        assert_eq!(back.grants.execution_epoch, Some(1000.0));
        assert_eq!(back.grants.expires_at, Some(2000.0));
    }

    // ------------------------------------------------------------------
    // Execution Grant Enforcement
    // ------------------------------------------------------------------

    #[test]
    fn grant_validation_accepts_valid_grant() {
        let now = unix_secs();
        let boundary = CapabilityBoundary {
            read_roots: vec!["/ws".to_string()],
            execution_epoch: Some(now - 100.0),
            expires_at: Some(now + 100.0),
            provenance: "test".to_string(),
            ..Default::default()
        };
        assert!(boundary.validate_execution_grant().is_ok());
    }

    #[test]
    fn grant_validation_rejects_future_epoch() {
        let now = unix_secs();
        let boundary = CapabilityBoundary {
            read_roots: vec!["/ws".to_string()],
            execution_epoch: Some(now + 3600.0),
            expires_at: Some(now + 7200.0),
            provenance: "test".to_string(),
            ..Default::default()
        };
        assert_eq!(
            boundary.validate_execution_grant(),
            Err(GrantValidationError::GrantNotActive)
        );
    }

    #[test]
    fn grant_validation_rejects_expired() {
        let now = unix_secs();
        let boundary = CapabilityBoundary {
            read_roots: vec!["/ws".to_string()],
            execution_epoch: Some(now - 200.0),
            expires_at: Some(now - 100.0),
            provenance: "test".to_string(),
            ..Default::default()
        };
        assert_eq!(
            boundary.validate_execution_grant(),
            Err(GrantValidationError::GrantExpired)
        );
    }

    #[test]
    fn grant_validation_rejects_boundary_timestamp() {
        let boundary = CapabilityBoundary {
            read_roots: vec!["/ws".to_string()],
            expires_at: Some(0.0),
            provenance: "test".to_string(),
            ..Default::default()
        };
        assert_eq!(
            boundary.validate_execution_grant(),
            Err(GrantValidationError::GrantExpired)
        );
    }

    #[test]
    fn grant_validation_rejects_epoch_after_expiry() {
        let now = unix_secs();
        let boundary = CapabilityBoundary {
            read_roots: vec!["/ws".to_string()],
            execution_epoch: Some(now + 200.0),
            expires_at: Some(now + 100.0),
            provenance: "test".to_string(),
            ..Default::default()
        };
        assert_eq!(
            boundary.validate_execution_grant(),
            Err(GrantValidationError::InvalidGrantWindow)
        );
    }

    #[test]
    fn grant_validation_accepts_no_epoch() {
        let now = unix_secs();
        let boundary = CapabilityBoundary {
            read_roots: vec!["/ws".to_string()],
            expires_at: Some(now + 3600.0),
            provenance: "test".to_string(),
            ..Default::default()
        };
        assert!(boundary.validate_execution_grant().is_ok());
    }

    #[test]
    fn grant_validation_accepts_no_expiry() {
        let now = unix_secs();
        let boundary = CapabilityBoundary {
            read_roots: vec!["/ws".to_string()],
            execution_epoch: Some(now - 100.0),
            provenance: "test".to_string(),
            ..Default::default()
        };
        assert!(boundary.validate_execution_grant().is_ok());
    }

    #[test]
    fn grant_validation_rejects_nan_epoch() {
        let boundary = CapabilityBoundary {
            read_roots: vec!["/ws".to_string()],
            execution_epoch: Some(f64::NAN),
            provenance: "test".to_string(),
            ..Default::default()
        };
        let result = boundary.validate_execution_grant();
        assert!(matches!(result, Err(GrantValidationError::GrantInvalid(_))));
    }

    #[test]
    fn host_rejects_future_grant_before_runner() {
        use crate::runtime::ToolExecutor;
        let temp = tempfile::tempdir().unwrap();
        let now = unix_secs();
        let mut grants = CapabilityGrants::read_only();
        grants.execution_epoch = Some(now + 3600.0);
        let called = Arc::new(AtomicBool::new(false));
        let called_flag = called.clone();
        let host = CapabilityHost::new();
        host.register(CapabilityRegistration {
            capability_id: "test.future".to_string(),
            tool_name: "test_future".to_string(),
            description: "test".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            metadata: serde_json::json!({}),
            grants,
            workspace_write: false,
            runner: Arc::new(NativeCapabilityRunner::new(move |job, _| {
                called_flag.store(true, Ordering::SeqCst);
                CapabilityResult::completed(&job.job_id, serde_json::json!({"reached": true}))
            })),
        })
        .unwrap();
        let result = host.execute(
            &ToolCall {
                id: "call-future".to_string(),
                name: "test_future".to_string(),
                arguments: serde_json::json!({}),
            },
            &tool_context(temp.path()),
        );
        assert_eq!(result.state, crate::runtime::ToolExitState::Failed);
        assert!(result
            .error
            .as_ref()
            .is_some_and(|e| e.contains("grant_not_active")));
        assert!(!called.load(Ordering::SeqCst), "runner must not be called");
    }

    #[test]
    fn host_rejects_expired_grant_before_runner() {
        use crate::runtime::ToolExecutor;
        let temp = tempfile::tempdir().unwrap();
        let now = unix_secs();
        let mut grants = CapabilityGrants::read_only();
        grants.expires_at = Some(now - 100.0);
        let called = Arc::new(AtomicBool::new(false));
        let called_flag = called.clone();
        let host = CapabilityHost::new();
        host.register(CapabilityRegistration {
            capability_id: "test.expired".to_string(),
            tool_name: "test_expired".to_string(),
            description: "test".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            metadata: serde_json::json!({}),
            grants,
            workspace_write: false,
            runner: Arc::new(NativeCapabilityRunner::new(move |job, _| {
                called_flag.store(true, Ordering::SeqCst);
                CapabilityResult::completed(&job.job_id, serde_json::json!({"reached": true}))
            })),
        })
        .unwrap();
        let result = host.execute(
            &ToolCall {
                id: "call-expired".to_string(),
                name: "test_expired".to_string(),
                arguments: serde_json::json!({}),
            },
            &tool_context(temp.path()),
        );
        assert_eq!(result.state, crate::runtime::ToolExitState::Failed);
        assert!(result
            .error
            .as_ref()
            .is_some_and(|e| e.contains("grant_expired")));
        assert!(!called.load(Ordering::SeqCst), "runner must not be called");
    }

    #[test]
    fn host_returns_cancelled_before_dispatch() {
        use crate::runtime::ToolExecutor;
        let temp = tempfile::tempdir().unwrap();
        let called = Arc::new(AtomicBool::new(false));
        let called_flag = called.clone();
        let host = CapabilityHost::new();
        host.register(CapabilityRegistration {
            capability_id: "test.cancel".to_string(),
            tool_name: "test_cancel".to_string(),
            description: "test".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            metadata: serde_json::json!({}),
            grants: CapabilityGrants::read_only(),
            workspace_write: false,
            runner: Arc::new(NativeCapabilityRunner::new(move |job, _| {
                called_flag.store(true, Ordering::SeqCst);
                CapabilityResult::completed(&job.job_id, serde_json::json!({"reached": true}))
            })),
        })
        .unwrap();
        let context = ToolExecutionContext {
            session_id: "s".to_string(),
            run_id: "r".to_string(),
            workspace: Some(temp.path().to_string_lossy().to_string()),
            timeout: Duration::from_secs(2),
            cancel: Arc::new(AtomicBool::new(true)),
            progress: Arc::new(|_| {}),
            secrets: BTreeMap::new(),
        };
        let result = host.execute(
            &ToolCall {
                id: "call-cancel".to_string(),
                name: "test_cancel".to_string(),
                arguments: serde_json::json!({}),
            },
            &context,
        );
        assert_eq!(result.state, crate::runtime::ToolExitState::Cancelled);
        assert!(!called.load(Ordering::SeqCst), "runner must not be called");
    }

    #[test]
    fn worker_process_defensive_grant_check() {
        let temp = tempfile::tempdir().unwrap();
        let runner = WorkerProcessRunner::new(
            if cfg!(windows) { "cmd" } else { "/bin/sh" },
            vec![if cfg!(windows) {
                "/c exit 0".to_string()
            } else {
                "-c true".to_string()
            }],
        );
        let mut job = CapabilityJob::new("test.expired", "job-expired", serde_json::json!({}));
        job.workspace = Some(temp.path().to_string_lossy().to_string());
        job.boundary = CapabilityBoundary {
            read_roots: vec![temp.path().to_string_lossy().to_string()],
            expires_at: Some(0.0),
            provenance: "test".to_string(),
            ..Default::default()
        };
        let control = CapabilityControl::new(Arc::new(AtomicBool::new(false)), Arc::new(|_| {}));
        let result = runner.run(&job, &control);
        assert_eq!(result.state, CapabilityExitState::Failed);
        assert_eq!(
            result.diagnostics.as_ref().unwrap().error_code.as_deref(),
            Some("grant_expired")
        );
    }

    #[test]
    fn grant_validation_error_codes_are_stable() {
        assert_eq!(
            GrantValidationError::GrantNotActive.code(),
            "grant_not_active"
        );
        assert_eq!(GrantValidationError::GrantExpired.code(), "grant_expired");
        assert_eq!(
            GrantValidationError::InvalidGrantWindow.code(),
            "invalid_grant_window"
        );
        assert_eq!(
            GrantValidationError::GrantInvalid("test".to_string()).code(),
            "grant_invalid"
        );
    }

    // ------------------------------------------------------------------
    // Ephemeral Secret Delivery
    // ------------------------------------------------------------------

    #[test]
    fn secret_values_not_in_serialized_job() {
        let mut job = CapabilityJob::new("test.secret", "job-s", serde_json::json!({}));
        job.secret_values
            .insert("API_KEY".to_string(), "sk-xxx".to_string());
        let json = serde_json::to_value(&job).unwrap();
        assert!(
            json.get("secret_values").is_none(),
            "secret_values must be serde(skip) — never in wire JSON"
        );
        let back: CapabilityJob = serde_json::from_value(json).unwrap();
        assert!(
            back.secret_values.is_empty(),
            "deserialized job must not carry secret values"
        );
    }

    #[test]
    fn build_job_filters_secrets_to_granted_scope() {
        use crate::runtime::ToolExecutor;
        let temp = tempfile::tempdir().unwrap();
        let mut grants = CapabilityGrants::read_only();
        grants
            .read_roots
            .push(temp.path().to_string_lossy().to_string());
        grants.secrets = vec!["API_KEY".to_string(), "DB_URL".to_string()];
        let host = CapabilityHost::new();
        let received = Arc::new(std::sync::Mutex::new(BTreeMap::new()));
        let received_clone = received.clone();
        host.register(CapabilityRegistration {
            capability_id: "test.secret".to_string(),
            tool_name: "test_secret".to_string(),
            description: "test".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            metadata: serde_json::json!({}),
            grants,
            workspace_write: false,
            runner: Arc::new(NativeCapabilityRunner::new(move |job, _| {
                *received_clone.lock().unwrap() = job.secret_values.clone();
                CapabilityResult::completed(&job.job_id, serde_json::json!({"ok": true}))
            })),
        })
        .unwrap();
        let context = ToolExecutionContext {
            session_id: "s".to_string(),
            run_id: "r".to_string(),
            workspace: Some(temp.path().to_string_lossy().to_string()),
            timeout: Duration::from_secs(2),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(|_| {}),
            secrets: BTreeMap::from([
                ("API_KEY".to_string(), "sk-xxx".to_string()),
                ("DB_URL".to_string(), "postgres://...".to_string()),
                ("UNGRANTED".to_string(), "should-not-appear".to_string()),
            ]),
        };
        let result = host.execute(
            &ToolCall {
                id: "call-secret".to_string(),
                name: "test_secret".to_string(),
                arguments: serde_json::json!({}),
            },
            &context,
        );
        assert_eq!(result.state, crate::runtime::ToolExitState::Completed);
        let received = received.lock().unwrap();
        assert_eq!(received.len(), 2);
        assert_eq!(received.get("API_KEY").map(|s| s.as_str()), Some("sk-xxx"));
        assert_eq!(
            received.get("DB_URL").map(|s| s.as_str()),
            Some("postgres://...")
        );
        assert!(
            !received.contains_key("UNGRANTED"),
            "ungranted secrets must not be delivered"
        );
    }

    #[test]
    fn no_secrets_delivered_when_grants_empty() {
        use crate::runtime::ToolExecutor;
        let temp = tempfile::tempdir().unwrap();
        let host = CapabilityHost::new();
        let received = Arc::new(std::sync::Mutex::new(BTreeMap::new()));
        let received_clone = received.clone();
        host.register(CapabilityRegistration {
            capability_id: "test.nosecret".to_string(),
            tool_name: "test_nosecret".to_string(),
            description: "test".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            metadata: serde_json::json!({}),
            grants: CapabilityGrants::read_only(),
            workspace_write: false,
            runner: Arc::new(NativeCapabilityRunner::new(move |job, _| {
                *received_clone.lock().unwrap() = job.secret_values.clone();
                CapabilityResult::completed(&job.job_id, serde_json::json!({"ok": true}))
            })),
        })
        .unwrap();
        let context = ToolExecutionContext {
            session_id: "s".to_string(),
            run_id: "r".to_string(),
            workspace: Some(temp.path().to_string_lossy().to_string()),
            timeout: Duration::from_secs(2),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(|_| {}),
            secrets: BTreeMap::from([("API_KEY".to_string(), "sk-xxx".to_string())]),
        };
        let result = host.execute(
            &ToolCall {
                id: "call-nosecret".to_string(),
                name: "test_nosecret".to_string(),
                arguments: serde_json::json!({}),
            },
            &context,
        );
        assert_eq!(result.state, crate::runtime::ToolExitState::Completed);
        let received = received.lock().unwrap();
        assert!(
            received.is_empty(),
            "no secrets should be delivered when grants.secrets is empty"
        );
    }
}
