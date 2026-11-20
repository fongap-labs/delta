//! Wire conversions — explicit Runtime-to-SDK DTO mappings (R8.2).
//!
//! The Rust Runtime (`delta-core`) owns internal models for efficiency
//! and type safety. These `From` implementations convert runtime models
//! to the canonical wire contracts defined in `delta-sdk`. Conversions
//! are lossless: every semantic field maps to its wire counterpart with
//! stable field names and unified time semantics (Unix timestamps, f64).
//!
//! Contract tests in `tests/contract_wire.rs` verify round-trip
//! serialization for every type below.

use crate::capability::{
    CapabilityArtifact, CapabilityBoundary, CapabilityDiagnostics, CapabilityExitState,
    CapabilityGrants, CapabilityInputFile, CapabilityJob, CapabilityProgress, CapabilityResult,
    GrantValidationError,
};

// -- ExitState --

impl From<CapabilityExitState> for delta_sdk::capability::CapabilityExitState {
    fn from(state: CapabilityExitState) -> Self {
        match state {
            CapabilityExitState::Completed => Self::Completed,
            CapabilityExitState::Failed => Self::Failed,
            CapabilityExitState::Cancelled => Self::Cancelled,
            CapabilityExitState::TimedOut => Self::TimedOut,
        }
    }
}

// -- InputFile --

impl From<CapabilityInputFile> for delta_sdk::capability::CapabilityInputFile {
    fn from(file: CapabilityInputFile) -> Self {
        Self {
            path: file.path,
            sha256: file.sha256,
            size: file.size,
        }
    }
}

// -- Grants --

impl From<CapabilityGrants> for delta_sdk::capability::CapabilityGrants {
    fn from(grants: CapabilityGrants) -> Self {
        Self {
            read_roots: grants.read_roots,
            write_roots: grants.write_roots,
            network: grants.network,
            secrets: grants.secrets,
            exec: grants.exec,
            execution_epoch: grants.execution_epoch,
            expires_at: grants.expires_at,
        }
    }
}

// -- Boundary --

impl From<CapabilityBoundary> for delta_sdk::capability::CapabilityBoundary {
    fn from(boundary: CapabilityBoundary) -> Self {
        Self {
            read_roots: boundary.read_roots,
            write_roots: boundary.write_roots,
            network: boundary.network,
            exec: boundary.exec,
            secrets: boundary.secrets,
            destructive: boundary.destructive,
            provenance: boundary.provenance,
            execution_epoch: boundary.execution_epoch,
            expires_at: boundary.expires_at,
        }
    }
}

// -- Progress --

impl From<CapabilityProgress> for delta_sdk::capability::CapabilityProgress {
    fn from(progress: CapabilityProgress) -> Self {
        Self {
            job_id: progress.job_id,
            fraction: progress.fraction,
            stage: progress.stage,
            message: progress.message,
        }
    }
}

// -- Artifact --

impl From<CapabilityArtifact> for delta_sdk::capability::CapabilityArtifact {
    fn from(artifact: CapabilityArtifact) -> Self {
        Self {
            staging_path: artifact.staging_path,
            relative_path: artifact.relative_path,
            kind: artifact.kind,
            sha256: artifact.sha256,
            size: artifact.size,
            incomplete: artifact.incomplete,
        }
    }
}

// -- Diagnostics --

impl From<CapabilityDiagnostics> for delta_sdk::capability::CapabilityDiagnostics {
    fn from(diagnostics: CapabilityDiagnostics) -> Self {
        Self {
            stderr_lines: diagnostics.stderr_lines,
            error_code: diagnostics.error_code,
            error_message: diagnostics.error_message,
        }
    }
}

// -- Result --

impl From<CapabilityResult> for delta_sdk::capability::CapabilityResult {
    fn from(result: CapabilityResult) -> Self {
        Self {
            abi_version: result.abi_version,
            job_id: result.job_id,
            state: result.state.into(),
            result: result.result,
            output: result.output,
            artifacts: result.artifacts.into_iter().map(Into::into).collect(),
            diagnostics: result.diagnostics.map(Into::into),
            finished_at: result.finished_at,
        }
    }
}

// -- Job --

impl From<CapabilityJob> for delta_sdk::capability::CapabilityJob {
    fn from(job: CapabilityJob) -> Self {
        Self {
            abi_version: job.abi_version,
            capability_id: job.capability_id,
            job_id: job.job_id,
            run_id: job.run_id,
            session_id: job.session_id,
            workspace: job.workspace,
            input_files: job.input_files.into_iter().map(Into::into).collect(),
            arguments: job.arguments,
            grants: job.grants.into(),
            boundary: job.boundary.into(),
            timeout_secs: job.timeout_secs,
            artifact_staging_dir: job.artifact_staging_dir,
        }
    }
}

// -- GrantValidationError --

impl From<GrantValidationError> for delta_sdk::capability::GrantValidationError {
    fn from(error: GrantValidationError) -> Self {
        match error {
            GrantValidationError::GrantNotActive => Self::GrantNotActive,
            GrantValidationError::GrantExpired => Self::GrantExpired,
            GrantValidationError::InvalidGrantWindow => Self::InvalidGrantWindow,
            GrantValidationError::GrantInvalid(msg) => Self::GrantInvalid(msg),
        }
    }
}
