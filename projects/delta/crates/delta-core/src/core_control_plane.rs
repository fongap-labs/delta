//! Product-level Rust Core authority root.
//!
//! `CoreControlPlane` is the only production construction root for Delta's
//! durable authorities and active Runtime handles. Product shells may invoke
//! its typed ports, but must not open authority stores or databases directly.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use crate::application::ApplicationStore;
use crate::approval::ApprovalRecordInput;
use crate::automation::AutomationStore;
use crate::capability::{CapabilityHost, CapabilityRegistration};
use crate::control_plane;
use crate::extension_manifest::load_worker_manifests;
use crate::mcp::McpStore;
use crate::mcp_runtime::McpRuntime;
use crate::memory::MemoryStore;
use crate::model_authority::ModelAuthority;
use crate::policy::RiskLevel;
use crate::policy::{ExecutionMode, ToolMetadata};
use crate::runtime::{
    EventSink, RecoveryReport, RuntimeAuthorities, RuntimeConfig, RuntimeHandle, RuntimeHost,
    ToolCall, ToolExecutionContext, ToolExecutor, ToolExitState,
};
use crate::skills::SkillStore;
use crate::{ApprovalWriter, SourceCitationReader, ValidationReader, DELTA_AGENT};

/// Hits shown by the desktop search box; the model's tool asks for its own limit.
const SEARCH_HIT_LIMIT: usize = 12;

/// Upper bound for a statistics run started from the product; R's own limit is shorter.
const STATISTICS_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone)]
pub struct RuntimeStartRequest {
    pub session_id: String,
    pub model_id: String,
    pub user_input: String,
    pub workspace: Option<String>,
    pub attachments: Option<Vec<Value>>,
    pub skill: Option<String>,
    pub mode: Option<String>,
    pub max_iterations: Option<usize>,
    pub max_retries: Option<u32>,
    pub source: Option<Value>,
}

/// Single product authority root for Runtime, durable state and capability
/// control. The desktop shell owns presentation/platform lifecycle only.
pub struct CoreControlPlane {
    state_dir: PathBuf,
    hosts: Mutex<HashMap<String, Arc<RuntimeHandle>>>,
    models: Mutex<ModelAuthority>,
    authorities: RuntimeAuthorities,
    capabilities: Arc<CapabilityHost>,
    memory: Arc<MemoryStore>,
    automations: Arc<Mutex<AutomationStore>>,
    mcp: Arc<McpStore>,
    mcp_runtime: Arc<McpRuntime>,
    skills: Arc<SkillStore>,
    library: Arc<crate::retrieval::Library>,
    application: Arc<ApplicationStore>,
}

/// Registers the Explore and Office tools when a Python runtime is configured for development or
/// installed under `state_dir` from `pack` (ADR-0085). Without one nothing is registered. Tools
/// registered earlier under the same names are replaced, so the set follows the installed runtime.
fn register_python_tools(
    capabilities: &CapabilityHost,
    state_dir: &Path,
    pack: Option<&crate::runtime_install::PackSource>,
) -> Result<(), String> {
    capabilities.unregister_prefix("statistics_explore_");
    capabilities.unregister_prefix("office_create_");
    let explore = crate::statistics_explore::ExploreRuntime::from_env().or_else(|| {
        pack.and_then(|pack| {
            crate::statistics_explore::ExploreRuntime::from_installed(state_dir, pack)
        })
    });
    for registration in crate::statistics_explore::registrations(explore.as_ref()) {
        capabilities.register(registration)?;
    }
    let office = crate::office_worker::OfficeRuntime::from_env().or_else(|| {
        pack.and_then(|pack| crate::office_worker::OfficeRuntime::from_installed(state_dir, pack))
    });
    for registration in crate::office_worker::registrations(office.as_ref()) {
        capabilities.register(registration)?;
    }
    Ok(())
}

impl CoreControlPlane {
    pub fn open_default() -> Result<Self, String> {
        Self::open(default_state_dir())
    }

    pub fn open(state_dir: impl AsRef<Path>) -> Result<Self, String> {
        let state_dir = state_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&state_dir).map_err(|error| error.to_string())?;

        let application =
            Arc::new(ApplicationStore::open(&state_dir).map_err(|error| error.to_string())?);
        let capability_host = CapabilityHost::product_defaults()?;
        let skills = Arc::new(SkillStore::open(&state_dir).map_err(|error| error.to_string())?);
        capability_host.register(crate::skills::save_registration(skills.clone()))?;
        for registration in crate::skills::lookup_registrations(skills.clone()) {
            capability_host.register(registration)?;
        }
        register_python_tools(
            &capability_host,
            &state_dir,
            crate::runtime_install::published_python_pack().as_ref(),
        )?;
        let library_state = state_dir.clone();
        let library = Arc::new(
            crate::retrieval::Library::open(&state_dir)?.with_hook(Arc::new(move |report| {
                // The library's own version is the authority; the ledger keeps a record that it moved.
                let written =
                    crate::ledger::LedgerWriter::open(library_state.join("run_events.db"))
                        .and_then(|ledger| {
                            ledger.append(
                                "$source",
                                "source.indexed",
                                "runtime",
                                crate::retrieval::now_seconds(),
                                &report.to_json(),
                                &report.root,
                            )
                        });
                if let Err(error) = written {
                    eprintln!("library: could not record source.indexed: {error}");
                }
            })),
        );
        for registration in crate::retrieval::tools::registrations(library.clone()) {
            capability_host.register(registration)?;
        }
        capability_host.register(crate::conversation_search::tool_registration(
            state_dir.clone(),
        ))?;
        let automations = Arc::new(Mutex::new(
            AutomationStore::open(&state_dir).map_err(|error| error.to_string())?,
        ));
        capability_host.register(crate::automation::create_registration(automations.clone()))?;

        // Rust ApplicationStore remains the sole product credential authority.
        // Capability jobs receive only the exact grant-scoped keys requested by
        // their registration, and WorkerProcessRunner keeps them memory-only.
        let secret_authority = application.clone();
        capability_host.set_secret_resolver(move |keys| {
            secret_authority
                .resolve_capability_secrets(keys)
                .map_err(|error| error.to_string())
        });

        // Optional extensions may add worker-backed capabilities. Each extension
        // is isolated: one invalid manifest is skipped without blocking Foundation
        // or other installed extensions.
        let extension_report = load_worker_manifests(&capability_host, &state_dir);
        for error in extension_report.errors {
            eprintln!("Delta extension worker manifest ignored: {error}");
        }

        let tool_authority = application.clone();
        capability_host.set_execution_gate(move |registration, context| {
            let Some(connector) = registration
                .metadata
                .get("connector")
                .and_then(Value::as_str)
            else {
                return Ok(());
            };
            match tool_authority.tool_available(
                connector,
                &registration.tool_name,
                Some(&context.session_id),
            ) {
                Ok(true) => Ok(()),
                Ok(false) => Err(format!(
                    "connector tool is disabled or disconnected: {}",
                    registration.tool_name
                )),
                Err(error) => Err(error.to_string()),
            }
        });

        Ok(Self {
            hosts: Mutex::new(HashMap::new()),
            models: Mutex::new(ModelAuthority::open(&state_dir)?),
            authorities: RuntimeAuthorities::open(&state_dir)?,
            capabilities: Arc::new(capability_host),
            memory: Arc::new(MemoryStore::open(&state_dir).map_err(|error| error.to_string())?),
            automations,
            mcp: Arc::new(McpStore::open(&state_dir).map_err(|error| error.to_string())?),
            mcp_runtime: Arc::new(McpRuntime::new()),
            skills,
            library,
            application,
            state_dir,
        })
    }

    /// The shared Python runtime's state for the application to show.
    pub fn shared_runtime_status(&self) -> serde_json::Value {
        crate::runtime_install::shared_runtime_status(
            &self.state_dir,
            crate::runtime_install::published_python_pack().as_ref(),
        )
    }

    /// Installs the shared Python runtime after the user confirmed it in the application, then
    /// registers the Explore and Office tools against it (ADR-0085). Nothing is fetched before the
    /// confirmation, and the install leaves nothing behind when it fails.
    pub fn install_shared_runtime(&self) -> Result<serde_json::Value, String> {
        let pack = crate::runtime_install::published_python_pack()
            .ok_or_else(|| "the shared runtime is not published yet".to_string())?;
        let approved_at = chrono::Utc::now().to_rfc3339();
        crate::runtime_install::approve_and_install(
            &self.state_dir,
            &pack,
            &crate::runtime_install::HttpsFetch::new(&self.state_dir),
            &approved_at,
        )?;
        register_python_tools(&self.capabilities, &self.state_dir, Some(&pack))?;
        Ok(self.shared_runtime_status())
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    pub fn models(&self) -> &Mutex<ModelAuthority> {
        &self.models
    }

    /// Check a provider's connection. Only reads the stored settings, so it does not take the
    /// authority's lock while it waits on the network.
    pub fn verify_provider(&self, name: &str, fields: &Value) -> Result<Value, String> {
        ModelAuthority::open(&self.state_dir)?.verify_provider(name, fields)
    }

    /// Fetch a provider's model list and add it to the user's models. The wait on the provider
    /// happens outside the authority's lock; only the update to the list is made under it.
    pub fn fetch_provider_models(&self, name: &str, fields: &Value) -> Result<Value, String> {
        let probed = ModelAuthority::open(&self.state_dir)?.probe_models(name, fields)?;
        self.models
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .add_fetched_models(name, &probed)
    }

    pub fn runtime_authorities(&self) -> &RuntimeAuthorities {
        &self.authorities
    }

    pub fn capabilities(&self) -> &Arc<CapabilityHost> {
        &self.capabilities
    }

    /// Invoke a registered capability through the same Rust-owned boundary used
    /// by model tool calls. Product UI helpers use this for optional runtime
    /// surfaces such as the interactive browser.
    pub fn invoke_capability(&self, tool_name: &str, arguments: Value) -> Value {
        // This method bypasses the policy, approval and egress layers because
        // it is only called by the desktop UI for browser state. Restrict it
        // to that allowlist so a future call site cannot accidentally execute
        // an arbitrary capability outside the normal trust boundary.
        const UI_SAFE_TOOLS: &[&str] =
            &["browser_state", "browser_screenshot_state", "browser_close"];
        if !UI_SAFE_TOOLS.contains(&tool_name) {
            return json!({
                "ok": false,
                "available": false,
                "error": format!("invoke_capability: {tool_name} is not on the UI-safe allowlist"),
            });
        }
        let call_id = uuid::Uuid::new_v4().to_string();
        let run_id = format!("system-{}", uuid::Uuid::new_v4());
        let call = ToolCall {
            id: call_id.clone(),
            name: tool_name.to_string(),
            arguments,
        };
        let context = ToolExecutionContext {
            session_id: "__system__".to_string(),
            run_id,
            workspace: None,
            timeout: Duration::from_secs(30),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(|_| {}),
            secrets: BTreeMap::new(),
            proxy: None,
        };
        let result = self.capabilities.execute(&call, &context);
        if let Some(error) = result.error {
            return json!({
                "ok": false,
                "available": false,
                "error": error,
                "result": result.output,
            });
        }
        result.output
    }

    /// The statistics tools a product surface can run without a model turn.
    pub fn statistics_tools(&self) -> Value {
        self.capabilities
            .tool_schemas_filtered(CapabilityRegistration::is_local_statistics)
    }

    /// Runs one local statistics tool directly, so the calculators work with no AI gateway
    /// configured. Only read-only statistics tools are accepted (see `is_local_statistics`), and
    /// the call still goes through the capability host, so the execution gate applies.
    pub fn run_statistics_tool(&self, tool_name: &str, arguments: Value, workspace: &str) -> Value {
        if workspace.trim().is_empty() {
            return json!({"ok": false, "error": "a workspace is needed to read the data file"});
        }
        let Some(registration) = self.capabilities.registration(tool_name) else {
            return json!({"ok": false, "error": format!("unknown statistics tool: {tool_name}")});
        };
        if !registration.is_local_statistics() {
            return json!({"ok": false, "error": format!("{tool_name} is not a local statistics tool")});
        }
        // Fail closed: a run that cannot be written to the audit log does not start.
        if let Err(error) = self.audit_statistics_run(tool_name, &arguments, workspace) {
            return json!({"ok": false, "error": format!("audit log unavailable: {error}")});
        }
        let call = ToolCall {
            id: uuid::Uuid::new_v4().to_string(),
            name: tool_name.to_string(),
            arguments,
        };
        let context = ToolExecutionContext {
            session_id: "__statistics__".to_string(),
            run_id: format!("statistics-{}", uuid::Uuid::new_v4()),
            workspace: Some(workspace.to_string()),
            timeout: STATISTICS_TIMEOUT,
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(|_| {}),
            secrets: BTreeMap::new(),
            proxy: None,
        };
        let result = self.capabilities.execute(&call, &context);
        match result.error {
            Some(error) => json!({"ok": false, "error": error, "result": result.output}),
            None => result.output,
        }
    }

    /// Writes the start of a product-run statistics call to the audit log, where the Activity view
    /// lists it with the approvals and tool calls of the model path.
    fn audit_statistics_run(
        &self,
        tool_name: &str,
        arguments: &Value,
        workspace: &str,
    ) -> Result<(), String> {
        let mut writer = ApprovalWriter::open(
            self.state_dir
                .join("audit_events.db")
                .to_string_lossy()
                .as_ref(),
        )
        .map_err(|error| error.to_string())?;
        let ts = crate::runtime::now_ts();
        writer
            .record(
                ApprovalRecordInput {
                    session_id: "__statistics__".to_string(),
                    agent: Some("delta".to_string()),
                    workspace: Some(workspace.to_string()),
                    connector: None,
                    tool: tool_name.to_string(),
                    stage: "execute".to_string(),
                    status: Some("started".to_string()),
                    approval: None,
                    arguments: Some(arguments.clone()),
                    result_preview: None,
                    reason: Some("statistics run without a model turn".to_string()),
                    resource: None,
                    level: Some(format!("{:?}", RiskLevel::L0)),
                    isolation: Some("runtime".to_string()),
                    ts: Some(ts),
                },
                ts,
                workspace,
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn memory(&self) -> &Arc<MemoryStore> {
        &self.memory
    }

    pub fn automations(&self) -> &Arc<Mutex<AutomationStore>> {
        &self.automations
    }

    pub fn mcp(&self) -> &Arc<McpStore> {
        &self.mcp
    }

    fn refresh_dynamic_tools(&self) {
        self.hosts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|_, handle| handle.state().is_active());
    }

    pub fn mcp_list_runtime(&self) -> Value {
        let mut rows = self.mcp.list();
        self.mcp_runtime.decorate_rows(&mut rows);
        json!({"servers": rows})
    }

    pub fn mcp_connect(&self, name: &str) -> Value {
        let Some(config) = self.mcp.config(name) else {
            return json!({"ok": false, "error": "MCP server not found"});
        };
        let value = self.mcp_runtime.connect(name, &config, &self.capabilities);
        if value.get("ok").and_then(Value::as_bool) == Some(true) {
            self.refresh_dynamic_tools();
        }
        value
    }

    pub fn mcp_tools(&self, name: &str) -> Value {
        self.mcp_runtime.tools(name)
    }

    pub fn mcp_reload(&self) -> Value {
        self.mcp_runtime.disconnect_all(&self.capabilities);
        let mut connected = Vec::new();
        let mut errors = Vec::new();
        for (name, config) in self.mcp.configs() {
            if config.get("enabled").and_then(Value::as_bool) == Some(false) {
                continue;
            }
            let result = self.mcp_runtime.connect(&name, &config, &self.capabilities);
            if result.get("ok").and_then(Value::as_bool) == Some(true) {
                connected.push(name);
            } else {
                errors.push(json!({"name": name, "error": result.get("error")}));
            }
        }
        self.refresh_dynamic_tools();
        json!({"ok": errors.is_empty(), "connected": connected, "errors": errors})
    }

    pub fn mcp_signout(&self, name: &str) -> Value {
        let disconnected = self.mcp_runtime.disconnect(name, &self.capabilities);
        self.refresh_dynamic_tools();
        json!({"ok": true, "disconnected": disconnected, "status": "configured"})
    }

    pub fn mcp_delete_runtime(&self, name: &str) -> Value {
        self.mcp_runtime.disconnect(name, &self.capabilities);
        self.refresh_dynamic_tools();
        match self.mcp.delete(name) {
            Ok(value) => value,
            Err(error) => json!({"ok": false, "error": error.to_string()}),
        }
    }

    pub fn skills(&self) -> &Arc<SkillStore> {
        &self.skills
    }

    pub fn application(&self) -> &Arc<ApplicationStore> {
        &self.application
    }

    pub fn boot_recovery(&self) -> Result<RecoveryReport, String> {
        let report = self.authorities.recover_interrupted_runs()?;
        for run_id in &report.interrupted_runs {
            let session_id = format!("__run__{run_id}");
            self.automations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .finalize_session(&session_id, "interrupted")
                .map_err(|error| error.to_string())?;
        }
        Ok(report)
    }

    pub fn record_runtime_event(&self, frame: &Value) -> Result<(), String> {
        control_plane::record_runtime_event(&self.state_dir, frame)
            .map_err(|error| format!("persist runtime event: {error}"))?;
        let event_type = frame
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if matches!(event_type, "turn_end" | "interrupted" | "error") {
            if let Some(session_id) = frame
                .get("sessionId")
                .and_then(Value::as_str)
                .filter(|session_id| session_id.starts_with("__run__"))
            {
                let recovery = json!({
                    "event": event_type,
                    "payload": frame.get("payload").cloned().unwrap_or_else(|| json!({})),
                });
                self.automations
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .finalize_recovery(session_id, &recovery)
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    pub fn claim_due_runs(&self) -> Result<Vec<Value>, String> {
        let mut automations = self
            .automations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        automations
            .reconcile_terminal_sessions()
            .map_err(|error| error.to_string())?;
        let mut runs = automations
            .claim_due_runs()
            .map_err(|error| error.to_string())?;
        // A folder that cannot be looked at must not hold back the scheduled runs.
        match automations.claim_folder_runs() {
            Ok(folder_runs) => runs.extend(folder_runs),
            Err(error) => eprintln!("automation folder trigger failed: {error}"),
        }
        Ok(runs)
    }

    /// Start one scheduler-claimed automation through the same Core-owned
    /// Runtime path as interactive sessions. Product shells provide only the
    /// event sink; model selection, Runtime request construction, and failure
    /// finalization remain inside the Core authority boundary.
    pub fn start_claimed_automation(&self, run: &Value, sink: Arc<dyn EventSink>) -> Value {
        let session_id = run
            .get("session_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let workspace = run
            .get("workspace")
            .and_then(Value::as_str)
            .map(str::to_string);
        let model_id = match self.default_model_id() {
            Ok(model_id) => model_id,
            Err(error) => {
                if let Err(finalization_error) =
                    self.automation_start_failed(&session_id, Value::String(error.clone()))
                {
                    return json!({
                        "ok": false,
                        "error": error,
                        "finalization_error": finalization_error,
                    });
                }
                return json!({"ok": false, "error": error});
            }
        };

        let accepted = self.start_runtime(
            RuntimeStartRequest {
                session_id: session_id.clone(),
                model_id,
                user_input: run
                    .get("prompt")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                workspace,
                attachments: None,
                skill: None,
                mode: Some("unattended".to_string()),
                max_iterations: None,
                max_retries: None,
                source: Some(json!({
                    "automation_id": run.get("task_id"),
                    "trigger": run.get("trigger").and_then(Value::as_str).unwrap_or("scheduled"),
                })),
            },
            sink,
        );

        if accepted.get("ok").and_then(Value::as_bool) != Some(true) {
            let error = accepted.get("error").cloned().unwrap_or_else(|| {
                Value::String("automation runtime start was not accepted".to_string())
            });
            if let Err(finalization_error) =
                self.automation_start_failed(&session_id, error.clone())
            {
                return json!({
                    "ok": false,
                    "error": error,
                    "finalization_error": finalization_error,
                });
            }
        }
        accepted
    }

    pub fn automation_start_failed(&self, session_id: &str, error: Value) -> Result<(), String> {
        let recovery = json!({
            "event": "error",
            "payload": {
                "error": error,
                "error_type": "automation_start_failed",
                "terminal": true,
            }
        });
        let frame = json!({
            "type": "error",
            "version": 1,
            "sessionId": session_id,
            "sequence": 0,
            "payload": recovery.get("payload").cloned().unwrap_or(Value::Null),
        });
        control_plane::record_runtime_event(&self.state_dir, &frame)
            .map_err(|error| format!("persist runtime event: {error}"))?;
        self.automations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .finalize_recovery(session_id, &recovery)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn default_model_id(&self) -> Result<String, String> {
        let settings = self
            .models
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .settings()?;
        Ok(settings
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string())
    }

    pub fn start_runtime(&self, request: RuntimeStartRequest, sink: Arc<dyn EventSink>) -> Value {
        self.start_runtime_with(request, sink, false)
    }

    /// Whether the session has an interrupted run that `resume_runtime` would continue (ADR-0053).
    /// Read-only; the runtime answers, the UI derives nothing.
    pub fn runtime_resumable(&self, session_id: &str) -> Value {
        match crate::runtime::interrupted_run_of(&self.authorities, session_id) {
            Ok(run_id) => json!({"resumable": true, "runId": run_id}),
            Err(reason) => json!({"resumable": false, "reason": reason}),
        }
    }

    /// Resume the session's interrupted run under its own run id, with the model, workspace and
    /// mode the session was last run with. Works after a restart: the runtime handle is created
    /// the same way a new run creates it. Rejected, writing nothing, when there is nothing to resume.
    pub fn resume_runtime(&self, session_id: &str, sink: Arc<dyn EventSink>) -> Value {
        if let Err(error) = crate::runtime::interrupted_run_of(&self.authorities, session_id) {
            return json!({"ok": false, "error": error});
        }
        let (model_id, mode) = match control_plane::session_model_mode(&self.state_dir, session_id)
        {
            Ok(settings) => settings,
            Err(error) => return json!({"ok": false, "error": error.to_string()}),
        };
        self.start_runtime_with(
            RuntimeStartRequest {
                session_id: session_id.to_string(),
                model_id,
                user_input: String::new(),
                workspace: None,
                attachments: None,
                skill: None,
                mode: Some(mode),
                max_iterations: None,
                max_retries: None,
                source: None,
            },
            sink,
            true,
        )
    }

    fn start_runtime_with(
        &self,
        request: RuntimeStartRequest,
        sink: Arc<dyn EventSink>,
        is_resume: bool,
    ) -> Value {
        let RuntimeStartRequest {
            session_id,
            model_id,
            user_input,
            workspace,
            attachments,
            skill,
            mode,
            max_iterations,
            max_retries,
            source,
        } = request;

        let mut config = match self
            .models
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .resolve_runtime_config(&model_id)
        {
            Ok(config) => config,
            Err(error) => return json!({"ok": false, "error": error}),
        };
        let persisted_workspace =
            control_plane::get_session_workspace(&self.state_dir, &session_id)
                .ok()
                .filter(|value| !value.is_empty());
        let workspace = workspace
            .filter(|value| !value.is_empty())
            .or(persisted_workspace);
        if let Some(effort) = control_plane::get_reasoning_effort(&self.state_dir, &session_id)
            .ok()
            .filter(|value| value != "auto")
        {
            config.model_settings["reasoning_effort"] = Value::String(effort);
        }

        // Housekeeping for what the runtime keeps in the workspace for safety (old versions of
        // replaced files, abandoned staging folders). Cheap, best effort, never blocks a run.
        if let Some(workspace) = workspace.as_deref() {
            control_plane::prune_workspace_leftovers(
                std::path::Path::new(workspace),
                std::time::SystemTime::now(),
            );
        }

        // A tool the egress level refuses for every call is not offered to the model (ADR-0061).
        // An optional tool the session has no use for is left out too, to keep its definition out
        // of every request.
        let tool_workspace = workspace.clone().unwrap_or_default();
        let facts = self.session_facts(&session_id, workspace.as_deref());
        let visible_tools = self.capabilities.tool_schemas_filtered(|registration| {
            if !crate::tool_surface::is_useful(&registration.tool_name, facts) {
                return false;
            }
            let offered = serde_json::from_value::<ToolMetadata>(registration.metadata.clone())
                .map(|metadata| {
                    crate::egress::offered_to_model(
                        &self.state_dir,
                        &tool_workspace,
                        &registration.tool_name,
                        &metadata,
                    )
                })
                .unwrap_or(false);
            if !offered {
                return false;
            }
            let Some(connector) = registration
                .metadata
                .get("connector")
                .and_then(Value::as_str)
            else {
                return true;
            };
            self.application
                .tool_available(connector, &registration.tool_name, Some(&session_id))
                .unwrap_or(false)
        });

        // The prompt describes the tools this session offers and no others, so a tool that is
        // switched off, refused by the egress level or not installed is neither listed nor
        // mentioned. The offered set is the same on every turn unless the user changes it.
        let offered_names: Vec<&str> = visible_tools
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| tool.pointer("/function/name").and_then(Value::as_str))
            .collect();

        let mut prompt = DELTA_AGENT.system_prompt(&offered_names);
        prompt.push_str(&crate::agent::environment_block(&config.model));
        if mode.as_deref() == Some("plan") {
            prompt.push_str("\n\nPlan mode is active. Inspect and reason, but do not perform writes until the user explicitly approves the plan.");
        }
        if let Some(skill_name) = skill.as_deref() {
            let enabled = self
                .skills
                .session_rows(&session_id, workspace.as_deref())
                .ok()
                .and_then(|rows| {
                    rows.into_iter().find(|row| {
                        row.get("name").and_then(Value::as_str) == Some(skill_name)
                            && row.get("enabled").and_then(Value::as_bool) == Some(true)
                    })
                })
                .is_some();
            if !enabled {
                return json!({"ok": false, "error": format!("skill is unavailable for this session: {skill_name}")});
            }
            if let Ok(skills) = self.skills.list(workspace.as_deref()) {
                if let Some(selected) = skills
                    .into_iter()
                    .find(|row| row.get("name").and_then(Value::as_str) == Some(skill_name))
                {
                    prompt.push_str("\n\nActive skill instructions:\n");
                    prompt.push_str(
                        selected
                            .get("instructions")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    );
                }
            }
        }

        let config = RuntimeConfig {
            max_iterations: max_iterations.unwrap_or(12),
            max_retries: max_retries.unwrap_or(crate::runtime::DEFAULT_MAX_RETRIES),
            system_prompt: Some(prompt),
            workspace,
            unattended: control_plane::get_unattended(&self.state_dir, &session_id)
                .unwrap_or(false),
            mode: if mode.as_deref() == Some("plan") {
                ExecutionMode::Plan
            } else {
                ExecutionMode::Execute
            },
            ..self.with_compaction_settings(&model_id, config)
        };

        if let Err(error) = control_plane::ensure_session(
            &self.state_dir,
            &session_id,
            config
                .workspace
                .as_deref()
                .filter(|value| !value.is_empty()),
            &model_id,
        ) {
            return json!({"ok": false, "error": error.to_string()});
        }
        if let Some(mode) = mode.as_deref() {
            match control_plane::set_session_mode(&self.state_dir, &session_id, mode) {
                Ok(value) if value.get("ok").and_then(Value::as_bool) == Some(false) => {
                    return value
                }
                Err(error) => return json!({"ok": false, "error": error.to_string()}),
                _ => {}
            }
        }

        let mut hosts = self
            .hosts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let handle = if let Some(existing) = hosts.get(&session_id).cloned() {
            if existing.state().is_active() {
                return json!({
                    "ok": false,
                    "error": format!("session {session_id} already has an active run")
                });
            }
            if let Err(error) = existing.refresh_runtime(config, visible_tools.clone()) {
                return json!({"ok": false, "error": error});
            }
            existing
        } else {
            let mut host = RuntimeHost::new(&session_id, config)
                .with_authorities(self.authorities.clone())
                .with_tools(visible_tools)
                .with_tool_executor(self.capabilities.clone())
                .with_event_sink(sink);
            let mut seeded_len = 0usize;
            if let Ok(messages) = control_plane::get_session_messages(&self.state_dir, &session_id)
            {
                if let Some(messages) = messages.get("messages").and_then(Value::as_array).cloned()
                {
                    seeded_len = messages.len();
                    host = host.with_messages(messages);
                }
            }
            // A persisted compaction is reloaded with the transcript: the
            // boundary indexes the append-only log, so it survives restarts.
            // A stale index (a transcript that somehow shrank) clamps to the
            // seeded history instead of swallowing it.
            if let Ok(Some(compaction)) =
                control_plane::get_session_compaction(&self.state_dir, &session_id)
            {
                // A compaction saved without an anchor predates the fingerprint check and its
                // index cannot be trusted; it is skipped and the next compaction starts afresh.
                if let (Some(summary), Some(boundary), Some(anchor)) = (
                    compaction.get("summary").and_then(Value::as_str),
                    compaction.get("boundary").and_then(Value::as_u64),
                    compaction.get("anchor").and_then(Value::as_str),
                ) {
                    host = host.with_persisted_compaction(
                        summary.to_string(),
                        (boundary as usize).min(seeded_len),
                        anchor,
                    );
                }
            }
            let handle = match RuntimeHandle::spawn(host) {
                Ok(handle) => Arc::new(handle),
                Err(error) => return json!({"ok": false, "error": error}),
            };
            hosts.insert(session_id.clone(), handle.clone());
            handle
        };
        let accepted = if is_resume {
            handle.resume()
        } else {
            handle.run_with_attachments(user_input, attachments.unwrap_or_default(), source)
        };
        drop(hosts);

        match accepted {
            Ok(run_id) => json!({
                "ok": true,
                "accepted": true,
                "runId": run_id,
                "state": handle.state(),
            }),
            Err(error) => json!({"ok": false, "error": error}),
        }
    }

    fn runtime_handle(&self, session_id: &str) -> Option<Arc<RuntimeHandle>> {
        self.hosts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(session_id)
            .cloned()
    }

    pub fn runtime_retry(&self, session_id: &str) -> Value {
        match self.runtime_handle(session_id) {
            Some(handle) => match handle.retry() {
                Ok(run_id) => json!({"ok": true, "accepted": true, "runId": run_id}),
                Err(error) => json!({"ok": false, "error": error}),
            },
            None => json!({"ok": false, "error": format!("session not found: {session_id}")}),
        }
    }

    pub fn runtime_steer(&self, session_id: &str, text: &str, source: Option<Value>) -> Value {
        match self.runtime_handle(session_id) {
            Some(handle) => match handle.steer(text, source) {
                Ok(()) => json!({"ok": true, "accepted": true}),
                Err(error) => json!({"ok": false, "error": error}),
            },
            None => json!({"ok": false, "error": format!("session not found: {session_id}")}),
        }
    }

    pub fn runtime_follow_up(&self, session_id: &str, text: &str, source: Option<Value>) -> Value {
        match self.runtime_handle(session_id) {
            Some(handle) => match handle.follow_up(text, source) {
                Ok(run_id) => json!({"ok": true, "accepted": true, "runId": run_id}),
                Err(error) => json!({"ok": false, "error": error}),
            },
            None => json!({"ok": false, "error": format!("session not found: {session_id}")}),
        }
    }

    pub fn runtime_cancel(&self, session_id: &str) -> Value {
        match self.runtime_handle(session_id) {
            Some(handle) => json!({"ok": true, "cancelled": handle.cancel()}),
            None => json!({"ok": false, "error": format!("session not found: {session_id}")}),
        }
    }

    /// Lifecycle state of one session's runtime, or `idle` when no live handle
    /// exists. Switching conversations tears down the foreground socket, so a
    /// turn that is still running keeps running — this is how the UI finds that
    /// out when it opens the session again.
    pub fn runtime_state(&self, session_id: &str) -> Value {
        match self.runtime_handle(session_id) {
            Some(handle) => {
                let state = handle.state();
                json!({
                    "ok": true,
                    "state": state,
                    "active": state.is_active(),
                })
            }
            None => json!({"ok": true, "state": "idle", "active": false}),
        }
    }

    pub fn runtime_resolve_approval(
        &self,
        session_id: &str,
        tool_call_id: Option<&str>,
        decision: &str,
    ) -> Value {
        match self.runtime_handle(session_id) {
            Some(handle) => match handle.resolve_approval(tool_call_id, decision) {
                Ok(resolved) => json!({"ok": true, "toolCallId": resolved, "decision": decision}),
                Err(error) => json!({"ok": false, "error": error}),
            },
            None => json!({"ok": false, "error": format!("session not found: {session_id}")}),
        }
    }

    /// What the user has allowed without being asked (A3): this session's standing grants, and the
    /// rules of the workspace. Both are shown in the app and revoked there.
    pub fn approval_rules(&self, session_id: &str, workspace: &str) -> Value {
        let session: Vec<Value> = self
            .runtime_handle(session_id)
            .map(|handle| {
                handle
                    .approval_grants()
                    .into_iter()
                    .map(|grant| json!({"tool": grant.tool, "command": grant.command}))
                    .collect()
            })
            .unwrap_or_default();
        let (rules, error) = if workspace.trim().is_empty() {
            (Vec::new(), None)
        } else {
            match crate::workspace_rules::list(&self.state_dir, workspace) {
                Ok(rules) => (rules, None),
                Err(error) => (Vec::new(), Some(error.to_string())),
            }
        };
        json!({
            "ok": true,
            "session": session,
            "workspace": rules.iter().map(crate::workspace_rules::Rule::to_json).collect::<Vec<_>>(),
            "workspace_error": error,
        })
    }

    /// Take back one allowance. `scope` is `session` or `workspace`; afterwards the call asks again.
    pub fn revoke_approval_rule(
        &self,
        scope: &str,
        session_id: &str,
        workspace: &str,
        tool: &str,
        command: Option<&str>,
    ) -> Value {
        let revoked = match scope {
            "session" => match self.runtime_handle(session_id) {
                Some(handle) => Ok(handle.revoke_approval_grant(tool, command)),
                None => Ok(false),
            },
            "workspace" => {
                crate::workspace_rules::revoke(&self.state_dir, workspace, tool, command)
                    .map_err(|error| error.to_string())
            }
            other => Err(format!("unknown scope: {other}")),
        };
        match revoked {
            Ok(was_revoked) => json!({"ok": true, "revoked": was_revoked}),
            Err(error) => json!({"ok": false, "error": error}),
        }
    }

    pub fn runtime_resolve_interaction(
        &self,
        session_id: &str,
        kind: &str,
        tool_call_id: Option<&str>,
        response: Value,
    ) -> Value {
        let Some(handle) = self.runtime_handle(session_id) else {
            return json!({"ok": false, "error": format!("session not found: {session_id}")});
        };
        match handle.resolve_interaction(kind, tool_call_id, response) {
            Ok(id) => json!({"ok": true, "toolCallId": id}),
            Err(error) => json!({"ok": false, "error": error}),
        }
    }

    pub fn runtime_messages(&self, session_id: &str) -> Value {
        match self.runtime_handle(session_id) {
            Some(handle) => json!({"messages": handle.messages(), "state": handle.state()}),
            None => json!({"ok": false, "error": format!("session not found: {session_id}")}),
        }
    }

    pub fn runtime_switch_model(&self, session_id: &str, model_id: &str) -> Value {
        let config = match self
            .models
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .resolve_runtime_config(model_id)
        {
            Ok(config) => self.with_compaction_settings(model_id, config),
            Err(error) => return json!({"ok": false, "error": error}),
        };
        match self.runtime_handle(session_id) {
            Some(handle) => match handle.switch_runtime_config(config) {
                Ok(Some(notice)) => json!({"ok": true, "notice": notice}),
                Ok(None) => json!({"ok": true, "notice": Value::Null}),
                Err(error) => json!({"ok": false, "error": error}),
            },
            None => json!({"ok": false, "error": format!("session not found: {session_id}")}),
        }
    }

    pub fn runtime_truncate(&self, session_id: &str, index: usize) -> Value {
        match self.runtime_handle(session_id) {
            Some(handle) => match handle.truncate_messages(index) {
                Ok(len) => json!({"ok": true, "len": len}),
                Err(error) => json!({"ok": false, "error": error}),
            },
            None => json!({"ok": false, "error": format!("session not found: {session_id}")}),
        }
    }

    /// Fill the context-compaction settings of a runtime config: the model's
    /// catalog context window plus the persisted product preferences. The
    /// threshold and cap were validated when they were saved, so the values
    /// are only bounded again defensively here.
    fn with_compaction_settings(&self, model_id: &str, mut config: RuntimeConfig) -> RuntimeConfig {
        let prefs = crate::model_authority::prefs_at(&self.state_dir).unwrap_or_default();
        let product = crate::product_settings::ProductSettings::snapshot(&prefs);
        config.context_window_tokens = crate::model_catalog::MODEL_MATRIX
            .iter()
            .find(|(id, _, _)| *id == model_id.trim())
            .and_then(|(_, _, window)| *window);
        config.compaction_threshold_pct = product
            .get("compaction_threshold_pct")
            .and_then(Value::as_f64)
            .filter(|value| (0.10..=0.95).contains(value))
            .unwrap_or(0.8);
        config.compaction_cap_tokens = product
            .get("compaction_cap_tokens")
            .and_then(Value::as_i64)
            .map(|value| value.clamp(10_000, 2_000_000) as u64)
            .unwrap_or(250_000);
        config.compaction_model = product
            .get("compaction_model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        config
    }

    /// Read inbox state through the Core authority boundary. Product shells
    /// receive a serialized view and never receive RuntimeAuthorities itself.
    pub fn list_inbox(&self, session_id: Option<&str>, item_state: Option<&str>) -> Value {
        json!({
            "items": self.authorities.inbox().list(session_id, item_state)
        })
    }

    pub fn resolve_inbox(&self, id: &str, resolution: &str) -> Value {
        let inbox = self.authorities.inbox();
        let Some(item) = inbox.get(id) else {
            return json!({"ok": false, "error": "inbox item not found"});
        };
        if item.state != "pending" {
            return json!({"ok": true, "resolution": item.resolution});
        }
        // A notice answers nothing a run waits for: dismissing it only closes it.
        if item.kind == "notice" {
            return match inbox.resolve(id, resolution) {
                Ok(_) => json!({"ok": true, "resolution": resolution}),
                Err(error) => json!({"ok": false, "error": error.to_string()}),
            };
        }
        let Some(handle) = self.runtime_handle(&item.session_id) else {
            return json!({"ok": false, "error": "the owning runtime is not active; resume the session first"});
        };
        let resolved = if item.kind == "approval" {
            let decision = match resolution {
                "allow" | "approve" | "approved" | "yes" => "once",
                "always_tool" => "always_tool",
                "always_command" => "always_command",
                "always_task" => "always_task",
                _ => "deny",
            };
            handle
                .resolve_approval(item.tool_call_id.as_deref(), decision)
                .map(|_| ())
        } else {
            let response = match item.kind.as_str() {
                "question" => json!({"answer": resolution}),
                "directory" => {
                    let parsed = serde_json::from_str::<Value>(resolution).unwrap_or(Value::Null);
                    let response = if parsed.is_object() {
                        parsed
                    } else {
                        json!({
                            "granted": matches!(resolution, "allow" | "approve" | "approved" | "yes"),
                            "path": item.data.get("path").cloned().unwrap_or(Value::Null),
                            "writable": item.data.get("writable").cloned().unwrap_or(Value::Bool(false)),
                        })
                    };
                    // When the user grants a directory, register it with the
                    // session so the policy scope layer actually enforces the
                    // grant. Returning granted:true without calling add_root
                    // left the model believing access was allowed while the
                    // runtime confined every call to the workspace alone.
                    if response
                        .get("granted")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        if let Some(path) = response.get("path").and_then(Value::as_str) {
                            if !path.is_empty() {
                                let writable = response
                                    .get("writable")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false);
                                let _ = self.add_root(&item.session_id, path, writable);
                            }
                        }
                    }
                    response
                }
                "plan" => json!({
                    "approved": matches!(resolution, "allow" | "approve" | "approved" | "yes"),
                    "feedback": if matches!(resolution, "allow" | "approve" | "approved" | "yes") { Value::Null } else { Value::String(resolution.to_string()) },
                }),
                _ => return json!({"ok": false, "error": "unsupported inbox item kind"}),
            };
            handle
                .resolve_interaction(&item.kind, item.tool_call_id.as_deref(), response)
                .map(|_| ())
        };
        if let Err(error) = resolved {
            return json!({"ok": false, "error": error});
        }
        if let Err(error) = inbox.resolve(id, resolution) {
            return json!({"ok": false, "error": error.to_string()});
        }
        json!({"ok": true})
    }

    pub fn list_sessions(&self, workspace: Option<&str>) -> Result<Value, String> {
        control_plane::list_sessions(&self.state_dir, workspace).map_err(|error| error.to_string())
    }

    pub fn session_messages(&self, session_id: &str) -> Result<Value, String> {
        control_plane::get_session_messages(&self.state_dir, session_id)
            .map_err(|error| error.to_string())
    }

    /// Full-text search over the conversations of one workspace, for the desktop search box.
    pub fn search_conversations(&self, workspace: &str, query: &str) -> Result<Value, String> {
        crate::conversation_search::search(&self.state_dir, workspace, query, SEARCH_HIT_LIMIT)
    }

    pub fn rename_session(&self, session_id: &str, title: &str) -> Result<Value, String> {
        control_plane::rename_session(&self.state_dir, session_id, title)
            .map_err(|error| error.to_string())
    }

    pub fn set_session_flags(
        &self,
        session_id: &str,
        is_pinned: Option<bool>,
        is_archived: Option<bool>,
    ) -> Result<Value, String> {
        let archived = is_archived;
        let pinned = is_pinned;
        control_plane::set_session_flags(&self.state_dir, session_id, pinned, archived)
            .map_err(|error| error.to_string())
    }

    pub fn delete_session(&self, session_id: &str) -> Result<Value, String> {
        control_plane::delete_session(&self.state_dir, session_id)
            .map_err(|error| error.to_string())
    }

    pub fn list_recent_workspaces(&self) -> Result<Value, String> {
        control_plane::list_recent_workspaces(&self.state_dir).map_err(|error| error.to_string())
    }

    pub fn open_workspace(&self, path: &str, should_create: bool) -> Result<Value, String> {
        let create = should_create;
        control_plane::open_workspace(&self.state_dir, path, create)
            .map_err(|error| error.to_string())
    }

    pub fn list_trusted_workspaces(&self) -> Result<Value, String> {
        control_plane::list_trusted_workspaces(&self.state_dir).map_err(|error| error.to_string())
    }

    pub fn set_workspace_trusted(&self, path: &str, is_trusted: bool) -> Result<Value, String> {
        let trusted = is_trusted;
        control_plane::set_workspace_trusted(&self.state_dir, path, trusted)
            .map_err(|error| error.to_string())
    }

    /// Egress levels (ADR-0057): the settings, and the three changes only the user may make.
    pub fn egress_settings(&self) -> Result<Value, String> {
        crate::egress::settings(&self.state_dir).map_err(|error| error.to_string())
    }

    pub fn set_egress_default(&self, level: &str, is_confirmed: bool) -> Result<Value, String> {
        let level = crate::egress::EgressLevel::parse(level)?;
        crate::egress::set_default_level(&self.state_dir, level, is_confirmed)
            .map_err(|error| error.to_string())
    }

    /// The facts about a session that decide which optional tools it offers (`tool_surface`).
    fn session_facts(
        &self,
        session_id: &str,
        workspace: Option<&str>,
    ) -> crate::tool_surface::SessionFacts {
        let has_study = workspace
            .is_some_and(|path| crate::tool_surface::workspace_has_study_record(Path::new(path)));
        let has_statistics_runtime = self
            .capabilities
            .tool_schemas()
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| tool.pointer("/function/name").and_then(Value::as_str))
            .any(|name| name.starts_with("statistics_explore_"));
        let has_other_conversation = control_plane::list_sessions(&self.state_dir, workspace)
            .ok()
            .and_then(|listed| listed.get("sessions").and_then(Value::as_array).cloned())
            .is_some_and(|sessions| {
                sessions.iter().any(|session| {
                    session.get("session_id").and_then(Value::as_str) != Some(session_id)
                        && session.get("messages").and_then(Value::as_u64).unwrap_or(0) > 0
                })
            });
        crate::tool_surface::SessionFacts {
            has_analysis_work: has_study || has_statistics_runtime,
            has_library_folder: self.library.roots().is_ok_and(|roots| !roots.is_empty()),
            // A workspace-less session lists every conversation; only a named workspace scopes them.
            has_other_conversation: workspace.is_some() && has_other_conversation,
        }
    }

    /// The local library (B1): the folders in it, indexing one, and taking one out. Indexing runs
    /// for as long as it needs (up to 30 minutes) and is meant for a background thread.
    pub fn library_roots(&self) -> Value {
        match self.library.roots() {
            Ok(roots) => {
                json!({"ok": true, "roots": roots.iter().map(crate::retrieval::RootInfo::to_json).collect::<Vec<_>>()})
            }
            Err(error) => json!({"ok": false, "error": error}),
        }
    }

    pub fn library_index(&self, root: &str) -> Value {
        match self
            .library
            .index_root(root, Duration::from_secs(1800), &|| false, &|_, _| {})
        {
            Ok(report) => {
                let mut out = report.to_json();
                out["ok"] = json!(true);
                out
            }
            Err(error) => json!({"ok": false, "error": error}),
        }
    }

    pub fn library_remove(&self, root: &str) -> Value {
        match self.library.remove_root(root) {
            Ok(removed) => json!({"ok": true, "removed": removed}),
            Err(error) => json!({"ok": false, "error": error}),
        }
    }

    /// The completion webhook (roadmap B5): its setting, the change, and a test post. The post is
    /// judged by the workspace's egress level like every other route out.
    pub fn webhook_settings(&self) -> Result<Value, String> {
        crate::webhook::settings(&self.state_dir).map_err(|error| error.to_string())
    }

    pub fn set_webhook_settings(
        &self,
        url: Option<&str>,
        is_enabled: bool,
        is_detailed: bool,
    ) -> Result<Value, String> {
        crate::webhook::set_settings(&self.state_dir, url, is_enabled, is_detailed)
    }

    /// Blocking: up to 15 seconds. Callers keep it off the UI thread.
    pub fn test_webhook(&self, workspace: &str) -> Value {
        crate::webhook::test(&self.state_dir, workspace).to_json()
    }

    /// `level` is `None` to make the workspace follow the default again.
    pub fn set_workspace_egress(
        &self,
        workspace: &str,
        level: Option<&str>,
        is_confirmed: bool,
    ) -> Result<Value, String> {
        let level = level.map(crate::egress::EgressLevel::parse).transpose()?;
        crate::egress::set_workspace_level(&self.state_dir, workspace, level, is_confirmed)
            .map_err(|error| error.to_string())
    }

    /// `mark` is `None` to remove the host's mark.
    pub fn set_endpoint_mark(
        &self,
        host: &str,
        mark: Option<&str>,
        is_confirmed: bool,
    ) -> Result<Value, String> {
        let mark = mark.map(crate::egress::EndpointMark::parse).transpose()?;
        crate::egress::set_endpoint_mark(&self.state_dir, host, mark, is_confirmed)
            .map_err(|error| error.to_string())
    }

    /// Set or remove a per-path rule for a workspace (`""` for every workspace). Only this settings
    /// command sets rules: no model, skill or worker can (ADR-0061 point 3).
    pub fn set_path_rule(
        &self,
        workspace: &str,
        path: &str,
        class: &str,
        rule: Option<&str>,
        is_confirmed: bool,
    ) -> Result<Value, String> {
        let rule = rule.map(crate::egress::PathRule::parse).transpose()?;
        crate::egress::set_path_rule(&self.state_dir, workspace, path, class, rule, is_confirmed)
            .map_err(|error| error.to_string())
    }

    /// Mark a local MCP server as local-only, or remove the mark (ADR-0061).
    pub fn set_local_only(
        &self,
        server: &str,
        is_local_only: bool,
        is_confirmed: bool,
    ) -> Result<Value, String> {
        crate::egress::set_local_only(&self.state_dir, server, is_local_only, is_confirmed)
            .map_err(|error| error.to_string())
    }

    pub fn revert_session(&self, session_id: &str, index: usize) -> Result<Value, String> {
        control_plane::revert_session(&self.state_dir, session_id, index)
            .map_err(|error| error.to_string())
    }

    pub fn branch_session(
        &self,
        session_id: &str,
        index: usize,
        title: Option<&str>,
    ) -> Result<Value, String> {
        control_plane::branch_session(&self.state_dir, session_id, index, title)
            .map_err(|error| error.to_string())
    }

    pub fn undo_run_files(&self, session_id: &str, run_id: &str) -> Result<Value, String> {
        control_plane::undo_run_files(&self.state_dir, session_id, run_id)
            .map_err(|error| error.to_string())
    }

    pub fn restore_file_version(
        &self,
        session_id: &str,
        backup: &str,
        target: &str,
    ) -> Result<Value, String> {
        control_plane::restore_file_version(&self.state_dir, session_id, backup, target)
            .map_err(|error| error.to_string())
    }

    pub fn set_reasoning_effort(&self, session_id: &str, effort: &str) -> Result<Value, String> {
        control_plane::set_reasoning_effort(&self.state_dir, session_id, effort)
            .map_err(|error| error.to_string())
    }

    pub fn list_roots(&self, session_id: &str) -> Result<Value, String> {
        control_plane::list_roots(&self.state_dir, session_id).map_err(|error| error.to_string())
    }

    pub fn add_root(
        &self,
        session_id: &str,
        path: &str,
        is_writable: bool,
    ) -> Result<Value, String> {
        let writable = is_writable;
        control_plane::add_root(&self.state_dir, session_id, path, writable)
            .map_err(|error| error.to_string())
    }

    pub fn remove_root(&self, session_id: &str, path: &str) -> Result<Value, String> {
        control_plane::remove_root(&self.state_dir, session_id, path)
            .map_err(|error| error.to_string())
    }

    pub fn get_unattended(&self, session_id: &str) -> Result<bool, String> {
        control_plane::get_unattended(&self.state_dir, session_id)
            .map_err(|error| error.to_string())
    }

    pub fn set_unattended(&self, session_id: &str, is_unattended: bool) -> Result<Value, String> {
        let unattended = is_unattended;
        control_plane::set_unattended(&self.state_dir, session_id, unattended)
            .map_err(|error| error.to_string())
    }

    pub fn set_session_mode(&self, session_id: &str, mode: &str) -> Result<Value, String> {
        control_plane::set_session_mode(&self.state_dir, session_id, mode)
            .map_err(|error| error.to_string())
    }

    pub fn list_artifacts(&self, session_id: &str) -> Result<Value, String> {
        control_plane::list_artifacts(&self.state_dir, session_id)
            .map_err(|error| error.to_string())
    }

    pub fn read_artifact(&self, session_id: &str, path: &str) -> Result<Value, String> {
        control_plane::read_artifact(&self.state_dir, session_id, path)
            .map_err(|error| error.to_string())
    }

    pub fn plan_artifact_open(
        &self,
        session_id: &str,
        path: &str,
        should_open: bool,
    ) -> Result<control_plane::ArtifactOpenPlan, String> {
        control_plane::plan_artifact_open(&self.state_dir, session_id, path, should_open)
            .map_err(|error| error.to_string())
    }

    pub fn audit_list(
        &self,
        limit: Option<usize>,
        session_id: Option<&str>,
        connector: Option<&str>,
        tool: Option<&str>,
    ) -> Value {
        let writer = match ApprovalWriter::open(
            self.state_dir
                .join("audit_events.db")
                .to_string_lossy()
                .as_ref(),
        ) {
            Ok(writer) => writer,
            Err(error) => return json!({"events": [], "error": error.to_string()}),
        };
        match writer.list(
            limit.unwrap_or(100).clamp(1, 500),
            session_id,
            connector,
            tool,
        ) {
            Ok(events) => json!({"events": events}),
            Err(error) => json!({"events": [], "error": error.to_string()}),
        }
    }

    pub fn sources_list(&self) -> Value {
        let path = self.state_dir.join("run_events.db");
        if !path.exists() {
            return json!({"sources": []});
        }
        match SourceCitationReader::open(path).and_then(|reader| reader.list_sources()) {
            Ok(sources) => json!({"sources": sources}),
            Err(error) => json!({"sources": [], "error": error.to_string()}),
        }
    }

    /// The sources one run read (`source.read`), for the artifact trust strip. Read-only.
    pub fn run_sources_list(&self, run_id: &str) -> Value {
        let path = self.state_dir.join("run_events.db");
        if !path.exists() {
            return json!({"sources": []});
        }
        match SourceCitationReader::open(path).and_then(|reader| reader.sources_read(run_id)) {
            Ok(sources) => json!({"sources": sources}),
            Err(error) => json!({"sources": [], "error": error.to_string()}),
        }
    }

    pub fn validations_list(&self) -> Value {
        let path = self.state_dir.join("run_events.db");
        if !path.exists() {
            return json!({"validations": []});
        }
        match ValidationReader::open(path).and_then(|reader| reader.list_validations()) {
            Ok(validations) => json!({"validations": validations}),
            Err(error) => json!({"validations": [], "error": error.to_string()}),
        }
    }

    pub fn portable_self_test(&self) -> Result<(), String> {
        let settings = self
            .models
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .settings()?;
        if !settings.is_object() || self.capabilities.tool_schemas().as_array().is_none() {
            return Err("embedded Runtime authorities did not initialize".to_string());
        }
        let workspace = self.state_dir.join("runtime-self-test-workspace");
        if workspace.exists() {
            std::fs::remove_dir_all(&workspace).map_err(|error| error.to_string())?;
        }
        std::fs::create_dir_all(&workspace).map_err(|error| error.to_string())?;
        std::fs::write(workspace.join("probe.txt"), "delta-runtime-ready")
            .map_err(|error| error.to_string())?;
        let result = self.capabilities.execute(
            &ToolCall {
                id: "portable-self-test".to_string(),
                name: "read_file".to_string(),
                arguments: json!({"path": "probe.txt"}),
            },
            &ToolExecutionContext {
                session_id: "portable-self-test".to_string(),
                run_id: "portable-self-test".to_string(),
                workspace: Some(workspace.to_string_lossy().to_string()),
                timeout: Duration::from_secs(5),
                cancel: Arc::new(AtomicBool::new(false)),
                progress: Arc::new(|_| {}),
                secrets: BTreeMap::new(),
                proxy: None,
            },
        );
        let cleanup = std::fs::remove_dir_all(&workspace);
        if result.state != ToolExitState::Completed
            || result.output.get("text").and_then(Value::as_str) != Some("delta-runtime-ready")
        {
            return Err(format!(
                "native capability self-test failed: {:?}",
                result.state
            ));
        }
        cleanup.map_err(|error| error.to_string())?;
        Ok(())
    }
}

pub fn default_state_dir() -> PathBuf {
    if let Ok(directory) = std::env::var("DELTA_STATE_DIR") {
        return PathBuf::from(directory);
    }
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("delta");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config").join("delta")
}

#[cfg(test)]
mod statistics_run_tests {
    use super::*;

    fn open_plane() -> (tempfile::TempDir, CoreControlPlane) {
        let state = tempfile::tempdir().unwrap();
        let plane = CoreControlPlane::open(state.path()).unwrap();
        (state, plane)
    }

    fn names(schemas: &Value) -> Vec<String> {
        schemas
            .as_array()
            .unwrap()
            .iter()
            .map(|schema| schema["function"]["name"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn only_explore_statistics_tools_are_listed_for_the_statistics_page() {
        // Listed only when a pinned Explore runtime is configured (see statistics_explore).
        let (_state, plane) = open_plane();
        for name in names(&plane.statistics_tools()) {
            assert!(name.starts_with("statistics_explore_"), "{name}");
        }
    }

    #[test]
    fn a_session_offers_the_optional_tools_only_when_it_has_something_for_them_to_work_on() {
        let (state, plane) = open_plane();
        let workspace = state.path().join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        let ws = workspace.to_string_lossy().to_string();
        let facts = |plane: &CoreControlPlane| plane.session_facts("s1", Some(&ws));
        assert_eq!(facts(&plane), crate::tool_surface::SessionFacts::default());

        // A study record in the workspace.
        std::fs::write(workspace.join("study.json"), "{}").unwrap();
        assert!(facts(&plane).has_analysis_work);
        std::fs::remove_file(workspace.join("study.json")).unwrap();

        // Another conversation in the same workspace, with something said in it. The session's own
        // messages, an empty conversation and one in another workspace do not count.
        control_plane::ensure_session(state.path(), "s1", Some(&ws), "m").unwrap();
        control_plane::append_session_message(
            state.path(),
            "s1",
            &json!({"role": "user", "content": "hi"}),
        )
        .unwrap();
        control_plane::ensure_session(state.path(), "empty", Some(&ws), "m").unwrap();
        control_plane::ensure_session(state.path(), "elsewhere", Some("/other"), "m").unwrap();
        control_plane::append_session_message(
            state.path(),
            "elsewhere",
            &json!({"role": "user", "content": "hi"}),
        )
        .unwrap();
        assert!(!facts(&plane).has_other_conversation);
        control_plane::append_session_message(
            state.path(),
            "empty",
            &json!({"role": "user", "content": "hello"}),
        )
        .unwrap();
        assert!(facts(&plane).has_other_conversation);
        assert!(!plane.session_facts("s1", None).has_other_conversation);
    }

    #[test]
    fn an_unregistered_statistics_tool_is_refused_before_it_runs() {
        let (state, plane) = open_plane();
        let workspace = state.path().to_string_lossy().to_string();
        let missing = plane.run_statistics_tool("statistics_f2", json!({}), &workspace);
        assert!(missing["error"]
            .as_str()
            .unwrap()
            .contains("unknown statistics tool"));
    }
}

#[cfg(test)]
mod shared_runtime_tests {
    use super::*;
    use crate::runtime_install::{
        install_root, ArchiveFormat, PackSource, PYTHON_PACK_FORMAT, PYTHON_PACK_NAME,
    };

    /// A pack of the shared runtime's format, laid out as an installed copy: the two worker
    /// packages and a manifest that lists no files, so nothing needs hashing.
    fn installed_pack(state: &Path) -> PackSource {
        let pack = PackSource {
            name: PYTHON_PACK_NAME.to_string(),
            version: "0.1.0".to_string(),
            url: "https://packs.example.com/delta-python-0.1.0.tar.gz".to_string(),
            sha256: "0".repeat(64),
            size: 1,
            format: PYTHON_PACK_FORMAT.to_string(),
            interpreter: "python/bin/python3".to_string(),
            archive: ArchiveFormat::TarGz,
        };
        let root = install_root(state, &pack.name, &pack.version);
        for worker in [
            "packages/explore_pack/worker.py",
            "packages/office_worker/worker.py",
        ] {
            let path = root.join(worker);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        std::fs::write(
            root.join("manifest.json"),
            r#"{"format":"delta.python-runtime","version":1,"files":[]}"#,
        )
        .unwrap();
        pack
    }

    #[test]
    fn an_installed_runtime_registers_its_tools_and_registering_again_replaces_them() {
        let state = tempfile::tempdir().unwrap();
        let pack = installed_pack(state.path());
        let host = CapabilityHost::new();

        register_python_tools(&host, state.path(), Some(&pack)).unwrap();
        assert!(host.registration("office_create_docx").is_some());
        assert!(host.registration("statistics_explore_describe").is_some());

        register_python_tools(&host, state.path(), Some(&pack)).unwrap();
        assert!(host.registration("office_create_docx").is_some());
    }

    #[test]
    fn without_a_published_runtime_no_python_tool_is_registered() {
        let state = tempfile::tempdir().unwrap();
        let host = CapabilityHost::new();

        register_python_tools(&host, state.path(), None).unwrap();

        assert!(host.registration("office_create_docx").is_none());
    }

    #[test]
    fn installing_before_publication_is_refused_and_nothing_is_fetched() {
        let state = tempfile::tempdir().unwrap();
        let plane = CoreControlPlane::open(state.path()).unwrap();

        let error = plane.install_shared_runtime().unwrap_err();

        assert!(error.contains("not published"));
        assert_eq!(plane.shared_runtime_status()["published"], false);
    }
}
