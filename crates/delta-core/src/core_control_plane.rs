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
use crate::automation::AutomationStore;
use crate::capability::CapabilityHost;
use crate::control_plane;
use crate::extension_manifest::load_worker_manifests;
use crate::mcp::McpStore;
use crate::mcp_runtime::McpRuntime;
use crate::memory::MemoryStore;
use crate::model_authority::ModelAuthority;
use crate::policy::ExecutionMode;
use crate::runtime::{
    EventSink, RecoveryReport, RuntimeAuthorities, RuntimeConfig, RuntimeHandle, RuntimeHost,
    ToolCall, ToolExecutionContext, ToolExecutor, ToolExitState,
};
use crate::skills::SkillStore;
use crate::{ApprovalWriter, SourceCitationReader, ValidationReader, DELTA_AGENT};

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
    application: Arc<ApplicationStore>,
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
            automations: Arc::new(Mutex::new(
                AutomationStore::open(&state_dir).map_err(|error| error.to_string())?,
            )),
            mcp: Arc::new(McpStore::open(&state_dir).map_err(|error| error.to_string())?),
            mcp_runtime: Arc::new(McpRuntime::new()),
            skills: Arc::new(SkillStore::open(&state_dir).map_err(|error| error.to_string())?),
            application,
            state_dir,
        })
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    pub fn models(&self) -> &Mutex<ModelAuthority> {
        &self.models
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
        if let Ok(mut hosts) = self.hosts.lock() {
            hosts.retain(|_, handle| handle.state().is_active());
        }
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
                .map_err(|_| "automation authority lock poisoned".to_string())?
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
                    .map_err(|_| "automation authority lock poisoned".to_string())?
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
            .map_err(|_| "automation authority lock poisoned".to_string())?;
        automations
            .reconcile_terminal_sessions()
            .map_err(|error| error.to_string())?;
        automations
            .claim_due_runs()
            .map_err(|error| error.to_string())
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
            .map_err(|_| "automation authority lock poisoned".to_string())?
            .finalize_recovery(session_id, &recovery)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn default_model_id(&self) -> Result<String, String> {
        let settings = self
            .models
            .lock()
            .map_err(|_| "model authority lock poisoned".to_string())?
            .settings()?;
        Ok(settings
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string())
    }

    pub fn start_runtime(&self, request: RuntimeStartRequest, sink: Arc<dyn EventSink>) -> Value {
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
            .map_err(|_| "model authority lock poisoned".to_string())
            .and_then(|models| models.resolve_runtime_config(&model_id))
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

        let mut prompt = DELTA_AGENT.system_prompt.to_string();
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
            max_retries: max_retries.unwrap_or(2),
            system_prompt: Some(prompt),
            workspace,
            unattended: control_plane::get_unattended(&self.state_dir, &session_id)
                .unwrap_or(false),
            mode: if mode.as_deref() == Some("plan") {
                ExecutionMode::Plan
            } else {
                ExecutionMode::Execute
            },
            ..config
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

        let mut hosts = match self.hosts.lock() {
            Ok(hosts) => hosts,
            Err(_) => return json!({"ok": false, "error": "runtime host registry lock poisoned"}),
        };
        let handle = if let Some(existing) = hosts.get(&session_id).cloned() {
            if existing.state().is_active() {
                return json!({
                    "ok": false,
                    "error": format!("session {session_id} already has an active run")
                });
            }
            if let Err(error) = existing.switch_runtime_config(config) {
                return json!({"ok": false, "error": error});
            }
            existing
        } else {
            let mut host = RuntimeHost::new(&session_id, config)
                .with_authorities(self.authorities.clone())
                .with_tools(self.capabilities.tool_schemas_filtered(|registration| {
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
                }))
                .with_tool_executor(self.capabilities.clone())
                .with_event_sink(sink);
            if let Ok(messages) = control_plane::get_session_messages(&self.state_dir, &session_id)
            {
                if let Some(messages) = messages.get("messages").and_then(Value::as_array).cloned()
                {
                    host = host.with_messages(messages);
                }
            }
            let handle = match RuntimeHandle::spawn(host) {
                Ok(handle) => Arc::new(handle),
                Err(error) => return json!({"ok": false, "error": error}),
            };
            hosts.insert(session_id.clone(), handle.clone());
            handle
        };
        let accepted =
            handle.run_with_attachments(user_input, attachments.unwrap_or_default(), source);
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
        self.hosts.lock().ok()?.get(session_id).cloned()
    }

    pub fn runtime_resume(&self, session_id: &str) -> Value {
        match self.runtime_handle(session_id) {
            Some(handle) => match handle.resume() {
                Ok(run_id) => json!({"ok": true, "accepted": true, "runId": run_id}),
                Err(error) => json!({"ok": false, "error": error}),
            },
            None => json!({"ok": false, "error": format!("session not found: {session_id}")}),
        }
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
            .map_err(|_| "model authority lock poisoned".to_string())
            .and_then(|models| models.resolve_runtime_config(model_id))
        {
            Ok(config) => config,
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
                    if parsed.is_object() {
                        parsed
                    } else {
                        json!({
                            "granted": matches!(resolution, "allow" | "approve" | "approved" | "yes"),
                            "path": item.data.get("path").cloned().unwrap_or(Value::Null),
                            "writable": item.data.get("writable").cloned().unwrap_or(Value::Bool(false)),
                        })
                    }
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

    pub fn revert_session(&self, session_id: &str, index: usize) -> Result<Value, String> {
        control_plane::revert_session(&self.state_dir, session_id, index)
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

    pub fn resolve_artifact_path(&self, session_id: &str, path: &str) -> Result<PathBuf, String> {
        control_plane::resolve_artifact_path(&self.state_dir, session_id, path)
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
            .map_err(|_| "model authority lock poisoned".to_string())?
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
