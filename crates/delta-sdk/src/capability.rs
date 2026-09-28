//! Capability ABI — canonical wire contracts (R8.2).
//!
//! These types are the single canonical definition of capability-related
//! data shapes that cross process or repository boundaries. The Rust
//! Runtime (`delta-core`) owns internal runtime models; explicit
//! conversions exist from runtime models to these wire DTOs.
//!
//! Contract stability rules:
//! - One concept = one canonical contract.
//! - Field names are stable across Runtime, SDK, Suite, and Worker protocol.
//! - `execution_epoch` and `expires_at` are Unix timestamps (seconds, f64).
//! - Permission / Grant semantics are unified.
//! - Result / Error / Artifact semantics are unified.

use serde::{Deserialize, Serialize};

/// The Capability ABI wire version. Must match `delta_core::CAPABILITY_ABI_VERSION`.
pub const CAPABILITY_ABI_VERSION: u32 = 2;

/// Terminal state of a capability job (wire enum, matches Runtime enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityExitState {
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "timed_out")]
    TimedOut,
}

/// A file the capability is permitted to read (wire, matches Runtime type).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityInputFile {
    pub path: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

/// Permission grants scoped to a single job (wire, matches Runtime type).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityGrants {
    pub read_roots: Vec<String>,
    pub write_roots: Vec<String>,
    pub network: Vec<String>,
    pub secrets: Vec<String>,
    pub exec: bool,
    /// Unix timestamp (seconds, fractional) when the grant becomes active.
    /// `None` = immediately active.
    #[serde(default)]
    pub execution_epoch: Option<f64>,
    /// Unix timestamp (seconds, fractional) when the grant expires.
    /// `None` = no expiry.
    #[serde(default)]
    pub expires_at: Option<f64>,
}

/// The effective, auditable resource boundary derived from grants (wire).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityBoundary {
    pub read_roots: Vec<String>,
    pub write_roots: Vec<String>,
    pub network: Vec<String>,
    pub exec: bool,
    pub secrets: Vec<String>,
    pub destructive: bool,
    pub provenance: String,
    #[serde(default)]
    pub execution_epoch: Option<f64>,
    #[serde(default)]
    pub expires_at: Option<f64>,
}

/// Execution Grant — the temporal authorization contract that binds a
/// capability invocation to a run and an execution window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionGrant {
    pub run_id: String,
    /// Unix timestamp (seconds, fractional) when the grant becomes active.
    pub execution_epoch: f64,
    pub capability_id: String,
    pub permission_scope: Vec<String>,
    /// Unix timestamp (seconds, fractional) when the grant expires.
    pub expires_at: f64,
}

/// Progress frame emitted during execution (wire, matches Runtime type).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityProgress {
    pub job_id: String,
    /// 0.0..=1.0 coarse progress, or -1 for indeterminate.
    pub fraction: f64,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

/// A staged artifact produced by a job, awaiting Runtime formalization
/// (wire, matches Runtime type).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityArtifact {
    pub staging_path: String,
    pub relative_path: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub incomplete: bool,
}

/// Structured diagnostics (wire, matches Runtime type).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityDiagnostics {
    #[serde(default)]
    pub stderr_lines: Vec<String>,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
}

/// The typed result a Worker returns after a job terminates (wire).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityResult {
    pub abi_version: u32,
    pub job_id: String,
    pub state: CapabilityExitState,
    /// Typed output (parsed), or null when untyped.
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    /// Text output (stdout) when the capability emits one.
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub artifacts: Vec<CapabilityArtifact>,
    #[serde(default)]
    pub diagnostics: Option<CapabilityDiagnostics>,
    pub finished_at: f64,
}

/// The full capability invocation contract sent to a Worker Runner (wire).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityJob {
    pub abi_version: u32,
    pub capability_id: String,
    pub job_id: String,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub input_files: Vec<CapabilityInputFile>,
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub grants: CapabilityGrants,
    #[serde(default)]
    pub boundary: CapabilityBoundary,
    #[serde(default)]
    pub timeout_secs: u64,
    #[serde(default)]
    pub artifact_staging_dir: Option<String>,
}

/// Public API request — the high-level request submitted by an extension
/// to invoke a capability. This is distinct from `CapabilityJob` (the
/// full execution contract built by the Runtime from a request).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRequest {
    pub run_id: String,
    pub capability_id: String,
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub deadline: Option<String>,
}

/// Stable error codes for execution grant validation (wire, matches Runtime).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantValidationError {
    GrantNotActive,
    GrantExpired,
    InvalidGrantWindow,
    GrantInvalid(String),
}

impl GrantValidationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::GrantNotActive => "grant_not_active",
            Self::GrantExpired => "grant_expired",
            Self::InvalidGrantWindow => "invalid_grant_window",
            Self::GrantInvalid(_) => "grant_invalid",
        }
    }
}

/// Capability API — the stable surface through which extensions request
/// capability execution from the Delta Runtime.
pub trait CapabilityApi {
    fn request(&self, req: CapabilityRequest) -> Result<CapabilityResult, crate::Error>;
    fn cancel(&self, run_id: &str, tool_call_id: &str) -> Result<(), crate::Error>;
}
