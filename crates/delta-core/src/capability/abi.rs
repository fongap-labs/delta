//! Versioned Capability ABI data contract.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CAPABILITY_ABI_VERSION: u32 = 2;

/// A file the capability is permitted to read (workspace-relative or absolute).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityInputFile {
    pub path: String,
    /// SHA-256 of the file content at job start, for integrity / idempotency.
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

/// Permission grants scoped to a single job. A grant is the *most* a capability
/// may do; policy/approval already gated it upstream.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityGrants {
    /// Absolute read roots the job may read (the workspace is always included).
    pub read_roots: Vec<String>,
    /// Absolute write roots the job may write (empty = read-only).
    pub write_roots: Vec<String>,
    /// Allowed outbound network targets, e.g. "https://api.example.com:443".
    pub network: Vec<String>,
    /// Secret grants: keys the capability may read from the secrets boundary.
    pub secrets: Vec<String>,
    /// Whether the job may spawn processes (shell/exec).
    pub exec: bool,
    /// Unix timestamp (seconds, fractional) marking when the grant becomes
    /// active. `None` = immediately. The Runtime checks this before dispatch.
    #[serde(default)]
    pub execution_epoch: Option<f64>,
    /// Unix timestamp (seconds, fractional) marking when the grant expires.
    /// `None` = no expiry. The Runtime refuses to execute past this point.
    #[serde(default)]
    pub expires_at: Option<f64>,
}

impl CapabilityGrants {
    pub fn read_only() -> Self {
        Self {
            write_roots: Vec::new(),
            ..Default::default()
        }
    }
}

/// The effective resource boundary for one capability invocation, derived from
/// its grants. This is the minimal evolvable security surface:
/// filesystem read/write scope, network access, process execution, secret
/// access, and a destructive flag. Every field is auditable — `provenance`
/// records which policy decision granted the boundary so the grant chain is
/// traceable end to end.
///
/// The boundary is metadata derived from grants; the Runtime / Worker still
/// enforces it (the subprocess is `env_clear`-ed, paths are confined by
/// `is_under_roots`, secrets travel through the `secrets` grant — never through
/// the process environment).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityBoundary {
    /// Absolute directories the capability may read (always includes the
    /// workspace).
    pub read_roots: Vec<String>,
    /// Absolute directories the capability may write (empty = read-only).
    pub write_roots: Vec<String>,
    /// Allowed outbound network targets.
    pub network: Vec<String>,
    /// Whether the capability may spawn processes.
    pub exec: bool,
    /// Whether the capability may read granted secrets.
    pub secrets: Vec<String>,
    /// Whether the capability may cause a destructive / irreversible effect.
    pub destructive: bool,
    /// Auditable provenance: which policy decision or registration granted this
    /// boundary (e.g. `"runtime.auto_low_risk"`, `"policy"`, `"approval:ok"`).
    pub provenance: String,
    /// Unix timestamp (seconds, fractional) marking when the boundary becomes
    /// active. Propagated from `CapabilityGrants::execution_epoch`.
    #[serde(default)]
    pub execution_epoch: Option<f64>,
    /// Unix timestamp (seconds, fractional) marking when the boundary expires.
    /// Propagated from `CapabilityGrants::expires_at`. `None` = no expiry.
    #[serde(default)]
    pub expires_at: Option<f64>,
}

/// Stable error codes for execution grant validation.
/// These are the canonical error semantics referenced across the Capability
/// ABI. A Runner must never be reached when any of these conditions fail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantValidationError {
    /// The grant's execution epoch has not yet arrived (future grant).
    GrantNotActive,
    /// The grant has passed its expiry time.
    GrantExpired,
    /// The execution epoch is after the expiry (invalid time window).
    InvalidGrantWindow,
    /// The grant data is malformed or otherwise invalid.
    GrantInvalid(String),
}

impl GrantValidationError {
    /// Returns the stable error code string.
    pub fn code(&self) -> &'static str {
        match self {
            Self::GrantNotActive => "grant_not_active",
            Self::GrantExpired => "grant_expired",
            Self::InvalidGrantWindow => "invalid_grant_window",
            Self::GrantInvalid(_) => "grant_invalid",
        }
    }
}

impl std::fmt::Display for GrantValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GrantNotActive => {
                write!(f, "grant_not_active: execution epoch has not yet arrived")
            }
            Self::GrantExpired => write!(f, "grant_expired: grant has passed its expiry"),
            Self::InvalidGrantWindow => {
                write!(f, "invalid_grant_window: execution epoch is after expiry")
            }
            Self::GrantInvalid(msg) => write!(f, "grant_invalid: {msg}"),
        }
    }
}

impl std::error::Error for GrantValidationError {}

impl CapabilityBoundary {
    /// Derive the effective boundary from grants, defaulting to least
    /// privilege. `provenance` records the grant source for auditability.
    pub fn from_grants(grants: &CapabilityGrants, provenance: impl Into<String>) -> Self {
        Self {
            read_roots: grants.read_roots.clone(),
            write_roots: grants.write_roots.clone(),
            network: grants.network.clone(),
            exec: grants.exec,
            secrets: grants.secrets.clone(),
            destructive: false,
            provenance: provenance.into(),
            execution_epoch: grants.execution_epoch,
            expires_at: grants.expires_at,
        }
    }

    /// Validate structural boundary invariants. Empty filesystem roots are
    /// valid for workspace-less capabilities such as SaaS/network connectors;
    /// least privilege means they should not receive a fake filesystem grant.
    pub fn validate(&self) -> Result<(), String> {
        if let (Some(epoch), Some(expires)) = (self.execution_epoch, self.expires_at) {
            if epoch > expires {
                return Err(format!(
                    "capability boundary execution_epoch ({epoch}) is after \
                     expires_at ({expires}) (provenance={})",
                    self.provenance
                ));
            }
        }
        Ok(())
    }

    /// Fail-closed execution grant validation. This is the primary
    /// Authority validation point -- called by `CapabilityHost::execute()`
    /// before any Runner is dispatched. An invalid grant must never reach
    /// a Runner.
    ///
    /// Checks (in order):
    /// 1. Malformed timestamp values -> `GrantInvalid`
    /// 2. `execution_epoch > expires_at` -> `InvalidGrantWindow`
    /// 3. `execution_epoch > now` (future) -> `GrantNotActive`
    /// 4. `expires_at <= now` (expired) -> `GrantExpired`
    ///
    /// `None` fields are allowed: `execution_epoch = None` means
    /// immediately active; `expires_at = None` means no expiry.
    pub fn validate_execution_grant(&self) -> Result<(), GrantValidationError> {
        let now = unix_secs();

        if let Some(epoch) = self.execution_epoch {
            if !epoch.is_finite() || epoch < 0.0 {
                return Err(GrantValidationError::GrantInvalid(
                    "execution_epoch is not a valid Unix timestamp".to_string(),
                ));
            }
        }
        if let Some(expires) = self.expires_at {
            if !expires.is_finite() || expires < 0.0 {
                return Err(GrantValidationError::GrantInvalid(
                    "expires_at is not a valid Unix timestamp".to_string(),
                ));
            }
        }
        if let (Some(epoch), Some(expires)) = (self.execution_epoch, self.expires_at) {
            if epoch > expires {
                return Err(GrantValidationError::InvalidGrantWindow);
            }
        }
        if let Some(epoch) = self.execution_epoch {
            if now < epoch {
                return Err(GrantValidationError::GrantNotActive);
            }
        }
        if let Some(expires) = self.expires_at {
            if now >= expires {
                return Err(GrantValidationError::GrantExpired);
            }
        }
        Ok(())
    }

    /// True when the boundary's execution window has passed (current time is
    /// past `expires_at`). A boundary with `expires_at = None` never expires.
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .is_some_and(|expires| unix_secs() >= expires)
    }

    /// True when the boundary's execution epoch has not yet arrived. A
    /// boundary with `execution_epoch = None` is immediately active.
    pub fn is_pending(&self) -> bool {
        self.execution_epoch
            .is_some_and(|epoch| unix_secs() < epoch)
    }

    /// True when the boundary permits only read access (no writes, exec,
    /// network, or secrets). The read-only floor.
    pub fn is_read_only(&self) -> bool {
        self.write_roots.is_empty()
            && !self.exec
            && self.network.is_empty()
            && self.secrets.is_empty()
    }
}

/// The full capability invocation contract sent to a Worker Runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityJob {
    pub abi_version: u32,
    /// The capability id (e.g. "file.read", "shell.exec", "web.fetch", "mcp.call").
    pub capability_id: String,
    /// Unique job id for cancelling / correlating side effects.
    pub job_id: String,
    /// Optional owning run id (ledger/audit correlation).
    #[serde(default)]
    pub run_id: Option<String>,
    /// Optional owning session id.
    #[serde(default)]
    pub session_id: Option<String>,
    /// The workspace root the job executes under. None = workspace-less (chat).
    #[serde(default)]
    pub workspace: Option<String>,
    /// Files the job may read, with input hashes.
    #[serde(default)]
    pub input_files: Vec<CapabilityInputFile>,
    /// Capability-specific arguments (JSON).
    pub arguments: Value,
    /// Permission / network / secrets grants for this job.
    #[serde(default)]
    pub grants: CapabilityGrants,
    /// The effective, auditable resource boundary derived from `grants`. Sent
    /// to the worker so the protocol surface carries the provenance, and
    /// validated fail-closed before execution.
    #[serde(default)]
    pub boundary: CapabilityBoundary,
    /// Hard execution timeout in seconds. 0 = none.
    #[serde(default)]
    pub timeout_secs: u64,
    /// Where staged artifacts land (workspace-relative scratch subdir).
    #[serde(default)]
    pub artifact_staging_dir: Option<String>,
    /// Resolved secret values for this job. Never serialized to the
    /// wire JSON (serde(skip)). Delivered to workers via a separate stdin
    /// payload by WorkerProcessRunner. Memory-only, per-job, revoked after
    /// execution.
    #[serde(skip)]
    pub secret_values: BTreeMap<String, String>,
}

impl CapabilityJob {
    pub fn new(capability_id: &str, job_id: &str, arguments: Value) -> Self {
        Self {
            abi_version: Self::version(),
            capability_id: capability_id.to_string(),
            job_id: job_id.to_string(),
            run_id: None,
            session_id: None,
            workspace: None,
            input_files: Vec::new(),
            arguments,
            grants: CapabilityGrants::default(),
            boundary: CapabilityBoundary::default(),
            timeout_secs: 0,
            artifact_staging_dir: None,
            secret_values: BTreeMap::new(),
        }
    }

    pub fn version() -> u32 {
        CAPABILITY_ABI_VERSION
    }

    pub fn workspace(mut self, path: &str) -> Self {
        self.workspace = Some(path.to_string());
        self
    }

    pub fn grants(mut self, grants: CapabilityGrants) -> Self {
        self.grants = grants;
        self
    }

    pub fn running_f_in(mut self, file: CapabilityInputFile) -> Self {
        self.input_files.push(file);
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
}

/// Progress frame a worker emits during execution.
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

/// A staged artifact produced by the job (to be formalized by the Runtime).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityArtifact {
    /// Worker-owned candidate path under `artifact_staging_dir`.
    pub staging_path: String,
    /// Requested destination relative to the workspace. The Runtime validates
    /// this path before promoting the candidate.
    pub relative_path: String,
    /// Artifact kind (e.g. "file", "csv", "sheet", "image", "report").
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub incomplete: bool,
}

/// Structured diagnostics (stderr lines + a typed error, when any).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDiagnostics {
    #[serde(default)]
    pub stderr_lines: Vec<String>,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
}

/// The terminal state machine of a capability job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityExitState {
    /// Completed successfully with a typed result.
    #[serde(rename = "completed")]
    Completed,
    /// Failed with an error/diagnostics.
    #[serde(rename = "failed")]
    Failed,
    /// Cancelled (either by the runtime or a worker-side abort).
    #[serde(rename = "cancelled")]
    Cancelled,
    /// Exceeded the job timeout; runtime forced termination.
    #[serde(rename = "timed_out")]
    TimedOut,
}

/// The typed result a Worker Runner returns after a job terminates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityResult {
    pub abi_version: u32,
    pub job_id: String,
    pub state: CapabilityExitState,
    /// Typed output (parsed), or the raw text output when untyped.
    pub result: Option<Value>,
    /// Text output (stdout) when the capability emits one.
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub artifacts: Vec<CapabilityArtifact>,
    #[serde(default)]
    pub diagnostics: Option<CapabilityDiagnostics>,
    pub finished_at: f64,
}

impl CapabilityResult {
    pub fn completed(job_id: &str, result: Value) -> Self {
        Self {
            abi_version: CAPABILITY_ABI_VERSION,
            job_id: job_id.to_string(),
            state: CapabilityExitState::Completed,
            result: Some(result),
            output: None,
            artifacts: Vec::new(),
            diagnostics: None,
            finished_at: unix_secs(),
        }
    }

    pub fn failed(job_id: &str, message: &str, code: Option<&str>) -> Self {
        Self {
            abi_version: CAPABILITY_ABI_VERSION,
            job_id: job_id.to_string(),
            state: CapabilityExitState::Failed,
            result: None,
            output: None,
            artifacts: Vec::new(),
            diagnostics: Some(CapabilityDiagnostics {
                stderr_lines: Vec::new(),
                error_code: code.map(String::from),
                error_message: Some(message.to_string()),
            }),
            finished_at: unix_secs(),
        }
    }

    pub fn cancelled(job_id: &str) -> Self {
        Self {
            abi_version: CAPABILITY_ABI_VERSION,
            job_id: job_id.to_string(),
            state: CapabilityExitState::Cancelled,
            result: None,
            output: None,
            artifacts: Vec::new(),
            diagnostics: None,
            finished_at: unix_secs(),
        }
    }

    pub fn timed_out(job_id: &str) -> Self {
        Self {
            abi_version: CAPABILITY_ABI_VERSION,
            job_id: job_id.to_string(),
            state: CapabilityExitState::TimedOut,
            result: None,
            output: None,
            artifacts: Vec::new(),
            diagnostics: None,
            finished_at: unix_secs(),
        }
    }

    pub fn with_output(mut self, output: String) -> Self {
        self.output = Some(output);
        self
    }

    pub fn with_artifact(mut self, artifact: CapabilityArtifact) -> Self {
        self.artifacts.push(artifact);
        self
    }

    pub fn with_stderr(mut self, lines: impl IntoIterator<Item = String>) -> Self {
        let diag = self
            .diagnostics
            .get_or_insert_with(|| CapabilityDiagnostics {
                stderr_lines: Vec::new(),
                error_code: None,
                error_message: None,
            });
        diag.stderr_lines.extend(lines);
        self
    }
}

pub(super) fn unix_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
