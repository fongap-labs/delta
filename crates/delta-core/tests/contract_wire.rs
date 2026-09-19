//! Contract tests — verify wire DTO serialization round-trips and
//! Runtime-to-wire conversions are lossless (R8.2).
//!
//! These tests prove that the canonical contract types defined in
//! `delta-sdk` serialize and deserialize correctly, and that the
//! `From` conversions in `delta_core::wire` preserve all semantic fields.

use delta_core::capability::{
    CapabilityArtifact, CapabilityBoundary, CapabilityExitState, CapabilityGrants, CapabilityJob,
    CapabilityResult, CAPABILITY_ABI_VERSION,
};
use delta_sdk::capability as wire;

#[test]
fn abi_version_matches_across_crates() {
    assert_eq!(CAPABILITY_ABI_VERSION, wire::CAPABILITY_ABI_VERSION);
    assert_eq!(CAPABILITY_ABI_VERSION, 2);
}

#[test]
fn exit_state_serde_round_trip() {
    for state in [
        wire::CapabilityExitState::Completed,
        wire::CapabilityExitState::Failed,
        wire::CapabilityExitState::Cancelled,
        wire::CapabilityExitState::TimedOut,
    ] {
        let json = serde_json::to_value(state).unwrap();
        let back: wire::CapabilityExitState = serde_json::from_value(json).unwrap();
        assert_eq!(state, back);
    }
}

#[test]
fn exit_state_wire_names_are_stable() {
    assert_eq!(
        serde_json::to_value(wire::CapabilityExitState::Completed).unwrap(),
        serde_json::json!("completed")
    );
    assert_eq!(
        serde_json::to_value(wire::CapabilityExitState::Failed).unwrap(),
        serde_json::json!("failed")
    );
    assert_eq!(
        serde_json::to_value(wire::CapabilityExitState::Cancelled).unwrap(),
        serde_json::json!("cancelled")
    );
    assert_eq!(
        serde_json::to_value(wire::CapabilityExitState::TimedOut).unwrap(),
        serde_json::json!("timed_out")
    );
}

#[test]
fn grants_serde_round_trip_preserves_epoch_and_expiry() {
    let grants = wire::CapabilityGrants {
        read_roots: vec!["/ws".to_string()],
        execution_epoch: Some(1000.0),
        expires_at: Some(2000.0),
        ..Default::default()
    };
    let json = serde_json::to_value(&grants).unwrap();
    let back: wire::CapabilityGrants = serde_json::from_value(json).unwrap();
    assert_eq!(back.execution_epoch, Some(1000.0));
    assert_eq!(back.expires_at, Some(2000.0));
    assert_eq!(back.read_roots, vec!["/ws"]);
}

#[test]
fn boundary_serde_round_trip_preserves_all_fields() {
    let boundary = wire::CapabilityBoundary {
        read_roots: vec!["/ws".to_string()],
        write_roots: vec!["/ws/out".to_string()],
        network: vec!["https://api.example.com:443".to_string()],
        exec: true,
        secrets: vec!["API_KEY".to_string()],
        destructive: true,
        provenance: "policy.approved".to_string(),
        execution_epoch: Some(1000.0),
        expires_at: Some(2000.0),
    };
    let json = serde_json::to_value(&boundary).unwrap();
    let back: wire::CapabilityBoundary = serde_json::from_value(json).unwrap();
    assert_eq!(back.read_roots, boundary.read_roots);
    assert_eq!(back.write_roots, boundary.write_roots);
    assert_eq!(back.network, boundary.network);
    assert!(back.exec);
    assert_eq!(back.secrets, boundary.secrets);
    assert!(back.destructive);
    assert_eq!(back.provenance, "policy.approved");
    assert_eq!(back.execution_epoch, Some(1000.0));
    assert_eq!(back.expires_at, Some(2000.0));
}

#[test]
fn progress_serde_round_trip() {
    let progress = wire::CapabilityProgress {
        job_id: "job-1".to_string(),
        fraction: 0.5,
        stage: Some("processing".to_string()),
        message: Some("half done".to_string()),
    };
    let json = serde_json::to_value(&progress).unwrap();
    let back: wire::CapabilityProgress = serde_json::from_value(json).unwrap();
    assert_eq!(back.job_id, "job-1");
    assert!((back.fraction - 0.5).abs() < 1e-10);
    assert_eq!(back.stage.as_deref(), Some("processing"));
    assert_eq!(back.message.as_deref(), Some("half done"));
}

#[test]
fn artifact_serde_round_trip() {
    let artifact = wire::CapabilityArtifact {
        staging_path: "/ws/.delta/staging/job/out.csv".to_string(),
        relative_path: "out.csv".to_string(),
        kind: Some("csv".to_string()),
        sha256: Some("abc123".to_string()),
        size: Some(42),
        incomplete: false,
    };
    let json = serde_json::to_value(&artifact).unwrap();
    let back: wire::CapabilityArtifact = serde_json::from_value(json).unwrap();
    assert_eq!(back.staging_path, artifact.staging_path);
    assert_eq!(back.relative_path, "out.csv");
    assert_eq!(back.kind.as_deref(), Some("csv"));
    assert_eq!(back.sha256.as_deref(), Some("abc123"));
    assert_eq!(back.size, Some(42));
    assert!(!back.incomplete);
}

#[test]
fn diagnostics_serde_round_trip() {
    let diag = wire::CapabilityDiagnostics {
        stderr_lines: vec!["warn: deprecated".to_string()],
        error_code: Some("E_RUN".to_string()),
        error_message: Some("execution failed".to_string()),
    };
    let json = serde_json::to_value(&diag).unwrap();
    let back: wire::CapabilityDiagnostics = serde_json::from_value(json).unwrap();
    assert_eq!(back.stderr_lines, vec!["warn: deprecated"]);
    assert_eq!(back.error_code.as_deref(), Some("E_RUN"));
    assert_eq!(back.error_message.as_deref(), Some("execution failed"));
}

#[test]
fn result_serde_round_trip_preserves_state_and_artifacts() {
    let result = wire::CapabilityResult {
        abi_version: wire::CAPABILITY_ABI_VERSION,
        job_id: "job-r1".to_string(),
        state: wire::CapabilityExitState::Completed,
        result: Some(serde_json::json!({"ok": true})),
        output: None,
        artifacts: vec![wire::CapabilityArtifact {
            staging_path: "/staging/out.csv".to_string(),
            relative_path: "out.csv".to_string(),
            kind: Some("csv".to_string()),
            sha256: None,
            size: None,
            incomplete: false,
        }],
        diagnostics: None,
        finished_at: 1700000000.0,
    };
    let json = serde_json::to_value(&result).unwrap();
    let back: wire::CapabilityResult = serde_json::from_value(json).unwrap();
    assert_eq!(back.abi_version, wire::CAPABILITY_ABI_VERSION);
    assert_eq!(back.job_id, "job-r1");
    assert_eq!(back.state, wire::CapabilityExitState::Completed);
    assert_eq!(back.result, Some(serde_json::json!({"ok": true})));
    assert_eq!(back.artifacts.len(), 1);
    assert_eq!(back.artifacts[0].kind.as_deref(), Some("csv"));
    assert!((back.finished_at - 1700000000.0).abs() < 1e-10);
}

#[test]
fn job_serde_round_trip_preserves_all_fields() {
    let job = wire::CapabilityJob {
        abi_version: wire::CAPABILITY_ABI_VERSION,
        capability_id: "file.read".to_string(),
        job_id: "job-j1".to_string(),
        run_id: Some("run-1".to_string()),
        session_id: Some("session-1".to_string()),
        workspace: Some("/ws".to_string()),
        input_files: vec![wire::CapabilityInputFile {
            path: "/ws/a.txt".to_string(),
            sha256: Some("abc".to_string()),
            size: Some(3),
        }],
        arguments: serde_json::json!({"path": "a.txt"}),
        grants: wire::CapabilityGrants {
            read_roots: vec!["/ws".to_string()],
            ..Default::default()
        },
        boundary: wire::CapabilityBoundary {
            read_roots: vec!["/ws".to_string()],
            provenance: "capability.registration".to_string(),
            ..Default::default()
        },
        timeout_secs: 30,
        artifact_staging_dir: Some("/ws/.delta/staging/run/job".to_string()),
    };
    let json = serde_json::to_value(&job).unwrap();
    let back: wire::CapabilityJob = serde_json::from_value(json).unwrap();
    assert_eq!(back.abi_version, wire::CAPABILITY_ABI_VERSION);
    assert_eq!(back.capability_id, "file.read");
    assert_eq!(back.job_id, "job-j1");
    assert_eq!(back.run_id.as_deref(), Some("run-1"));
    assert_eq!(back.workspace.as_deref(), Some("/ws"));
    assert_eq!(back.input_files.len(), 1);
    assert_eq!(back.timeout_secs, 30);
    assert_eq!(back.grants.read_roots, vec!["/ws"]);
    assert_eq!(back.boundary.provenance, "capability.registration");
}

#[test]
fn worker_manifest_matches_schema_shape() {
    let manifest = delta_sdk::worker::WorkerManifest {
        worker_id: "python-stats".to_string(),
        capabilities: vec![delta_sdk::worker::ManifestCapability {
            capability_id: "research.statistics.anova".to_string(),
            permissions: vec!["filesystem.read".to_string()],
            timeout_seconds: Some(60),
        }],
    };
    let json = serde_json::to_value(&manifest).unwrap();
    assert_eq!(json["worker_id"], "python-stats");
    assert_eq!(
        json["capabilities"][0]["capability_id"],
        "research.statistics.anova"
    );
    assert_eq!(json["capabilities"][0]["permissions"][0], "filesystem.read");
    assert_eq!(json["capabilities"][0]["timeout_seconds"], 60);
}

#[test]
fn execution_grant_serde_round_trip() {
    let grant = wire::ExecutionGrant {
        run_id: "run_abc".to_string(),
        execution_epoch: 1700000000.0,
        capability_id: "file.read".to_string(),
        permission_scope: vec!["filesystem.read".to_string()],
        expires_at: 1700003600.0,
    };
    let json = serde_json::to_value(&grant).unwrap();
    let back: wire::ExecutionGrant = serde_json::from_value(json).unwrap();
    assert_eq!(back.run_id, "run_abc");
    assert!((back.execution_epoch - 1700000000.0).abs() < 1e-10);
    assert_eq!(back.capability_id, "file.read");
    assert_eq!(back.permission_scope, vec!["filesystem.read"]);
    assert!((back.expires_at - 1700003600.0).abs() < 1e-10);
}

#[test]
fn grant_validation_error_serde_round_trip() {
    for variant in [
        wire::GrantValidationError::GrantNotActive,
        wire::GrantValidationError::GrantExpired,
        wire::GrantValidationError::InvalidGrantWindow,
    ] {
        let json = serde_json::to_value(&variant).unwrap();
        let back: wire::GrantValidationError = serde_json::from_value(json).unwrap();
        assert_eq!(back.code(), variant.code());
    }
    let invalid = wire::GrantValidationError::GrantInvalid("bad timestamp".to_string());
    let json = serde_json::to_value(&invalid).unwrap();
    let back: wire::GrantValidationError = serde_json::from_value(json).unwrap();
    assert_eq!(back.code(), "grant_invalid");
}

// -- Runtime-to-wire conversion tests --

#[test]
fn runtime_result_converts_to_wire_losslessly() {
    let runtime_result = CapabilityResult::completed("job-conv", serde_json::json!({"ok": true}))
        .with_artifact(CapabilityArtifact {
            staging_path: "/staging/out.csv".to_string(),
            relative_path: "out.csv".to_string(),
            kind: Some("csv".to_string()),
            sha256: None,
            size: None,
            incomplete: false,
        })
        .with_stderr(["warn: test".to_string()]);

    let wire_result: wire::CapabilityResult = runtime_result.into();
    assert_eq!(wire_result.abi_version, CAPABILITY_ABI_VERSION);
    assert_eq!(wire_result.job_id, "job-conv");
    assert_eq!(wire_result.state, wire::CapabilityExitState::Completed);
    assert_eq!(wire_result.result, Some(serde_json::json!({"ok": true})));
    assert_eq!(wire_result.artifacts.len(), 1);
    assert_eq!(wire_result.artifacts[0].kind.as_deref(), Some("csv"));
    assert_eq!(
        wire_result.diagnostics.as_ref().unwrap().stderr_lines,
        vec!["warn: test"]
    );
}

#[test]
fn runtime_job_converts_to_wire_losslessly() {
    let mut runtime_job = CapabilityJob::new(
        "file.read",
        "job-conv-2",
        serde_json::json!({"path": "a.txt"}),
    )
    .workspace("/ws")
    .grants(CapabilityGrants {
        read_roots: vec!["/ws".to_string()],
        execution_epoch: Some(1000.0),
        expires_at: Some(2000.0),
        ..Default::default()
    })
    .with_timeout(30);
    runtime_job.run_id = Some("run-conv".to_string());
    runtime_job.boundary =
        delta_core::capability::CapabilityBoundary::from_grants(&runtime_job.grants, "test");

    let wire_job: wire::CapabilityJob = runtime_job.into();
    assert_eq!(wire_job.abi_version, CAPABILITY_ABI_VERSION);
    assert_eq!(wire_job.capability_id, "file.read");
    assert_eq!(wire_job.job_id, "job-conv-2");
    assert_eq!(wire_job.run_id.as_deref(), Some("run-conv"));
    assert_eq!(wire_job.workspace.as_deref(), Some("/ws"));
    assert_eq!(wire_job.timeout_secs, 30);
    assert_eq!(wire_job.grants.execution_epoch, Some(1000.0));
    assert_eq!(wire_job.grants.expires_at, Some(2000.0));
    assert_eq!(wire_job.boundary.execution_epoch, Some(1000.0));
    assert_eq!(wire_job.boundary.expires_at, Some(2000.0));
}

#[test]
fn runtime_boundary_converts_to_wire_losslessly() {
    let runtime_boundary = CapabilityBoundary {
        read_roots: vec!["/ws".to_string()],
        write_roots: vec!["/ws/out".to_string()],
        network: vec!["https://api.example.com:443".to_string()],
        exec: true,
        secrets: vec!["API_KEY".to_string()],
        destructive: true,
        provenance: "policy.approved".to_string(),
        execution_epoch: Some(1000.0),
        expires_at: Some(2000.0),
    };

    let wire_boundary: wire::CapabilityBoundary = runtime_boundary.into();
    assert_eq!(wire_boundary.read_roots, vec!["/ws"]);
    assert_eq!(wire_boundary.write_roots, vec!["/ws/out"]);
    assert_eq!(wire_boundary.network, vec!["https://api.example.com:443"]);
    assert!(wire_boundary.exec);
    assert_eq!(wire_boundary.secrets, vec!["API_KEY"]);
    assert!(wire_boundary.destructive);
    assert_eq!(wire_boundary.provenance, "policy.approved");
    assert_eq!(wire_boundary.execution_epoch, Some(1000.0));
    assert_eq!(wire_boundary.expires_at, Some(2000.0));
}

#[test]
fn runtime_exit_state_converts_to_wire() {
    assert_eq!(
        wire::CapabilityExitState::from(CapabilityExitState::Completed),
        wire::CapabilityExitState::Completed
    );
    assert_eq!(
        wire::CapabilityExitState::from(CapabilityExitState::Failed),
        wire::CapabilityExitState::Failed
    );
    assert_eq!(
        wire::CapabilityExitState::from(CapabilityExitState::Cancelled),
        wire::CapabilityExitState::Cancelled
    );
    assert_eq!(
        wire::CapabilityExitState::from(CapabilityExitState::TimedOut),
        wire::CapabilityExitState::TimedOut
    );
}

#[test]
fn artifact_submission_serde_round_trip() {
    let submission = delta_sdk::artifact::ArtifactSubmission {
        run_id: "run-1".to_string(),
        staging_path: "/staging/out.csv".to_string(),
        relative_path: "out.csv".to_string(),
        kind: Some("csv".to_string()),
        sha256: Some("abc123".to_string()),
        size: Some(42),
        incomplete: false,
    };
    let json = serde_json::to_value(&submission).unwrap();
    let back: delta_sdk::artifact::ArtifactSubmission = serde_json::from_value(json).unwrap();
    assert_eq!(back.run_id, "run-1");
    assert_eq!(back.staging_path, "/staging/out.csv");
    assert_eq!(back.relative_path, "out.csv");
    assert_eq!(back.kind.as_deref(), Some("csv"));
    assert_eq!(back.sha256.as_deref(), Some("abc123"));
    assert_eq!(back.size, Some(42));
    assert!(!back.incomplete);
}

#[test]
fn artifact_record_serde_round_trip() {
    let record = delta_sdk::artifact::ArtifactRecord {
        artifact_id: "art-1".to_string(),
        run_id: "run-1".to_string(),
        relative_path: "out.csv".to_string(),
        kind: "csv".to_string(),
        sha256: "abc123".to_string(),
        size: 42,
        registered_at: "2026-09-17T12:00:00Z".to_string(),
    };
    let json = serde_json::to_value(&record).unwrap();
    let back: delta_sdk::artifact::ArtifactRecord = serde_json::from_value(json).unwrap();
    assert_eq!(back.artifact_id, "art-1");
    assert_eq!(back.kind, "csv");
    assert_eq!(back.sha256, "abc123");
    assert_eq!(back.size, 42);
}
