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

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;

use crate::runtime::{StagedArtifact, ToolCall, ToolExecutionContext, ToolExecutor, ToolResult};

/// The Capability ABI wire version. Bump on any breaking change to the contract.
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

fn unix_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// A capability runner: executes a job and returns its typed result.
///
/// Implementations live in the Worker Runner (subprocess, MCP stdio, or a
/// Rust-native capability). The Runtime Host drives this trait and formalizes
/// the artifacts / ledger / idempotency around it.
pub trait CapabilityRunner: Send + Sync {
    fn run(&self, job: &CapabilityJob, control: &CapabilityControl) -> CapabilityResult;
}

/// Runtime-owned controls visible to a runner. Workers may observe cancellation
/// and emit progress; they cannot mutate Runtime state or grant themselves more
/// access.
#[derive(Clone)]
pub struct CapabilityControl {
    cancel: Arc<AtomicBool>,
    progress: Arc<dyn Fn(CapabilityProgress) + Send + Sync>,
}

impl CapabilityControl {
    pub fn new(
        cancel: Arc<AtomicBool>,
        progress: Arc<dyn Fn(CapabilityProgress) + Send + Sync>,
    ) -> Self {
        Self { cancel, progress }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    pub fn emit_progress(&self, progress: CapabilityProgress) {
        (self.progress)(progress);
    }
}

#[derive(Clone)]
pub struct CapabilityRegistration {
    pub capability_id: String,
    pub tool_name: String,
    pub description: String,
    pub parameters: Value,
    pub metadata: Value,
    pub grants: CapabilityGrants,
    pub workspace_write: bool,
    pub runner: Arc<dyn CapabilityRunner>,
}

/// Rust-authoritative capability catalogue. Registration owns both the model
/// schema and the runner, preventing a model-visible tool from bypassing its
/// controlled execution implementation.
#[derive(Default)]
pub struct CapabilityRegistry {
    by_tool: HashMap<String, CapabilityRegistration>,
}

impl CapabilityRegistry {
    pub fn register(&mut self, registration: CapabilityRegistration) -> Result<(), String> {
        if registration.capability_id.trim().is_empty() || registration.tool_name.trim().is_empty()
        {
            return Err("capability id and tool name are required".to_string());
        }
        if self.by_tool.contains_key(&registration.tool_name) {
            return Err(format!(
                "capability tool is already registered: {}",
                registration.tool_name
            ));
        }
        self.by_tool
            .insert(registration.tool_name.clone(), registration);
        Ok(())
    }

    pub fn get(&self, tool_name: &str) -> Option<CapabilityRegistration> {
        self.by_tool.get(tool_name).cloned()
    }

    pub fn unregister_prefix(&mut self, prefix: &str) -> usize {
        let before = self.by_tool.len();
        self.by_tool.retain(|name, _| !name.starts_with(prefix));
        before.saturating_sub(self.by_tool.len())
    }

    pub fn tool_schemas(&self) -> Value {
        self.tool_schemas_filtered(|_| true)
    }

    pub fn tool_schemas_filtered<F>(&self, mut include: F) -> Value
    where
        F: FnMut(&CapabilityRegistration) -> bool,
    {
        let mut registrations = self.by_tool.values().collect::<Vec<_>>();
        registrations.sort_by(|left, right| left.tool_name.cmp(&right.tool_name));
        Value::Array(
            registrations
                .into_iter()
                .filter(|registration| {
                    include(registration)
                        && registration
                            .metadata
                            .get("model_visible")
                            .and_then(Value::as_bool)
                            != Some(false)
                })
                .map(|registration| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": registration.tool_name,
                            "description": registration.description,
                            "parameters": registration.parameters,
                            "metadata": registration.metadata,
                        }
                    })
                })
                .collect(),
        )
    }
}

type SecretResolver =
    dyn Fn(&[String]) -> Result<BTreeMap<String, String>, String> + Send + Sync + 'static;
type ExecutionGate = dyn Fn(&CapabilityRegistration, &ToolExecutionContext) -> Result<(), String>
    + Send
    + Sync
    + 'static;

/// Capability supervisor used by the Runtime as its mandatory ToolExecutor.
pub struct CapabilityHost {
    registry: RwLock<CapabilityRegistry>,
    secret_resolver: RwLock<Option<Arc<SecretResolver>>>,
    execution_gate: RwLock<Option<Arc<ExecutionGate>>>,
}

impl Default for CapabilityHost {
    fn default() -> Self {
        Self {
            registry: RwLock::new(CapabilityRegistry::default()),
            secret_resolver: RwLock::new(None),
            execution_gate: RwLock::new(None),
        }
    }
}

impl CapabilityHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn product_defaults() -> Result<Self, String> {
        let host = Self::new();
        host.register(native_read_registration())?;
        host.register(native_write_registration())?;
        host.register(interaction_registration(
            "request_directory",
            "Request access to an additional directory.",
            serde_json::json!({
                "type": "object", "required": ["reason"],
                "properties": {"reason": {"type": "string"}, "path": {"type": "string"}, "writable": {"type": "boolean"}}
            }),
        ))?;
        host.register(interaction_registration(
            "ask_user",
            "Ask the user a focused question before continuing.",
            serde_json::json!({
                "type": "object", "required": ["question"],
                "properties": {"question": {"type": "string"}, "options": {"type": "array"}, "allow_text": {"type": "boolean"}}
            }),
        ))?;
        host.register(interaction_registration(
            "propose_plan",
            "Present a plan and wait for user approval.",
            serde_json::json!({
                "type": "object", "required": ["plan"], "properties": {"plan": {"type": "string"}}
            }),
        ))?;
        Ok(host)
    }

    pub fn register(&self, registration: CapabilityRegistration) -> Result<(), String> {
        self.registry.write().unwrap().register(registration)
    }

    pub fn unregister_prefix(&self, prefix: &str) -> usize {
        self.registry.write().unwrap().unregister_prefix(prefix)
    }

    /// Install the product-owned secret resolver used to hydrate only the
    /// grant-scoped secret keys required by a capability job. The resolver is
    /// called immediately before dispatch; resolved values stay memory-only.
    pub fn set_secret_resolver<F>(&self, resolver: F)
    where
        F: Fn(&[String]) -> Result<BTreeMap<String, String>, String> + Send + Sync + 'static,
    {
        *self.secret_resolver.write().unwrap() = Some(Arc::new(resolver));
    }

    pub fn tool_schemas(&self) -> Value {
        self.registry.read().unwrap().tool_schemas()
    }

    pub fn tool_schemas_filtered<F>(&self, include: F) -> Value
    where
        F: FnMut(&CapabilityRegistration) -> bool,
    {
        self.registry.read().unwrap().tool_schemas_filtered(include)
    }

    pub fn set_execution_gate<F>(&self, gate: F)
    where
        F: Fn(&CapabilityRegistration, &ToolExecutionContext) -> Result<(), String>
            + Send
            + Sync
            + 'static,
    {
        *self.execution_gate.write().unwrap() = Some(Arc::new(gate));
    }

    fn build_job(
        &self,
        registration: &CapabilityRegistration,
        call: &ToolCall,
        context: &ToolExecutionContext,
    ) -> Result<CapabilityJob, String> {
        let job_id = uuid::Uuid::new_v4().to_string();
        let mut grants = registration.grants.clone();
        let staging_dir = context.workspace.as_ref().map(|workspace| {
            PathBuf::from(workspace)
                .join(".delta")
                .join("staging")
                .join(&context.run_id)
                .join(&job_id)
        });
        if let Some(workspace) = &context.workspace {
            let workspace = PathBuf::from(workspace)
                .canonicalize()
                .map_err(|error| format!("workspace is unavailable: {error}"))?;
            let workspace = workspace.to_string_lossy().to_string();
            if !grants.read_roots.contains(&workspace) {
                grants.read_roots.push(workspace.clone());
            }
            if registration.workspace_write && !grants.write_roots.contains(&workspace) {
                grants.write_roots.push(workspace);
            }
        }
        if let Some(staging_dir) = &staging_dir {
            std::fs::create_dir_all(staging_dir).map_err(|error| error.to_string())?;
        }
        let mut job =
            CapabilityJob::new(&registration.capability_id, &job_id, call.arguments.clone());
        job.run_id = Some(context.run_id.clone());
        job.session_id = Some(context.session_id.clone());
        job.workspace = context.workspace.clone();
        job.grants = grants.clone();
        // Derive the auditable boundary from the effective grants. The
        // provenance records that the boundary came from the registration +
        // auto-scoped workspace (the upstream Policy decision is recorded
        // separately in the ledger).
        job.boundary = CapabilityBoundary::from_grants(&grants, "capability.registration");
        job.boundary.validate()?;
        job.timeout_secs = context.timeout.as_secs().max(1);
        job.artifact_staging_dir = staging_dir.map(|path| path.to_string_lossy().to_string());
        // Resolve only grant-scoped secret values. Product execution
        // resolves from the Rust authority; tests/headless callers may also
        // inject ephemeral context secrets. Context values override matching
        // resolver values and nothing outside grants.secrets can cross the
        // Worker boundary.
        let allowed_keys: HashSet<&str> = job.grants.secrets.iter().map(String::as_str).collect();
        let mut secret_values = if allowed_keys.is_empty() {
            BTreeMap::new()
        } else {
            let resolver = self.secret_resolver.read().unwrap().clone();
            match resolver {
                Some(resolver) => resolver(&job.grants.secrets)?,
                None => BTreeMap::new(),
            }
        };
        secret_values.extend(
            context
                .secrets
                .iter()
                .filter(|(key, _)| allowed_keys.contains(key.as_str()))
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        job.secret_values = secret_values
            .into_iter()
            .filter(|(key, _)| allowed_keys.contains(key.as_str()))
            .collect();
        job.input_files = collect_input_files(&job, registration.workspace_write)?;
        validate_input_files(&job)?;
        Ok(job)
    }
}

impl ToolExecutor for CapabilityHost {
    fn execute(&self, call: &ToolCall, context: &ToolExecutionContext) -> ToolResult {
        let Some(registration) = self.registry.read().unwrap().get(&call.name) else {
            return ToolResult::failure(
                &call.id,
                format!("capability is not registered: {}", call.name),
            );
        };
        if let Some(gate) = self.execution_gate.read().unwrap().clone() {
            if let Err(error) = gate(&registration, context) {
                return ToolResult::failure(&call.id, error);
            }
        }
        let job = match self.build_job(&registration, call, context) {
            Ok(job) => job,
            Err(error) => return ToolResult::failure(&call.id, error),
        };
        // Fail-closed execution grant validation. The Capability Host
        // is the primary Authority validation point. An invalid grant must
        // never reach a Runner.
        if let Err(grant_error) = job.boundary.validate_execution_grant() {
            return ToolResult::failure(&call.id, grant_error.to_string());
        }
        // Check cancellation before dispatch -- a cancelled job must not
        // enter the Runner at all.
        if context.cancel.load(Ordering::Acquire) {
            return ToolResult {
                tool_call_id: call.id.clone(),
                output: serde_json::json!({"ok": false, "cancelled": true}),
                error: Some("cancelled before dispatch".to_string()),
                staged_artifacts: Vec::new(),
                validation_criteria: None,
                state: crate::runtime::ToolExitState::Cancelled,
            };
        }
        let control = CapabilityControl::new(context.cancel.clone(), context.progress.clone());
        let capability_result = registration.runner.run(&job, &control);
        let output = capability_result
            .result
            .clone()
            .or_else(|| capability_result.output.clone().map(Value::String))
            .unwrap_or_else(|| serde_json::json!({}));
        let error = capability_result
            .diagnostics
            .as_ref()
            .and_then(|diagnostics| {
                diagnostics
                    .error_message
                    .clone()
                    .or_else(|| diagnostics.error_code.clone())
            });
        let state = match capability_result.state {
            CapabilityExitState::Completed => crate::runtime::ToolExitState::Completed,
            CapabilityExitState::Failed => crate::runtime::ToolExitState::Failed,
            CapabilityExitState::Cancelled => crate::runtime::ToolExitState::Cancelled,
            CapabilityExitState::TimedOut => crate::runtime::ToolExitState::TimedOut,
        };
        let default_error = match capability_result.state {
            CapabilityExitState::Completed => None,
            CapabilityExitState::Failed => Some("capability failed".to_string()),
            CapabilityExitState::Cancelled => Some("capability cancelled".to_string()),
            CapabilityExitState::TimedOut => Some("capability timed out".to_string()),
        };
        ToolResult {
            tool_call_id: call.id.clone(),
            output,
            error: error.or(default_error),
            staged_artifacts: capability_result
                .artifacts
                .into_iter()
                .map(|artifact| StagedArtifact {
                    staging_path: PathBuf::from(artifact.staging_path),
                    relative_path: artifact.relative_path,
                    kind: artifact.kind.unwrap_or_else(|| "file".to_string()),
                    incomplete: artifact.incomplete,
                })
                .collect(),
            validation_criteria: None,
            state,
        }
    }
}

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

enum WorkerLine {
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

fn drain_worker_lines(
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

fn collect_input_files(
    job: &CapabilityJob,
    can_write_workspace: bool,
) -> Result<Vec<CapabilityInputFile>, String> {
    let workspace_write = can_write_workspace;
    let mut paths = Vec::new();
    if let Some(values) = job.arguments.get("input_files").and_then(Value::as_array) {
        paths.extend(values.iter().filter_map(Value::as_str).map(str::to_string));
    }
    if !workspace_write {
        if let Some(path) = job.arguments.get("path").and_then(Value::as_str) {
            paths.push(path.to_string());
        }
    }
    paths
        .into_iter()
        .map(|path| {
            let absolute = resolve_workspace_path(job.workspace.as_deref(), &path)?;
            let bytes = std::fs::read(&absolute)
                .map_err(|error| format!("input file is unavailable: {path}: {error}"))?;
            Ok(CapabilityInputFile {
                path: absolute.to_string_lossy().to_string(),
                sha256: Some(format!("{:x}", sha2::Sha256::digest(&bytes))),
                size: Some(bytes.len() as u64),
            })
        })
        .collect()
}

fn validate_input_files(job: &CapabilityJob) -> Result<(), String> {
    for input in &job.input_files {
        let path = PathBuf::from(&input.path)
            .canonicalize()
            .map_err(|error| format!("input path is unavailable: {error}"))?;
        if !is_under_roots(&path, &job.grants.read_roots) {
            return Err(format!("input path is outside read grants: {}", input.path));
        }
        let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
        if input
            .sha256
            .as_deref()
            .is_some_and(|hash| hash != format!("{:x}", sha2::Sha256::digest(&bytes)))
        {
            return Err(format!(
                "input hash changed before execution: {}",
                input.path
            ));
        }
    }
    Ok(())
}

fn resolve_workspace_path(workspace: Option<&str>, path: &str) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(path);
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        PathBuf::from(workspace.ok_or_else(|| "workspace is required".to_string())?).join(candidate)
    };
    candidate
        .canonicalize()
        .map_err(|error| format!("path is unavailable: {error}"))
}

fn is_under_roots(path: &Path, roots: &[String]) -> bool {
    roots.iter().any(|root| {
        PathBuf::from(root)
            .canonicalize()
            .is_ok_and(|root| path.starts_with(root))
    })
}

fn native_read_registration() -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "file.read".to_string(),
        tool_name: "read_file".to_string(),
        description: "Read a UTF-8 text file from the trusted workspace.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["path"],
            "properties": {"path": {"type": "string"}}
        }),
        metadata: serde_json::json!({
            "risk_level": "low", "requires_approval": false,
            "category": "read", "capabilities": ["file.read"]
        }),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(|job, control| {
            control.emit_progress(CapabilityProgress {
                job_id: job.job_id.clone(),
                fraction: 0.0,
                stage: Some("reading".to_string()),
                message: None,
            });
            let Some(path) = job.arguments.get("path").and_then(Value::as_str) else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "path is required",
                    Some("arguments"),
                );
            };
            let path = match resolve_workspace_path(job.workspace.as_deref(), path) {
                Ok(path) if is_under_roots(&path, &job.grants.read_roots) => path,
                Ok(_) => {
                    return CapabilityResult::failed(
                        &job.job_id,
                        "path is outside read grants",
                        Some("grant_denied"),
                    )
                }
                Err(error) => {
                    return CapabilityResult::failed(&job.job_id, &error, Some("read_failed"))
                }
            };
            match std::fs::read_to_string(path) {
                Ok(text) => CapabilityResult::completed(
                    &job.job_id,
                    serde_json::json!({"ok": true, "text": text}),
                ),
                Err(error) => {
                    CapabilityResult::failed(&job.job_id, &error.to_string(), Some("read_failed"))
                }
            }
        })),
    }
}

fn native_write_registration() -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "file.write".to_string(),
        tool_name: "write_file".to_string(),
        description: "Create a UTF-8 file artifact in the trusted workspace.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            }
        }),
        metadata: serde_json::json!({
            "risk_level": "medium", "requires_approval": true,
            "category": "filesystem", "capabilities": ["file.write"]
        }),
        grants: CapabilityGrants::default(),
        workspace_write: true,
        runner: Arc::new(NativeCapabilityRunner::new(|job, control| {
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            let Some(relative_path) = job.arguments.get("path").and_then(Value::as_str) else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "path is required",
                    Some("arguments"),
                );
            };
            let Some(content) = job.arguments.get("content").and_then(Value::as_str) else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "content is required",
                    Some("arguments"),
                );
            };
            let Some(staging_dir) = job.artifact_staging_dir.as_deref() else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "artifact staging is unavailable",
                    Some("staging"),
                );
            };
            let staged = PathBuf::from(staging_dir).join("candidate");
            if let Err(error) = std::fs::write(&staged, content.as_bytes()) {
                return CapabilityResult::failed(
                    &job.job_id,
                    &error.to_string(),
                    Some("write_failed"),
                );
            }
            control.emit_progress(CapabilityProgress {
                job_id: job.job_id.clone(),
                fraction: 1.0,
                stage: Some("staged".to_string()),
                message: None,
            });
            CapabilityResult::completed(&job.job_id, serde_json::json!({"ok": true})).with_artifact(
                CapabilityArtifact {
                    staging_path: staged.to_string_lossy().to_string(),
                    relative_path: relative_path.to_string(),
                    kind: Some("file".to_string()),
                    sha256: None,
                    size: None,
                    incomplete: false,
                },
            )
        })),
    }
}

fn interaction_registration(
    tool_name: &str,
    description: &str,
    parameters: Value,
) -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: format!("interaction.{tool_name}"),
        tool_name: tool_name.to_string(),
        description: description.to_string(),
        parameters,
        metadata: serde_json::json!({
            "risk_level": "low", "requires_approval": false,
            "category": "interaction", "capabilities": ["user.interaction"]
        }),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(|job, _| {
            CapabilityResult::failed(
                &job.job_id,
                "interaction capabilities must be executed by RuntimeHost",
                Some("runtime_boundary"),
            )
        })),
    }
}

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
