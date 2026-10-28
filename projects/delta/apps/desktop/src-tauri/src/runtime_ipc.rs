//! R6 Tauri Runtime IPC — in-process Runtime Host.
//!
//! React invokes commands directly on the embedded Rust Runtime.
//!
//! Runtime events (turn_start, assistant_delta, tool_finished, etc.)
//! are emitted via `app.emit("delta-runtime-event", frame)` so the
//! frontend's existing event-parsing code (`parseRuntimeEvent`) works
//! unchanged through the single product event contract.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use delta_core::{ArtifactOpenAction, CoreControlPlane, EventSink, RuntimeStartRequest};

struct TauriEventSink {
    app: AppHandle,
    core: Arc<CoreControlPlane>,
}

impl EventSink for TauriEventSink {
    fn emit(&self, frame: Value) -> Result<(), String> {
        self.core.record_runtime_event(&frame)?;
        let _ = self.app.emit("delta-runtime-event", frame);
        Ok(())
    }
}

pub type RuntimeRegistry = Arc<CoreControlPlane>;

pub fn init() -> RuntimeRegistry {
    let core =
        Arc::new(CoreControlPlane::open_default().expect("initialize Rust Core control plane"));
    // The desktop app can also see the screen, use the clipboard and open files and links (ADR-0059).
    // A failure to register them leaves the rest of Delta working.
    if let Err(error) = delta_computer::register(core.capabilities(), core.state_dir()) {
        eprintln!("computer capabilities unavailable: {error}");
    }
    let report = core.boot_recovery().unwrap_or_else(|error| {
        eprintln!("runtime boot recovery failed: {error}");
        delta_core::RecoveryReport::default()
    });
    if !report.interrupted_runs.is_empty() || !report.recovered_waiting.is_empty() {
        eprintln!(
            "runtime recovery: {} interrupted, {} waiting, {} side effects swept",
            report.interrupted_runs.len(),
            report.recovered_waiting.len(),
            report.swept_side_effects.len(),
        );
    }
    core
}

/// Headless release smoke uses the same Core authority root as the product.
pub fn portable_self_test() -> Result<(), String> {
    CoreControlPlane::open_default()?.portable_self_test()
}

static APP_EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Run one step of a background loop so that a panic in it ends that step, not the thread: a
/// scheduler that dies silently stops every automation until the app is restarted. Returns
/// whether the step finished.
fn run_guarded(label: &str, step: impl FnOnce()) -> bool {
    let finished = std::panic::catch_unwind(std::panic::AssertUnwindSafe(step)).is_ok();
    if !finished {
        eprintln!("{label} stopped on an internal error; it will try again");
    }
    finished
}

/// Claim the automation runs that are due and start each one.
fn scheduler_tick(app: &AppHandle) {
    let state = app.state::<RuntimeRegistry>();
    let runs = match state.claim_due_runs() {
        Ok(runs) => runs,
        Err(error) => {
            eprintln!("automation scheduler claim failed: {error}");
            Vec::new()
        }
    };
    for run in runs {
        let session_id = run
            .get("session_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let accepted = state.start_claimed_automation(
            &run,
            Arc::new(TauriEventSink {
                app: app.clone(),
                core: state.inner().clone(),
            }),
        );
        if accepted.get("ok").and_then(Value::as_bool) == Some(true) {
            let sequence = APP_EVENT_SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = app.emit(
                "delta-runtime-event",
                json!({
                    "type": "automation_run_started", "version": 1,
                    "sessionId": Value::Null, "sequence": sequence,
                    "payload": {
                        "task_id": run.get("task_id"),
                        "task_title": run.get("task_title"),
                        "session_id": session_id,
                        "workspace": run.get("workspace"),
                    }
                }),
            );
        } else {
            eprintln!(
                "automation runtime start failed: {}",
                accepted
                    .get("error")
                    .map(Value::to_string)
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }
    }
}

pub fn start_scheduler(app: AppHandle) {
    std::thread::Builder::new()
        .name("delta-rust-scheduler".to_string())
        .spawn(move || loop {
            run_guarded("automation scheduler", || scheduler_tick(&app));
            std::thread::sleep(Duration::from_secs(15));
        })
        .expect("start Rust automation scheduler");
}

#[tauri::command]
pub fn health() -> Value {
    json!({
        "status": "ok",
        "default_workspace": null,
        "model": "",
        "protocolVersion": 1,
        "capabilities": ["events.app-wide", "provider.custom", "session.message-revert", "session.reasoning-effort"],
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn runtime_run(
    app: AppHandle,
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    model_id: String,
    user_input: String,
    workspace: Option<String>,
    attachments: Option<Vec<Value>>,
    skill: Option<String>,
    mode: Option<String>,
    max_iterations: Option<usize>,
    max_retries: Option<u32>,
    source: Option<Value>,
) -> Value {
    state.start_runtime(
        RuntimeStartRequest {
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
        },
        Arc::new(TauriEventSink {
            app,
            core: state.inner().clone(),
        }),
    )
}

#[tauri::command]
pub fn runtime_resume(
    app: AppHandle,
    state: State<'_, RuntimeRegistry>,
    session_id: String,
) -> Value {
    state.resume_runtime(
        &session_id,
        Arc::new(TauriEventSink {
            app,
            core: state.inner().clone(),
        }),
    )
}

#[tauri::command]
pub fn runtime_resumable(state: State<'_, RuntimeRegistry>, session_id: String) -> Value {
    state.runtime_resumable(&session_id)
}

#[tauri::command]
pub fn runtime_retry(state: State<'_, RuntimeRegistry>, session_id: String) -> Value {
    state.runtime_retry(&session_id)
}

#[tauri::command]
pub fn runtime_steer(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    text: String,
    source: Option<Value>,
) -> Value {
    state.runtime_steer(&session_id, &text, source)
}

#[tauri::command]
pub fn runtime_follow_up(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    text: String,
    source: Option<Value>,
) -> Value {
    state.runtime_follow_up(&session_id, &text, source)
}

#[tauri::command]
pub fn runtime_cancel(state: State<'_, RuntimeRegistry>, session_id: String) -> Value {
    state.runtime_cancel(&session_id)
}

#[tauri::command]
pub fn runtime_state(state: State<'_, RuntimeRegistry>, session_id: String) -> Value {
    state.runtime_state(&session_id)
}

#[tauri::command]
pub fn runtime_approval(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    decision: String,
    tool_call_id: Option<String>,
) -> Value {
    state.runtime_resolve_approval(&session_id, tool_call_id.as_deref(), &decision)
}

/// What the user has allowed without being asked: this session's grants and the workspace's rules.
#[tauri::command]
pub fn approval_rules(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    workspace: String,
) -> Value {
    state.approval_rules(&session_id, &workspace)
}

/// Take back one allowance, so the next matching call asks again.
#[tauri::command]
pub fn revoke_approval_rule(
    state: State<'_, RuntimeRegistry>,
    scope: String,
    session_id: String,
    workspace: String,
    tool: String,
    command: Option<String>,
) -> Value {
    state.revoke_approval_rule(&scope, &session_id, &workspace, &tool, command.as_deref())
}

#[tauri::command]
pub fn resolve_directory_request(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    is_granted: bool,
    path: Option<String>,
    can_write: bool,
    tool_call_id: Option<String>,
) -> Value {
    state.runtime_resolve_interaction(
        &session_id,
        "directory",
        tool_call_id.as_deref(),
        json!({"granted": is_granted, "path": path, "writable": can_write}),
    )
}

#[tauri::command]
pub fn resolve_plan_request(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    is_approved: bool,
    mode: Option<String>,
    feedback: Option<String>,
    tool_call_id: Option<String>,
) -> Value {
    state.runtime_resolve_interaction(
        &session_id,
        "plan",
        tool_call_id.as_deref(),
        json!({"approved": is_approved, "mode": mode, "feedback": feedback}),
    )
}

#[tauri::command]
pub fn resolve_question_request(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    answer: String,
    tool_call_id: Option<String>,
) -> Value {
    state.runtime_resolve_interaction(
        &session_id,
        "question",
        tool_call_id.as_deref(),
        json!({"answer": answer}),
    )
}

#[tauri::command]
pub fn runtime_messages(state: State<'_, RuntimeRegistry>, session_id: String) -> Value {
    state.runtime_messages(&session_id)
}

#[tauri::command]
pub fn runtime_switch_model(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    model_id: String,
) -> Value {
    state.runtime_switch_model(&session_id, &model_id)
}

#[tauri::command]
pub fn runtime_set_mode(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    mode: String,
) -> Value {
    authority_result(state.set_session_mode(&session_id, &mode))
}

#[tauri::command]
pub fn runtime_truncate(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    index: usize,
) -> Value {
    state.runtime_truncate(&session_id, index)
}

// ---------------------------------------------------------------------------
// R6 Provider / Model / Settings / Secrets authority.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn settings_get(state: State<'_, RuntimeRegistry>) -> Value {
    authority_result(
        state
            .models()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .settings(),
    )
}

#[tauri::command]
pub fn update_model_key(state: State<'_, RuntimeRegistry>, api_key: String) -> Value {
    authority_result(
        state
            .models()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_provider("openai", None, &json!({"api_key": api_key})),
    )
}

#[tauri::command]
pub fn update_model_default(state: State<'_, RuntimeRegistry>, model_id: String) -> Value {
    authority_result(
        state
            .models()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_default_model(&model_id),
    )
}

#[tauri::command]
pub fn settings_add_model(state: State<'_, RuntimeRegistry>, model_id: String) -> Value {
    authority_result(
        state
            .models()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .add_model(&model_id),
    )
}

#[tauri::command]
pub fn settings_remove_model(state: State<'_, RuntimeRegistry>, model_id: String) -> Value {
    authority_result(
        state
            .models()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove_model(&model_id),
    )
}

#[tauri::command]
pub fn settings_set_onboarded(state: State<'_, RuntimeRegistry>, is_onboarded: bool) -> Value {
    authority_result(
        state
            .models()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_onboarded(is_onboarded),
    )
}

#[tauri::command]
pub fn settings_set_language(state: State<'_, RuntimeRegistry>, language: String) -> Value {
    authority_result(
        state
            .models()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_language(&language),
    )
}

#[tauri::command]
pub fn update_context_bar(state: State<'_, RuntimeRegistry>, is_shown: bool) -> Value {
    authority_result(
        state
            .models()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_context_bar(is_shown),
    )
}

#[tauri::command]
pub fn update_session_peek(state: State<'_, RuntimeRegistry>, count: i64) -> Value {
    authority_result(
        state
            .models()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_sessions_peek(count),
    )
}

#[tauri::command]
pub fn update_scratch_base(state: State<'_, RuntimeRegistry>, path: String) -> Value {
    authority_result(
        state
            .models()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_scratch_base(&path),
    )
}

#[tauri::command]
pub fn update_pdf_settings(state: State<'_, RuntimeRegistry>, patch: Value) -> Value {
    authority_result(
        state
            .models()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_pdf_settings(&patch),
    )
}

#[tauri::command]
pub fn update_compaction(state: State<'_, RuntimeRegistry>, patch: Value) -> Value {
    authority_result(
        state
            .models()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_compaction_settings(&patch),
    )
}

/// Which proxy the outbound doors use. The snapshot already reports what `system`
/// mode currently resolves to, so a read costs the caller no second round trip.
#[tauri::command]
pub fn update_proxy_settings(state: State<'_, RuntimeRegistry>, patch: Value) -> Value {
    state
        .models()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .set_proxy_settings(&patch)
        .unwrap_or_else(|error| json!({"ok": false, "error": error}))
}

#[tauri::command]
pub fn providers_list(state: State<'_, RuntimeRegistry>) -> Value {
    authority_result(
        state
            .models()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .providers(),
    )
}

#[tauri::command]
pub fn provider_protocols(state: State<'_, RuntimeRegistry>) -> Value {
    state
        .models()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .protocols()
}

#[tauri::command]
pub fn provider_set(
    state: State<'_, RuntimeRegistry>,
    name: String,
    protocol: Option<String>,
    fields: Value,
) -> Value {
    authority_result(
        state
            .models()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_provider(&name, protocol.as_deref(), &fields),
    )
}

#[tauri::command]
pub fn provider_remove(state: State<'_, RuntimeRegistry>, name: String) -> Value {
    authority_result(
        state
            .models()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove_provider(&name),
    )
}

/// Run blocking work off the thread that serves the window: a synchronous command runs on the
/// main thread, so a request to a provider or a server that is slow to answer would freeze the UI.
async fn off_main_thread<F>(work: F) -> Result<Value, String>
where
    F: FnOnce() -> Value + Send + 'static,
{
    Ok(tauri::async_runtime::spawn_blocking(work)
        .await
        .unwrap_or_else(|error| {
            json!({"ok": false, "error": format!("the request stopped unexpectedly: {error}")})
        }))
}

#[tauri::command]
pub async fn provider_verify(
    state: State<'_, RuntimeRegistry>,
    name: String,
    fields: Value,
) -> Result<Value, String> {
    let registry = state.inner().clone();
    off_main_thread(move || authority_result(registry.verify_provider(&name, &fields))).await
}

#[tauri::command]
pub async fn fetch_provider_models(
    state: State<'_, RuntimeRegistry>,
    name: String,
    fields: Value,
) -> Result<Value, String> {
    let registry = state.inner().clone();
    off_main_thread(move || authority_result(registry.fetch_provider_models(&name, &fields))).await
}

fn authority_result<E: std::fmt::Display>(result: Result<Value, E>) -> Value {
    result.unwrap_or_else(|error| json!({"ok": false, "error": error.to_string()}))
}

// ---------------------------------------------------------------------------
// R6 Application Control Plane — Tauri is a transport adapter only.
// Session/workspace state remains owned by CoreControlPlane.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn sessions_list(state: State<'_, RuntimeRegistry>, workspace: Option<String>) -> Value {
    match state.list_sessions(workspace.as_deref()) {
        Ok(value) => value,
        Err(error) => json!({"sessions": [], "error": error}),
    }
}

#[tauri::command]
pub fn session_messages(state: State<'_, RuntimeRegistry>, session_id: String) -> Value {
    match state.session_messages(&session_id) {
        Ok(value) => value,
        Err(error) => json!({"messages": [], "error": error}),
    }
}

#[tauri::command]
pub fn conversations_search(
    state: State<'_, RuntimeRegistry>,
    workspace: String,
    query: String,
) -> Value {
    match state.search_conversations(&workspace, &query) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "hits": [], "error": error}),
    }
}

/// The statistics tools that run without a model (ADR-0078): read-only, local, no gateway needed.
#[tauri::command]
pub fn statistics_tools(state: State<'_, RuntimeRegistry>) -> Value {
    json!({"tools": state.statistics_tools()})
}

#[tauri::command]
pub fn statistics_run(
    state: State<'_, RuntimeRegistry>,
    tool: String,
    arguments: Value,
    workspace: String,
) -> Value {
    state.run_statistics_tool(&tool, arguments, &workspace)
}

/// Whether the shared Python runtime is published, installed and approved (ADR-0085).
#[tauri::command]
pub fn runtime_pack_status(state: State<'_, RuntimeRegistry>) -> Value {
    state.shared_runtime_status()
}

/// Installs the shared Python runtime once the user has confirmed it. The download runs off the
/// UI thread.
#[tauri::command]
pub async fn runtime_pack_install(state: State<'_, RuntimeRegistry>) -> Result<Value, String> {
    let core = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.install_shared_runtime())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn session_rename(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    title: String,
) -> Value {
    authority_result(state.rename_session(&session_id, &title))
}

#[tauri::command]
pub fn update_session_flags(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    is_pinned: Option<bool>,
    is_archived: Option<bool>,
) -> Value {
    authority_result(state.set_session_flags(&session_id, is_pinned, is_archived))
}

#[tauri::command]
pub fn session_delete(state: State<'_, RuntimeRegistry>, session_id: String) -> Value {
    authority_result(state.delete_session(&session_id))
}

#[tauri::command]
pub fn workspaces_recent(state: State<'_, RuntimeRegistry>) -> Value {
    match state.list_recent_workspaces() {
        Ok(value) => value,
        Err(error) => json!({"workspaces": [], "error": error}),
    }
}

#[tauri::command]
pub fn workspace_open(
    state: State<'_, RuntimeRegistry>,
    path: String,
    should_create: bool,
) -> Value {
    match state.open_workspace(&path, should_create) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "path": path, "error": error}),
    }
}

#[tauri::command]
pub fn workspaces_trusted(state: State<'_, RuntimeRegistry>) -> Value {
    match state.list_trusted_workspaces() {
        Ok(value) => value,
        Err(error) => json!({"workspaces": [], "error": error}),
    }
}

#[tauri::command]
pub fn update_workspace_trust(
    state: State<'_, RuntimeRegistry>,
    path: String,
    is_trusted: bool,
) -> Value {
    match state.set_workspace_trusted(&path, is_trusted) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "path": path, "error": error}),
    }
}

/// Egress levels (ADR-0057): the settings, and the changes only the user makes from the settings
/// page or the first-run choice. A loosening change returns `needs_confirmation` until confirmed.
#[tauri::command]
pub fn get_egress_settings(state: State<'_, RuntimeRegistry>) -> Value {
    match state.egress_settings() {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "error": error}),
    }
}

#[tauri::command]
pub fn set_egress_default(
    state: State<'_, RuntimeRegistry>,
    level: String,
    is_confirmed: bool,
) -> Value {
    match state.set_egress_default(&level, is_confirmed) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "error": error}),
    }
}

/// The local library (B1): the folders in it, adding or refreshing one (which reads the files and
/// can take minutes, so it runs off the UI thread), and taking one out.
#[tauri::command]
pub fn library_roots(state: State<'_, RuntimeRegistry>) -> Value {
    state.library_roots()
}

#[tauri::command]
pub async fn library_index(
    state: State<'_, RuntimeRegistry>,
    root: String,
) -> Result<Value, String> {
    let registry = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || registry.library_index(&root))
        .await
        .map_err(|error| format!("the indexing stopped unexpectedly: {error}"))
}

#[tauri::command]
pub fn library_remove(state: State<'_, RuntimeRegistry>, root: String) -> Value {
    state.library_remove(&root)
}

/// The completion webhook (B5): the saved setting, its change, and a test post.
#[tauri::command]
pub fn get_webhook_settings(state: State<'_, RuntimeRegistry>) -> Value {
    match state.webhook_settings() {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "error": error}),
    }
}

#[tauri::command]
pub fn set_webhook_settings(
    state: State<'_, RuntimeRegistry>,
    url: Option<String>,
    is_enabled: bool,
    is_detailed: bool,
) -> Value {
    match state.set_webhook_settings(url.as_deref(), is_enabled, is_detailed) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "error": error}),
    }
}

/// Post a test message to the saved URL, under the workspace's egress level. Runs off the UI
/// thread: a dead server can take several seconds to give up.
#[tauri::command]
pub async fn test_webhook(
    state: State<'_, RuntimeRegistry>,
    workspace: String,
) -> Result<Value, String> {
    let registry = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || registry.test_webhook(&workspace))
        .await
        .map_err(|error| format!("the webhook test stopped unexpectedly: {error}"))
}

#[tauri::command]
pub fn set_workspace_egress(
    state: State<'_, RuntimeRegistry>,
    workspace: String,
    level: Option<String>,
    is_confirmed: bool,
) -> Value {
    match state.set_workspace_egress(&workspace, level.as_deref(), is_confirmed) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "error": error}),
    }
}

#[tauri::command]
pub fn set_endpoint_mark(
    state: State<'_, RuntimeRegistry>,
    host: String,
    mark: Option<String>,
    is_confirmed: bool,
) -> Value {
    match state.set_endpoint_mark(&host, mark.as_deref(), is_confirmed) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "error": error}),
    }
}

#[tauri::command]
pub fn set_path_rule(
    state: State<'_, RuntimeRegistry>,
    workspace: String,
    path: String,
    class: String,
    rule: Option<String>,
    is_confirmed: bool,
) -> Value {
    match state.set_path_rule(&workspace, &path, &class, rule.as_deref(), is_confirmed) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "error": error}),
    }
}

#[tauri::command]
pub fn set_local_only(
    state: State<'_, RuntimeRegistry>,
    server: String,
    is_local_only: bool,
    is_confirmed: bool,
) -> Value {
    match state.set_local_only(&server, is_local_only, is_confirmed) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "error": error}),
    }
}

#[tauri::command]
pub fn session_revert(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    index: usize,
) -> Value {
    authority_result(state.revert_session(&session_id, index))
}

/// Start a new conversation from the end of an answer (`index` is the answer's message index).
#[tauri::command]
pub fn session_branch(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    index: usize,
    title: Option<String>,
) -> Value {
    authority_result(state.branch_session(&session_id, index, title.as_deref()))
}

/// Undo the files one run wrote: replaced files get their earlier version back, created files are
/// taken out of the workspace (and kept under `.delta/versions/`).
#[tauri::command]
pub fn session_undo_files(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    run_id: String,
) -> Value {
    authority_result(state.undo_run_files(&session_id, &run_id))
}

#[tauri::command]
pub fn restore_file_version(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    backup: String,
    target: String,
) -> Value {
    authority_result(state.restore_file_version(&session_id, &backup, &target))
}

#[tauri::command]
pub fn update_session_reasoning(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    effort: String,
) -> Value {
    authority_result(state.set_reasoning_effort(&session_id, &effort))
}

#[tauri::command]
pub fn session_roots(state: State<'_, RuntimeRegistry>, session_id: String) -> Value {
    match state.list_roots(&session_id) {
        Ok(value) => value,
        Err(error) => json!({"roots": [], "error": error}),
    }
}

#[tauri::command]
pub fn add_session_root(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    path: String,
    can_write: bool,
) -> Value {
    authority_result(state.add_root(&session_id, &path, can_write))
}

#[tauri::command]
pub fn delete_session_root(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    path: String,
) -> Value {
    authority_result(state.remove_root(&session_id, &path))
}

#[tauri::command]
pub fn get_session_unattended(state: State<'_, RuntimeRegistry>, session_id: String) -> Value {
    match state.get_unattended(&session_id) {
        Ok(unattended) => json!({"ok": true, "unattended": unattended}),
        Err(error) => json!({"ok": false, "unattended": false, "error": error}),
    }
}

#[tauri::command]
pub fn update_session_unattended(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    is_unattended: bool,
) -> Value {
    match state.set_unattended(&session_id, is_unattended) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "unattended": false, "error": error}),
    }
}

#[tauri::command]
pub fn inbox_list(
    state: State<'_, RuntimeRegistry>,
    session_id: Option<String>,
    item_state: Option<String>,
) -> Value {
    state.list_inbox(session_id.as_deref(), item_state.as_deref())
}

#[tauri::command]
pub fn inbox_resolve(state: State<'_, RuntimeRegistry>, id: String, resolution: String) -> Value {
    state.resolve_inbox(&id, &resolution)
}

#[tauri::command]
pub fn artifacts_list(state: State<'_, RuntimeRegistry>, session_id: String) -> Value {
    match state.list_artifacts(&session_id) {
        Ok(value) => value,
        Err(error) => json!({"artifacts": [], "error": error}),
    }
}

#[tauri::command]
pub fn artifact_read(state: State<'_, RuntimeRegistry>, session_id: String, path: String) -> Value {
    match state.read_artifact(&session_id, &path) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "path": path, "kind": "unknown", "error": error}),
    }
}

/// Open an artifact with its default application, or show it in the file manager. The path is
/// resolved here from the session's registered artifacts, and only documents and pictures are
/// launched: the front end never hands the shell a file name of its own.
#[tauri::command]
pub fn artifact_open(
    app: AppHandle,
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    path: String,
    mode: String,
) -> Value {
    let plan = match state.plan_artifact_open(&session_id, &path, mode == "open") {
        Ok(plan) => plan,
        Err(error) => return json!({"ok": false, "error": error}),
    };
    let opener = app.opener();
    let outcome = match plan.action {
        ArtifactOpenAction::Open => opener
            .open_path(plan.path.to_string_lossy(), None::<&str>)
            .map_err(|error| error.to_string()),
        ArtifactOpenAction::Reveal => opener
            .reveal_item_in_dir(&plan.path)
            .map_err(|error| error.to_string()),
    };
    match outcome {
        Ok(()) if plan.downgraded => json!({"ok": true, "downgraded": true}),
        Ok(()) => json!({"ok": true}),
        Err(error) => json!({"ok": false, "error": error}),
    }
}

#[tauri::command]
pub fn memory_list(state: State<'_, RuntimeRegistry>) -> Value {
    match state.memory().list() {
        Ok(memory) => json!({"memory": memory}),
        Err(error) => json!({"memory": [], "error": error.to_string()}),
    }
}

#[tauri::command]
pub fn memory_update(state: State<'_, RuntimeRegistry>, id: i64, content: String) -> Value {
    authority_result(state.memory().update(id, &content))
}

#[tauri::command]
pub fn memory_delete(state: State<'_, RuntimeRegistry>, id: i64) -> Value {
    authority_result(state.memory().delete(id))
}

#[tauri::command]
pub fn memory_export(state: State<'_, RuntimeRegistry>) -> Value {
    authority_result(state.memory().export())
}

#[tauri::command]
pub fn memory_import(state: State<'_, RuntimeRegistry>, document: Value) -> Value {
    authority_result(state.memory().import(&document))
}

#[tauri::command]
pub fn clear_memory(state: State<'_, RuntimeRegistry>) -> Value {
    authority_result(state.memory().delete_all())
}

#[tauri::command]
pub fn memory_settings(state: State<'_, RuntimeRegistry>) -> Value {
    authority_result(state.memory().settings())
}

#[tauri::command]
pub fn update_memory_settings(state: State<'_, RuntimeRegistry>, patch: Value) -> Value {
    authority_result(state.memory().set_settings(&patch))
}

#[tauri::command]
pub fn automations_list(state: State<'_, RuntimeRegistry>) -> Value {
    match state
        .automations()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .list()
    {
        Ok(tasks) => json!({"tasks": tasks}),
        Err(error) => json!({"tasks": [], "error": error.to_string()}),
    }
}

#[tauri::command]
pub fn automation_create(state: State<'_, RuntimeRegistry>, payload: Value) -> Value {
    authority_result(
        state
            .automations()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .create(&payload),
    )
}

#[tauri::command]
pub fn automation_get(state: State<'_, RuntimeRegistry>, id: String) -> Value {
    authority_result(
        state
            .automations()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&id),
    )
}

#[tauri::command]
pub fn automation_update(state: State<'_, RuntimeRegistry>, id: String, changes: Value) -> Value {
    authority_result(
        state
            .automations()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .update(&id, &changes),
    )
}

#[tauri::command]
pub fn automation_delete(state: State<'_, RuntimeRegistry>, id: String) -> Value {
    authority_result(
        state
            .automations()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .delete(&id),
    )
}

#[tauri::command]
pub fn automation_denial_threshold(state: State<'_, RuntimeRegistry>) -> Value {
    match state
        .automations()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .denial_threshold()
    {
        Ok(threshold) => json!({"denial_threshold": threshold}),
        Err(error) => json!({"ok": false, "error": error.to_string()}),
    }
}

#[tauri::command]
pub fn set_denial_threshold(state: State<'_, RuntimeRegistry>, threshold: u32) -> Value {
    authority_result(
        state
            .automations()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_denial_threshold(threshold),
    )
}

#[tauri::command]
pub fn update_automation_seen(state: State<'_, RuntimeRegistry>, id: String) -> Value {
    authority_result(
        state
            .automations()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .mark_seen(&id),
    )
}

#[tauri::command]
pub fn prepare_automation_run(state: State<'_, RuntimeRegistry>, id: String) -> Value {
    authority_result(
        state
            .automations()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .prepare_run(&id),
    )
}

#[tauri::command]
pub fn finalize_automation_run(
    state: State<'_, RuntimeRegistry>,
    id: String,
    run_id: String,
) -> Value {
    authority_result(
        state
            .automations()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .finalize_run(&id, &run_id),
    )
}

#[tauri::command]
pub fn scheduler_due(state: State<'_, RuntimeRegistry>) -> Value {
    match state
        .automations()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .due()
    {
        Ok(tasks) => json!({"tasks": tasks}),
        Err(error) => json!({"tasks": [], "error": error.to_string()}),
    }
}

#[tauri::command]
pub fn mcp_list(state: State<'_, RuntimeRegistry>) -> Value {
    state.mcp_list_runtime()
}

#[tauri::command]
pub fn mcp_put(state: State<'_, RuntimeRegistry>, name: String, config: Value) -> Value {
    authority_result(state.mcp().put(&name, config))
}

#[tauri::command]
pub fn mcp_patch(state: State<'_, RuntimeRegistry>, name: String, changes: Value) -> Value {
    authority_result(state.mcp().patch(&name, &changes))
}

#[tauri::command]
pub fn mcp_delete(state: State<'_, RuntimeRegistry>, name: String) -> Value {
    state.mcp_delete_runtime(&name)
}

#[tauri::command]
pub fn mcp_tools(state: State<'_, RuntimeRegistry>, name: String) -> Value {
    state.mcp_tools(&name)
}

#[tauri::command]
pub async fn mcp_reload(state: State<'_, RuntimeRegistry>) -> Result<Value, String> {
    let registry = state.inner().clone();
    off_main_thread(move || registry.mcp_reload()).await
}

#[tauri::command]
pub async fn mcp_connect(state: State<'_, RuntimeRegistry>, name: String) -> Result<Value, String> {
    let registry = state.inner().clone();
    off_main_thread(move || registry.mcp_connect(&name)).await
}

#[tauri::command]
pub fn mcp_signout(state: State<'_, RuntimeRegistry>, name: String) -> Value {
    state.mcp_signout(&name)
}

#[tauri::command]
pub fn audit_list(
    state: State<'_, RuntimeRegistry>,
    limit: Option<usize>,
    session_id: Option<String>,
    connector: Option<String>,
    tool: Option<String>,
) -> Value {
    state.audit_list(
        limit,
        session_id.as_deref(),
        connector.as_deref(),
        tool.as_deref(),
    )
}

#[tauri::command]
pub fn run_sources_list(state: State<'_, RuntimeRegistry>, run_id: String) -> Value {
    state.run_sources_list(&run_id)
}

#[tauri::command]
pub fn sources_list(state: State<'_, RuntimeRegistry>) -> Value {
    state.sources_list()
}

#[tauri::command]
pub fn validations_list(state: State<'_, RuntimeRegistry>) -> Value {
    state.validations_list()
}

#[tauri::command]
pub fn skills_list(state: State<'_, RuntimeRegistry>, workspace: Option<String>) -> Value {
    match state.skills().list(workspace.as_deref()) {
        Ok(skills) => json!({"skills": skills}),
        Err(error) => json!({"skills": [], "error": error.to_string()}),
    }
}

#[tauri::command]
pub fn skill_create(state: State<'_, RuntimeRegistry>, body: Value) -> Value {
    authority_result(state.skills().create(&body))
}

#[tauri::command]
pub fn skill_update(state: State<'_, RuntimeRegistry>, name: String, patch: Value) -> Value {
    authority_result(state.skills().update(
        &name,
        &patch,
        patch.get("workspace").and_then(Value::as_str),
    ))
}

#[tauri::command]
pub fn skill_delete(
    state: State<'_, RuntimeRegistry>,
    name: String,
    workspace: Option<String>,
) -> Value {
    authority_result(state.skills().delete(&name, workspace.as_deref()))
}

#[tauri::command]
pub fn skill_move(
    state: State<'_, RuntimeRegistry>,
    name: String,
    scope: String,
    workspace: Option<String>,
) -> Value {
    authority_result(
        state
            .skills()
            .move_skill(&name, &scope, workspace.as_deref()),
    )
}

#[tauri::command]
pub fn resolve_skill_folder(
    state: State<'_, RuntimeRegistry>,
    name: String,
    workspace: Option<String>,
) -> Value {
    match state.skills().resolve_folder(&name, workspace.as_deref()) {
        Ok(path) => json!({"ok": true, "path": path}),
        Err(error) => json!({"ok": false, "error": error.to_string()}),
    }
}

#[tauri::command]
pub fn stage_skill_upload(
    state: State<'_, RuntimeRegistry>,
    data_b64: String,
    filename: String,
) -> Value {
    authority_result(state.skills().stage_upload(&data_b64, &filename))
}

#[tauri::command]
pub fn confirm_skill_upload(
    state: State<'_, RuntimeRegistry>,
    token: String,
    scope: String,
    workspace: Option<String>,
) -> Value {
    authority_result(
        state
            .skills()
            .confirm_upload(&token, &scope, workspace.as_deref()),
    )
}

#[tauri::command]
pub fn session_skills(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    workspace: Option<String>,
) -> Value {
    match state
        .skills()
        .session_rows(&session_id, workspace.as_deref())
    {
        Ok(skills) => json!({"skills": skills}),
        Err(error) => json!({"skills": [], "error": error.to_string()}),
    }
}

#[tauri::command]
pub fn update_session_skill(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    skill: String,
    is_enabled: bool,
    should_clear: bool,
    workspace: Option<String>,
) -> Value {
    authority_result(state.skills().set_session(
        &session_id,
        &skill,
        is_enabled,
        should_clear,
        workspace.as_deref(),
    ))
}

#[tauri::command]
pub fn connectors_list(state: State<'_, RuntimeRegistry>) -> Result<Value, String> {
    state
        .application()
        .connectors()
        .map(|connectors| json!({"connectors": connectors}))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn connector_connect(
    state: State<'_, RuntimeRegistry>,
    name: String,
    fields: BTreeMap<String, String>,
) -> Value {
    authority_result(state.application().connect(&name, &fields))
}

/// Which web search service is chosen and whether a key is saved (the key is never returned).
#[tauri::command]
pub fn read_search_settings(state: State<'_, RuntimeRegistry>) -> Value {
    authority_result(state.application().web_search_settings())
}

/// Save the search engine list: order, on/off, keys and the engines the user added.
#[tauri::command]
pub fn write_search_settings(state: State<'_, RuntimeRegistry>, engines: Value) -> Value {
    authority_result(state.application().write_search_settings(&engines))
}

/// Ask one engine a real question, with its saved key, so a new engine can be checked before it
/// is relied on. Runs off the UI thread; the key never leaves the core.
#[tauri::command]
pub async fn probe_search_engine(
    state: State<'_, RuntimeRegistry>,
    id: String,
) -> Result<Value, String> {
    let registry = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let engine = match registry.application().web_search_engine(&id) {
            Ok(Some(engine)) => engine,
            Ok(None) => {
                return json!({"ok": false, "error": "this engine has no key or address yet"})
            }
            Err(error) => return json!({"ok": false, "error": error.to_string()}),
        };
        match delta_core::web::search_one(
            &engine,
            "weather today",
            delta_core::web::Limits::PRODUCT,
            &|| false,
        ) {
            Ok(results) => json!({
                "ok": true, "count": results.len(),
                "first": results.first().map(|r| r.title.clone()).unwrap_or_default(),
            }),
            Err(error) => json!({"ok": false, "error": error}),
        }
    })
    .await
    .map_err(|error| format!("the search test stopped unexpectedly: {error}"))
}

#[tauri::command]
pub fn connector_disconnect(state: State<'_, RuntimeRegistry>, name: String) -> Value {
    authority_result(state.application().disconnect(&name))
}

#[tauri::command]
pub fn update_connector_tools(
    state: State<'_, RuntimeRegistry>,
    name: String,
    tool_states: BTreeMap<String, bool>,
) -> Value {
    authority_result(state.application().update_tools(&name, &tool_states))
}

#[tauri::command]
pub fn connector_action(
    state: State<'_, RuntimeRegistry>,
    name: String,
    action: String,
    payload: Value,
) -> Value {
    authority_result(state.application().action(&name, &action, &payload))
}

#[tauri::command]
pub fn session_connections(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
) -> Result<Value, String> {
    state
        .application()
        .session_connections(&session_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn update_session_connection(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    connector: String,
    is_enabled: bool,
    should_clear: bool,
) -> Value {
    authority_result(state.application().set_session_connection(
        &session_id,
        &connector,
        is_enabled,
        should_clear,
    ))
}

#[tauri::command]
pub fn subscriptions_list(state: State<'_, RuntimeRegistry>) -> Result<Value, String> {
    state
        .application()
        .subscriptions()
        .map(|subscriptions| json!({"subscriptions": subscriptions}))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn subscription_add(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    channel: String,
) -> Value {
    authority_result(state.application().subscribe(&session_id, &channel))
}

#[tauri::command]
pub fn subscription_remove(
    state: State<'_, RuntimeRegistry>,
    session_id: String,
    channel: String,
) -> Value {
    authority_result(state.application().unsubscribe(&session_id, &channel))
}

#[tauri::command]
pub fn list_inbox_routes(state: State<'_, RuntimeRegistry>) -> Result<Value, String> {
    state
        .application()
        .inbox_bindings()
        .map(|bindings| json!({"bindings": bindings}))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn update_inbox_routes(
    state: State<'_, RuntimeRegistry>,
    name: String,
    channel: Option<String>,
    target: String,
) -> Value {
    authority_result(
        state
            .application()
            .set_inbox_binding(&name, channel.as_deref(), &target),
    )
}

#[tauri::command]
pub fn unrouted_list(state: State<'_, RuntimeRegistry>) -> Result<Value, String> {
    state
        .application()
        .unrouted()
        .map(|items| json!({"items": items}))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn recent_channels(state: State<'_, RuntimeRegistry>) -> Result<Value, String> {
    state
        .application()
        .recent_channels()
        .map(|channels| json!({"channels": channels}))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_dm_route(state: State<'_, RuntimeRegistry>) -> Result<Value, String> {
    state
        .application()
        .dm_route()
        .map(|dm_session| json!({"dm_session": dm_session}))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn update_dm_route(state: State<'_, RuntimeRegistry>, session_id: String) -> Value {
    authority_result(state.application().set_dm_route(&session_id))
}

fn closed_browser_state() -> Value {
    json!({
        "ok": false,
        "available": false,
        "open": false,
        "url": "",
        "title": "",
        "status": "unavailable",
        "last_action": "",
        "last_result": "",
        "last_error": "browser execution bridge is not available in this build",
        "screenshot_data_url": "",
        "updated_at": Value::Null,
        "controls": [],
    })
}

fn browser_capability(state: &RuntimeRegistry, tool: &str) -> Value {
    let value = state.invoke_capability(tool, json!({}));
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        let mut fallback = closed_browser_state();
        fallback["error"] = value
            .get("error")
            .cloned()
            .unwrap_or_else(|| Value::String("browser capability unavailable".to_string()));
        return fallback;
    }
    value
}

#[tauri::command]
pub fn browser_state(state: State<'_, RuntimeRegistry>) -> Value {
    browser_capability(state.inner(), "browser_state")
}

#[tauri::command]
pub fn browser_screenshot(state: State<'_, RuntimeRegistry>) -> Value {
    browser_capability(state.inner(), "browser_screenshot_state")
}

#[tauri::command]
pub fn browser_close(state: State<'_, RuntimeRegistry>) -> Value {
    browser_capability(state.inner(), "browser_close")
}

#[cfg(test)]
mod tests {
    use super::run_guarded;

    #[test]
    fn a_panic_in_one_step_of_a_background_loop_does_not_end_the_loop() {
        let mut ran = Vec::new();
        for step in 0..3 {
            let finished = run_guarded("test loop", || {
                if step == 1 {
                    panic!("a step failed");
                }
                ran.push(step);
            });
            assert_eq!(finished, step != 1);
        }
        assert_eq!(ran, [0, 2], "the steps after the panic still ran");
    }
}
