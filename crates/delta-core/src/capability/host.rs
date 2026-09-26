//! Runtime-owned Capability execution authority.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};

use serde_json::Value;
use sha2::Digest;

use crate::runtime::{StagedArtifact, ToolCall, ToolExecutionContext, ToolExecutor, ToolResult};

use super::abi::{
    CapabilityArtifact, CapabilityBoundary, CapabilityExitState, CapabilityGrants,
    CapabilityInputFile, CapabilityJob, CapabilityProgress, CapabilityResult,
};
use super::registry::{CapabilityControl, CapabilityRegistration, CapabilityRegistry};
use super::runners::NativeCapabilityRunner;

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
