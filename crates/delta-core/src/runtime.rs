//! R6 Rust Runtime Host — the owned agent loop.
//!
//! Combines R1-R5.1 Rust authorities (provider, ledger, policy, tool_lifecycle,
//! idemlog, retry) into a complete runtime that drives a full turn:
//!
//! User Input -> Context -> Model -> Tool -> Tool Result -> Model Continue -> Artifact -> Complete
//!
//! Replaces the Python `TurnEngine` (core/engine.py). No Python dependency.
//!
//! Lifecycle:
//! - `run()` — fresh user input, event stream
//! - `resume()` — continue a suspended turn (pending tool calls)
//! - `retry()` — re-run after a provider error
//! - `steer()` — inject mid-turn steering text
//! - `follow_up()` — queue a follow-up turn for after completion
//! - `cancel()` — interrupt from any state

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, RwLock};
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};

use crate::approval::{ApprovalController, ApprovalDecision, ApprovalWriter};
use crate::capability::CapabilityProgress;
use crate::checkpoint::{CheckpointReader, CheckpointRegisterInput, CheckpointWriter};
use crate::idemlog::{IdempotencyWriter, SideEffectEntry};
use crate::inbox::InboxStore;
use crate::policy::{self, Decision, PolicyEvaluateInput, RiskLevel, RootEntry, ToolMetadata};
use crate::provider::{self, ProviderRequest};
use crate::LedgerWriter;

mod handle;
mod human_control;
mod model_turn;
mod tool_execution;

use human_control::InteractionController;

const DEFAULT_MAX_ITERATIONS: usize = 12;

/// Sink for runtime events. The delta_core binary installs a stdout sink;
/// the in-process Tauri shell installs a Tauri-event sink.
pub trait EventSink: Send + Sync {
    fn emit(&self, frame: Value) -> Result<(), String>;
}

/// Default sink: write the frame as one JSON line to stdout.
pub struct StdoutSink;
impl EventSink for StdoutSink {
    fn emit(&self, frame: Value) -> Result<(), String> {
        let mut stdout = std::io::stdout();
        writeln!(stdout, "{frame}").map_err(|error| error.to_string())?;
        stdout.flush().map_err(|error| error.to_string())?;
        Ok(())
    }
}

/// Silent sink: drop every event (tests, headless runners).
pub struct NullSink;
impl EventSink for NullSink {
    fn emit(&self, _: Value) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Default)]
pub struct AssistantTurn {
    pub text: Option<String>,
    pub reasoning: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<String>,
    pub usage: Option<Value>,
}

#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    TurnStart {
        input: Value,
        attachments: Vec<Value>,
        source: Option<Value>,
        run_id: Option<String>,
    },
    AssistantDelta {
        text: String,
    },
    ReasoningDelta {
        text: String,
    },
    AssistantMessage {
        message: Value,
        text: Option<String>,
        tool_calls: Vec<String>,
        reasoning: Option<String>,
        usage: Option<Value>,
    },
    ToolProposed {
        tool_call_id: String,
        name: String,
        arguments: Value,
        risk_level: Option<String>,
    },
    ToolStarted {
        tool_call_id: String,
        name: String,
    },
    ToolFinished {
        tool_call_id: String,
        name: String,
        result: Value,
        error: Option<String>,
    },
    PermissionRequired {
        tool_call_id: String,
        name: String,
        arguments: Value,
        reason: String,
    },
    DirectoryRequested {
        tool_call_id: String,
        reason: String,
        path: String,
        writable: bool,
    },
    QuestionRequested {
        tool_call_id: String,
        arguments: Value,
    },
    PlanProposed {
        tool_call_id: String,
        plan: String,
    },
    IterationEnd {
        iteration: usize,
    },
    TurnEnd {
        status: String,
        iterations: usize,
    },
    Error {
        error: String,
        error_type: String,
        terminal: bool,
    },
    Interrupted {
        iterations: usize,
    },
    Compacting,
    Compacted {
        text: String,
    },
    ModelChanged {
        model: String,
    },
}

/// The only event envelope exposed by the Rust Runtime to product surfaces.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RuntimeEventEnvelopeV1 {
    #[serde(rename = "type")]
    pub event_type: String,
    pub version: u8,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub sequence: u64,
    pub payload: Value,
}

/// Authoritative lifecycle state for one session runtime.
///
/// A [`RuntimeHandle`] owns this state independently from the worker thread so
/// control commands never need to lock the agent loop itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Idle,
    Running,
    WaitingApproval,
    WaitingUser,
    Cancelling,
    Interrupted,
    Failed,
    Completed,
}

impl RuntimeState {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Running | Self::WaitingApproval | Self::WaitingUser | Self::Cancelling
        )
    }
}

impl RuntimeEvent {
    pub fn to_frame(&self, session_id: &str, sequence: u64) -> Value {
        let (event_type, payload) = match self {
            Self::TurnStart {
                input,
                attachments,
                source,
                run_id,
            } => (
                "turn_start",
                json!({"input": input, "attachments": attachments, "source": source, "run_id": run_id}),
            ),
            Self::AssistantDelta { text } => ("assistant_delta", json!({"text": text})),
            Self::ReasoningDelta { text } => ("reasoning_delta", json!({"text": text})),
            Self::AssistantMessage {
                message,
                text,
                tool_calls,
                reasoning,
                usage,
            } => (
                "assistant_message",
                json!({"message": message, "text": text, "tool_calls": tool_calls, "reasoning": reasoning, "usage": usage}),
            ),
            Self::ToolProposed {
                tool_call_id,
                name,
                arguments,
                risk_level,
            } => (
                "tool_proposed",
                json!({"tool_call_id": tool_call_id, "name": name, "arguments": arguments, "risk_level": risk_level}),
            ),
            Self::ToolStarted { tool_call_id, name } => (
                "tool_started",
                json!({"tool_call_id": tool_call_id, "name": name}),
            ),
            Self::ToolFinished {
                tool_call_id,
                name,
                result,
                error,
            } => (
                "tool_finished",
                json!({"tool_call_id": tool_call_id, "name": name, "result": result, "error": error}),
            ),
            Self::PermissionRequired {
                tool_call_id,
                name,
                arguments,
                reason,
            } => (
                "permission_required",
                json!({"tool_call_id": tool_call_id, "name": name, "arguments": arguments, "reason": reason}),
            ),
            Self::DirectoryRequested {
                tool_call_id,
                reason,
                path,
                writable,
            } => (
                "directory_requested",
                json!({"tool_call_id": tool_call_id, "reason": reason, "path": path, "writable": writable}),
            ),
            Self::QuestionRequested {
                tool_call_id,
                arguments,
            } => {
                let mut payload = arguments.clone();
                payload["tool_call_id"] = Value::String(tool_call_id.clone());
                ("question_requested", payload)
            }
            Self::PlanProposed { tool_call_id, plan } => (
                "plan_proposed",
                json!({"tool_call_id": tool_call_id, "plan": plan}),
            ),
            Self::IterationEnd { iteration } => ("iteration_end", json!({"iteration": iteration})),
            Self::TurnEnd { status, iterations } => (
                "turn_end",
                json!({"status": status, "iterations": iterations}),
            ),
            Self::Error {
                error,
                error_type,
                terminal,
            } => (
                "error",
                json!({"error": error, "error_type": error_type, "terminal": terminal}),
            ),
            Self::Interrupted { iterations } => ("interrupted", json!({"iterations": iterations})),
            Self::Compacting => ("compacting", json!({})),
            Self::Compacted { text } => ("compacted", json!({"text": text})),
            Self::ModelChanged { model } => ("model_changed", json!({"model": model})),
        };
        serde_json::to_value(RuntimeEventEnvelopeV1 {
            event_type: event_type.to_string(),
            version: 1,
            session_id: session_id.to_string(),
            sequence,
            payload,
        })
        .expect("runtime event envelope is serializable")
    }
}

pub trait ToolExecutor: Send + Sync {
    fn execute(&self, call: &ToolCall, context: &ToolExecutionContext) -> ToolResult;
}

#[derive(Clone)]
pub struct ToolExecutionContext {
    pub session_id: String,
    pub run_id: String,
    pub workspace: Option<String>,
    pub timeout: Duration,
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<dyn Fn(CapabilityProgress) + Send + Sync>,
    /// R8.4: Ephemeral, task-scoped secret values resolved by the RuntimeHost
    /// from the Foundation authority. Never written to environment variables,
    /// never persisted, dropped after execution.
    pub secrets: BTreeMap<String, String>,
}

/// A worker-created file that is not visible as a product artifact until the
/// Rust Runtime validates, promotes, hashes and registers it.
#[derive(Debug, Clone)]
pub struct StagedArtifact {
    pub staging_path: PathBuf,
    pub relative_path: String,
    pub kind: String,
    pub incomplete: bool,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub output: Value,
    pub error: Option<String>,
    pub staged_artifacts: Vec<StagedArtifact>,
    pub validation_criteria: Option<Value>,
    pub state: ToolExitState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExitState {
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

impl ToolResult {
    pub fn success(tool_call_id: &str, output: Value) -> Self {
        Self {
            tool_call_id: tool_call_id.to_string(),
            output,
            error: None,
            staged_artifacts: Vec::new(),
            validation_criteria: None,
            state: ToolExitState::Completed,
        }
    }

    pub fn failure(tool_call_id: &str, error: impl Into<String>) -> Self {
        let error = error.into();
        Self {
            tool_call_id: tool_call_id.to_string(),
            output: json!({"ok": false, "error": error}),
            error: Some(error),
            staged_artifacts: Vec::new(),
            validation_criteria: None,
            state: ToolExitState::Failed,
        }
    }
}

struct UnavailableToolExecutor;
impl ToolExecutor for UnavailableToolExecutor {
    fn execute(&self, call: &ToolCall, _context: &ToolExecutionContext) -> ToolResult {
        ToolResult::failure(
            &call.id,
            format!("capability is not registered: {}", call.name),
        )
    }
}

/// Shared Rust authorities used by every active session. The contained SQLite
/// writers are serialized per authority, while provider streaming and runtime
/// controls remain independent.
#[derive(Clone)]
pub struct RuntimeAuthorities {
    ledger: Arc<Mutex<LedgerWriter>>,
    idempotency: Arc<Mutex<IdempotencyWriter>>,
    approvals: Arc<Mutex<ApprovalWriter>>,
    inbox: Arc<InboxStore>,
}

impl RuntimeAuthorities {
    pub fn open(state_dir: impl AsRef<Path>) -> Result<Self, String> {
        let state_dir = state_dir.as_ref();
        fs::create_dir_all(state_dir).map_err(|error| error.to_string())?;
        Ok(Self {
            ledger: Arc::new(Mutex::new(
                LedgerWriter::open(state_dir.join("run_events.db"))
                    .map_err(|error| error.to_string())?,
            )),
            idempotency: Arc::new(Mutex::new(
                IdempotencyWriter::open(state_dir.join("side_effects.db"))
                    .map_err(|error| error.to_string())?,
            )),
            approvals: Arc::new(Mutex::new(
                ApprovalWriter::open(state_dir.join("audit_events.db").to_string_lossy().as_ref())
                    .map_err(|error| error.to_string())?,
            )),
            inbox: Arc::new(
                InboxStore::open(state_dir.join("inbox.json"))
                    .map_err(|error| error.to_string())?,
            ),
        })
    }

    pub fn ledger(&self) -> Arc<Mutex<LedgerWriter>> {
        self.ledger.clone()
    }

    /// Read-write handle to the side-effect idempotency authority.
    pub fn idempotency(&self) -> Arc<Mutex<IdempotencyWriter>> {
        self.idempotency.clone()
    }

    pub fn inbox(&self) -> Arc<InboxStore> {
        self.inbox.clone()
    }

    /// Cold-start restart recovery: rebuild Run state from the sole durable
    /// Run authority (the ledger) and close every run that was interrupted by
    /// a crash, without ever auto-replaying a real side effect.
    ///
    /// State taxonomy on restart (ADR-042 run lifecycle + checkpoints):
    /// - **Recoverable** (latest checkpoint phase is `awaiting_approval` /
    ///   `awaiting_user` / `awaiting_question` / `awaiting_directory` /
    ///   `awaiting_plan`): the run was paused on a human action. It is left
    ///   open (`running`) so its approval / inbox / interaction state is
    ///   restored, not destroyed. Its side effects are not swept.
    /// - **Executing** (no recoverable checkpoint): the run was actively
    ///   executing when the process died. It is closed with a synthetic
    ///   `run.interrupted` (terminal) and every stale `Planned` / `Executing`
    ///   side effect is swept to `Uncertain` — never re-executed. An operator
    ///   resolves each `Uncertain` entry via `resolve_uncertain`.
    /// - **Terminal** (`completed` / `failed` / `interrupted` / `skipped` /
    ///   `cancelled`): already closed; not touched. Never re-executed.
    ///
    /// Idempotent: a second invocation finds no open runs and no uncommitted
    /// side effects, so it is a no-op. This holds for duplicate restarts and
    /// repeated reconciliation.
    ///
    /// The ledger is the single Run authority. The transcript and the
    /// `recovery` column are rebuild evidence only and are not consulted to
    /// decide whether a run may be replayed.
    pub fn recover_interrupted_runs(&self) -> Result<RecoveryReport, String> {
        let ledger = self.ledger.lock().unwrap();
        let reader = ledger.reader().map_err(|error| error.to_string())?;
        let open_runs = reader.open_runs().map_err(|error| error.to_string())?;
        if open_runs.is_empty() {
            return Ok(RecoveryReport::default());
        }

        // Partition open runs by their latest checkpoint phase. A run paused on
        // a human action is recoverable and must survive the restart; a run that
        // was mid-execution is interrupted.
        let checkpoint_reader = CheckpointReader::from_reader(reader);
        let mut interrupted = Vec::new();
        let mut recovered_waiting = Vec::new();
        for run_id in &open_runs {
            let recoverable = checkpoint_reader
                .latest(run_id)
                .map_err(|error| error.to_string())?
                .is_some_and(|checkpoint| checkpoint.recoverable);
            if recoverable {
                recovered_waiting.push(run_id.clone());
            } else {
                interrupted.push(run_id.clone());
            }
        }
        drop(checkpoint_reader);

        // Close every interrupted run with a synthetic `run.interrupted`. The
        // ledger state machine enforces `running | resumed -> interrupted`.
        let ts = now_ts();
        for run_id in &interrupted {
            ledger
                .transition(
                    run_id,
                    "run.interrupted",
                    "system",
                    ts,
                    &json!({"reason": "crashed"}),
                    "",
                )
                .map_err(|error| error.to_string())?;
        }
        drop(ledger);

        // Sweep stale side effects for the interrupted runs to `Uncertain`.
        // Recoverable (waiting) runs are intentionally excluded: their
        // `Planned` side effects represent intent awaiting a human decision,
        // not a crash mid-flight.
        let swept = if interrupted.is_empty() {
            Vec::new()
        } else {
            let idempotency = self.idempotency.lock().unwrap();
            idempotency
                .sweep_stale(&interrupted)
                .map_err(|error| error.to_string())?
        };

        Ok(RecoveryReport {
            interrupted_runs: interrupted,
            recovered_waiting,
            swept_side_effects: swept,
        })
    }
}

/// Outcome of a cold-start restart recovery sweep.
#[derive(Debug, Clone, Default)]
pub struct RecoveryReport {
    /// Runs that were mid-execution at crash and are now closed as
    /// `interrupted` (terminal). Their stale side effects were swept to
    /// `Uncertain` and must never be auto-replayed.
    pub interrupted_runs: Vec<String>,
    /// Runs that were paused on a human action (approval / interaction) and
    /// are left open so their waiting state is restored.
    pub recovered_waiting: Vec<String>,
    /// Side-effect rows swept from `Planned` / `Executing` to `Uncertain`.
    pub swept_side_effects: Vec<SideEffectEntry>,
}

#[derive(Clone)]
pub struct RuntimeConfig {
    /// Product-facing routed id (for example `anthropic:claude-sonnet-4-6`).
    /// `model` below is the provider-facing bare id.
    pub model_id: String,
    pub model: String,
    pub protocol: String,
    pub api_key: String,
    pub base_url: String,
    pub max_iterations: usize,
    pub max_retries: u32,
    pub ttft_timeout: Option<f64>,
    pub tool_timeout: Option<f64>,
    pub model_settings: Value,
    pub system_prompt: Option<String>,
    pub workspace: Option<String>,
    /// Unattended runs park approval requests in the Inbox authority instead
    /// of waiting on an in-composer response.
    pub unattended: bool,
    /// Execution mode drives the Plan-mode Trust constraint in the Policy
    /// layer. Default is `Execute`. `Plan` hard-rejects any non-read-only
    /// capability regardless of model behavior.
    pub mode: crate::policy::ExecutionMode,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            model_id: String::new(),
            model: String::new(),
            protocol: "openai_chat".to_string(),
            api_key: String::new(),
            base_url: "https://api.openai.com/v1".to_string(),
            max_iterations: DEFAULT_MAX_ITERATIONS,
            max_retries: 2,
            ttft_timeout: None,
            tool_timeout: None,
            model_settings: json!({}),
            system_prompt: None,
            workspace: None,
            unattended: false,
            mode: crate::policy::ExecutionMode::Execute,
        }
    }
}

pub struct RuntimeHost {
    config: RuntimeConfig,
    messages: Vec<Value>,
    cancel: Arc<AtomicBool>,
    provider_cancel: Arc<AtomicBool>,
    steering: Arc<SteeringQueue>,
    sequence: Arc<Mutex<u64>>,
    session_id: String,
    authorities: Option<RuntimeAuthorities>,
    tools: Option<Value>,
    tool_executor: Arc<dyn ToolExecutor>,
    approvals: Arc<ApprovalController>,
    interactions: Arc<InteractionController>,
    runtime_state: Option<Arc<Mutex<RuntimeState>>>,
    run_id: Option<String>,
    sink: Arc<dyn EventSink>,
    event_error: Arc<Mutex<Option<String>>>,
}

/// A queued steering/follow-up instruction: (text, optional MessageSource sidecar).
type SteeringItem = (String, Option<Value>);
type SteeringQueue = Mutex<Vec<SteeringItem>>;

#[derive(Debug)]
struct QueuedRun {
    input: String,
    attachments: Vec<Value>,
    source: Option<Value>,
    run_id: String,
}

enum RuntimeOperation {
    Run(QueuedRun),
    Resume { run_id: String },
    Retry { run_id: String },
}

enum RuntimeCommand {
    Execute(RuntimeOperation),
    SwitchModel {
        change: ModelChange,
        reply: mpsc::Sender<Result<Option<String>, String>>,
    },
    RefreshRuntime {
        config: Box<RuntimeConfig>,
        tools: Value,
        reply: mpsc::Sender<Result<Option<String>, String>>,
    },
    Truncate {
        index: usize,
        reply: mpsc::Sender<Result<usize, String>>,
    },
}

enum ModelChange {
    LegacyId(String),
    Resolved(Box<RuntimeConfig>),
}

/// Cloneable control surface for a session-owned background runtime.
///
/// The worker thread has exclusive ownership of [`RuntimeHost`]. The handle
/// exposes only short-lived state locks, cooperative cancellation tokens and
/// bounded queues, so steering/cancel/follow-up remain responsive while a
/// provider request or capability is in flight.
pub struct RuntimeHandle {
    session_id: String,
    command_tx: mpsc::Sender<RuntimeCommand>,
    cancel: Arc<AtomicBool>,
    provider_cancel: Arc<AtomicBool>,
    steering: Arc<SteeringQueue>,
    follow_ups: Arc<Mutex<VecDeque<QueuedRun>>>,
    state: Arc<Mutex<RuntimeState>>,
    messages: Arc<RwLock<Vec<Value>>>,
    approvals: Arc<ApprovalController>,
    interactions: Arc<InteractionController>,
}

fn now_ts() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn is_retryable_error(e: &str) -> bool {
    let lower = e.to_lowercase();
    lower.contains("429")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("504")
        || lower.contains("connection")
        || lower.contains("timeout")
        || lower.contains("transport")
}

fn classify_transient_error(e: &str) -> String {
    let lower = e.to_lowercase();
    if lower.contains("429") || lower.contains("rate") {
        "RateLimit".to_string()
    } else if lower.contains("50") || lower.contains("server") {
        "ServerError".to_string()
    } else if lower.contains("timeout") || lower.contains("ttft") {
        "TTFTTimeout".to_string()
    } else if lower.contains("connection") || lower.contains("transport") {
        "ConnectionError".to_string()
    } else {
        "Unknown".to_string()
    }
}

fn split_data_url(value: &str) -> Option<(&str, &str)> {
    let rest = value.strip_prefix("data:")?;
    let (metadata, data) = rest.split_once(',')?;
    metadata
        .strip_suffix(";base64")
        .map(|media_type| (media_type, data))
}

/// Convert the canonical transcript attachment shape at the provider boundary.
/// The transcript remains provider-neutral and therefore survives model changes.
fn provider_user_content(protocol: &str, text: &str, attachments: &[Value]) -> Value {
    if protocol == "anthropic" {
        let mut blocks = Vec::new();
        if !text.is_empty() {
            blocks.push(json!({"type": "text", "text": text}));
        }
        for attachment in attachments {
            let kind = attachment
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let name = attachment
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("attachment");
            if kind == "text" {
                if let Some(value) = attachment.get("text").and_then(Value::as_str) {
                    blocks.push(json!({"type": "text", "text": format!("<file name=\"{name}\">\n{value}\n</file>")}));
                }
            } else if let Some((media_type, data)) = attachment
                .get("data_url")
                .and_then(Value::as_str)
                .and_then(split_data_url)
            {
                let block_type = if kind == "pdf" { "document" } else { "image" };
                blocks.push(json!({"type": block_type, "source": {
                    "type": "base64", "media_type": media_type, "data": data
                }}));
            }
        }
        return Value::Array(blocks);
    }

    if protocol == "openai_responses" {
        let mut parts = Vec::new();
        if !text.is_empty() {
            parts.push(json!({"type": "input_text", "text": text}));
        }
        for attachment in attachments {
            let kind = attachment
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let name = attachment
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("attachment");
            if kind == "text" {
                if let Some(value) = attachment.get("text").and_then(Value::as_str) {
                    parts.push(json!({"type": "input_text", "text": format!("<file name=\"{name}\">\n{value}\n</file>")}));
                }
            } else if let Some(data_url) = attachment.get("data_url").and_then(Value::as_str) {
                if kind == "pdf" {
                    parts.push(
                        json!({"type": "input_file", "filename": name, "file_data": data_url}),
                    );
                } else {
                    parts.push(json!({"type": "input_image", "image_url": data_url}));
                }
            }
        }
        return Value::Array(parts);
    }

    let mut parts = Vec::new();
    if !text.is_empty() {
        parts.push(json!({"type": "text", "text": text}));
    }
    for attachment in attachments {
        let kind = attachment
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let name = attachment
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("attachment");
        if kind == "text" {
            if let Some(value) = attachment.get("text").and_then(Value::as_str) {
                parts.push(json!({"type": "text", "text": format!("<file name=\"{name}\">\n{value}\n</file>")}));
            }
        } else if kind == "image" {
            if let Some(data_url) = attachment.get("data_url").and_then(Value::as_str) {
                parts.push(json!({"type": "image_url", "image_url": {"url": data_url}}));
            }
        } else {
            parts.push(json!({"type": "text", "text": format!("[Attached PDF: {name}; select an OpenAI Responses or Anthropic model for native PDF input]")}));
        }
    }
    Value::Array(parts)
}

#[derive(Clone)]
struct RuntimeEventEmitter {
    session_id: String,
    sequence: Arc<Mutex<u64>>,
    sink: Arc<dyn EventSink>,
    event_error: Arc<Mutex<Option<String>>>,
}

impl RuntimeEventEmitter {
    fn emit(&self, event: RuntimeEvent) {
        if self.event_error.lock().unwrap().is_some() {
            return;
        }
        let mut sequence = self.sequence.lock().unwrap();
        let next_sequence = *sequence + 1;
        let frame = event.to_frame(&self.session_id, next_sequence);
        if let Err(error) = self.sink.emit(frame) {
            let mut event_error = self.event_error.lock().unwrap();
            if event_error.is_none() {
                *event_error = Some(error);
            }
            return;
        }
        *sequence = next_sequence;
    }
}

/// Provider-private stream frames terminate here. This adapter is the only
/// conversion boundary from provider deltas to the product event protocol;
/// raw provider frames never reach a Tauri/stdout event sink.
struct ProviderEventWriter {
    emitter: RuntimeEventEmitter,
    buffer: Vec<u8>,
    text: String,
    reasoning: String,
}

impl ProviderEventWriter {
    fn new(emitter: RuntimeEventEmitter) -> Self {
        Self {
            emitter,
            buffer: Vec::new(),
            text: String::new(),
            reasoning: String::new(),
        }
    }

    fn process_line(&mut self, line: &[u8]) {
        let Ok(frame) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        let Some(data) = frame.get("data") else {
            return;
        };
        if let Some(text) = data.get("text_delta").and_then(Value::as_str) {
            self.text.push_str(text);
            self.emitter.emit(RuntimeEvent::AssistantDelta {
                text: text.to_string(),
            });
        }
        if let Some(text) = data.get("reasoning_delta").and_then(Value::as_str) {
            self.reasoning.push_str(text);
            self.emitter.emit(RuntimeEvent::ReasoningDelta {
                text: text.to_string(),
            });
        }
    }

    fn process_complete_lines(&mut self) {
        while let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=newline).collect();
            while matches!(line.last(), Some(b'\n' | b'\r')) {
                line.pop();
            }
            if !line.is_empty() {
                self.process_line(&line);
            }
        }
    }

    fn finish(mut self) -> (String, String) {
        self.process_complete_lines();
        if !self.buffer.is_empty() {
            let tail = std::mem::take(&mut self.buffer);
            self.process_line(&tail);
        }
        (self.text, self.reasoning)
    }
}

impl Write for ProviderEventWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        self.process_complete_lines();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.process_complete_lines();
        Ok(())
    }
}

fn validate_schema_value(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array) {
        if !allowed.iter().any(|candidate| candidate == value) {
            return Err(format!(
                "tool schema validation failed at {path}: value is not allowed"
            ));
        }
    }
    if let Some(expected) = schema.get("type").and_then(Value::as_str) {
        let matches = match expected {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => false,
        };
        if !matches {
            return Err(format!(
                "tool schema validation failed at {path}: expected {expected}"
            ));
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return Err(format!(
                        "tool schema validation failed at {path}: missing {key}"
                    ));
                }
            }
        }
        if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
            for (key, item) in object {
                if let Some(property_schema) = properties.get(key) {
                    validate_schema_value(item, property_schema, &format!("{path}.{key}"))?;
                }
            }
        }
    }
    if let (Some(items), Some(item_schema)) = (
        value.as_array(),
        schema.get("items").filter(|item| item.is_object()),
    ) {
        for (index, item) in items.iter().enumerate() {
            validate_schema_value(item, item_schema, &format!("{path}[{index}]"))?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderStreamOutcome {
    Completed,
    Interrupted,
}

impl RuntimeHost {
    pub fn new(session_id: &str, config: RuntimeConfig) -> Self {
        let mut messages = Vec::new();
        if let Some(ref sys) = config.system_prompt {
            messages.push(json!({"role": "system", "content": sys}));
        }
        Self {
            config,
            messages,
            cancel: Arc::new(AtomicBool::new(false)),
            provider_cancel: Arc::new(AtomicBool::new(false)),
            steering: Arc::new(Mutex::new(Vec::new())),
            sequence: Arc::new(Mutex::new(0)),
            session_id: session_id.to_string(),
            authorities: None,
            tools: None,
            tool_executor: Arc::new(UnavailableToolExecutor),
            approvals: Arc::new(ApprovalController::default()),
            interactions: Arc::new(InteractionController::default()),
            runtime_state: None,
            run_id: None,
            sink: Arc::new(NullSink),
            event_error: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_tools(mut self, tools: Value) -> Self {
        self.tools = Some(tools);
        self
    }

    fn replace_tools(&mut self, tools: Value) {
        self.tools = Some(tools);
    }

    pub fn with_tool_executor(mut self, executor: Arc<dyn ToolExecutor>) -> Self {
        self.tool_executor = executor;
        self
    }
    pub fn with_authorities(mut self, authorities: RuntimeAuthorities) -> Self {
        self.authorities = Some(authorities);
        self
    }
    pub fn with_messages(mut self, messages: Vec<Value>) -> Self {
        self.messages = messages;
        self
    }
    pub fn with_run_id(mut self, run_id: String) -> Self {
        self.run_id = Some(run_id);
        self
    }

    pub fn set_run_id(&mut self, run_id: String) {
        self.run_id = Some(run_id);
    }

    /// Install a custom event sink (e.g. Tauri emit). Default is NullSink.
    pub fn with_event_sink(mut self, sink: Arc<dyn EventSink>) -> Self {
        self.sink = sink;
        self
    }

    pub fn reset_cancel(&self) {
        self.cancel.store(false, Ordering::SeqCst);
        self.provider_cancel.store(false, Ordering::SeqCst);
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.provider_cancel.store(true, Ordering::SeqCst);
    }
    pub fn steer(&self, text: &str, source: Option<Value>) {
        self.steering
            .lock()
            .unwrap()
            .push((text.to_string(), source));
        self.provider_cancel.store(true, Ordering::SeqCst);
    }
    pub fn model(&self) -> &str {
        if self.config.model_id.is_empty() {
            &self.config.model
        } else {
            &self.config.model_id
        }
    }
    pub fn messages(&self) -> &[Value] {
        &self.messages
    }
    pub fn truncate_messages(&mut self, index: usize) {
        if index < self.messages.len() {
            self.messages.truncate(index);
        }
    }

    pub fn switch_model(&mut self, model: &str) -> Option<String> {
        if model.is_empty() || model == self.model() {
            return None;
        }
        let had_history = self
            .messages
            .iter()
            .any(|m| m.get("role").and_then(|r| r.as_str()) != Some("system"));
        self.config.model = model.to_string();
        self.config.model_id = model.to_string();
        if had_history {
            let notice = format!("Model switched to {model}");
            self.messages.push(json!({
                "role": "notice", "kind": "model_switch", "text": &notice, "model": model, "ts": now_ts()
            }));
            Some(notice)
        } else {
            None
        }
    }

    pub fn switch_runtime_config(&mut self, config: RuntimeConfig) -> Option<String> {
        let model_id = if config.model_id.is_empty() {
            config.model.clone()
        } else {
            config.model_id.clone()
        };
        if model_id.is_empty() {
            return None;
        }
        let changed = model_id != self.model();
        let had_history = self
            .messages
            .iter()
            .any(|message| message.get("role").and_then(Value::as_str) != Some("system"));
        self.config.model_id = model_id.clone();
        self.config.model = config.model;
        self.config.protocol = config.protocol;
        self.config.api_key = config.api_key;
        self.config.base_url = config.base_url;
        self.config.model_settings = config.model_settings;
        self.config.max_iterations = config.max_iterations;
        self.config.max_retries = config.max_retries;
        self.config.ttft_timeout = config.ttft_timeout;
        self.config.tool_timeout = config.tool_timeout;
        self.config.workspace = config.workspace;
        self.config.unattended = config.unattended;
        if self.config.system_prompt != config.system_prompt {
            self.config.system_prompt = config.system_prompt.clone();
            if let Some(prompt) = config.system_prompt {
                if let Some(system) = self
                    .messages
                    .iter_mut()
                    .find(|message| message.get("role").and_then(Value::as_str) == Some("system"))
                {
                    *system = json!({"role": "system", "content": prompt});
                } else {
                    self.messages
                        .insert(0, json!({"role": "system", "content": prompt}));
                }
            } else {
                self.messages.retain(|message| {
                    message.get("role").and_then(Value::as_str) != Some("system")
                });
            }
        }
        if changed && had_history {
            let notice = format!("Model switched to {model_id}");
            self.messages.push(json!({
                "role": "notice", "kind": "model_switch", "text": &notice,
                "model": model_id, "ts": now_ts()
            }));
            self.emit_event(RuntimeEvent::ModelChanged { model: model_id });
            Some(notice)
        } else {
            None
        }
    }

    fn emit_event(&self, event: RuntimeEvent) {
        self.event_emitter().emit(event);
    }

    fn clear_event_error(&self) {
        *self.event_error.lock().unwrap() = None;
    }

    fn ensure_event_delivery(&self) -> Result<(), String> {
        match self.event_error.lock().unwrap().clone() {
            Some(error) => Err(format!("runtime event persistence failed: {error}")),
            None => Ok(()),
        }
    }

    fn event_emitter(&self) -> RuntimeEventEmitter {
        RuntimeEventEmitter {
            session_id: self.session_id.clone(),
            sequence: self.sequence.clone(),
            sink: self.sink.clone(),
            event_error: self.event_error.clone(),
        }
    }

    fn ledger_transition(
        &self,
        event_type: &str,
        actor: &str,
        payload: Value,
    ) -> Result<(), String> {
        if let Some(authorities) = &self.authorities {
            let run_id = self.run_id.clone().unwrap_or_else(uuid_v4);
            let ws = self.config.workspace.clone().unwrap_or_default();
            authorities
                .ledger
                .lock()
                .unwrap()
                .transition(&run_id, event_type, actor, now_ts(), &payload, &ws)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn ledger_append(&self, event_type: &str, actor: &str, payload: Value) -> Result<(), String> {
        if let Some(authorities) = &self.authorities {
            let run_id = self.run_id.clone().unwrap_or_else(uuid_v4);
            let workspace = self.config.workspace.clone().unwrap_or_default();
            authorities
                .ledger
                .lock()
                .unwrap()
                .append(&run_id, event_type, actor, now_ts(), &payload, &workspace)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn fail_after_ledger(
        &self,
        call: &ToolCall,
        event_type: &str,
        actor: &str,
        payload: Value,
        primary_error: impl Into<String>,
    ) -> ToolResult {
        let primary_error = primary_error.into();
        match self.ledger_append(event_type, actor, payload) {
            Ok(()) => ToolResult::failure(&call.id, primary_error),
            Err(error) => ToolResult::failure(
                &call.id,
                format!("{primary_error}; failed to persist {event_type}: {error}"),
            ),
        }
    }

    fn mark_tool_uncertain(
        &self,
        authorities: &RuntimeAuthorities,
        run_id: &str,
        call: &ToolCall,
        event_type: &str,
        actor: &str,
        payload: Value,
    ) -> Result<(), String> {
        {
            let writer = authorities.idempotency.lock().unwrap();
            writer.mark_uncertain(run_id, &call.id).map_err(|error| {
                format!("failed to persist uncertain side-effect state: {error}")
            })?;
        }
        self.ledger_append(event_type, actor, payload)
            .map_err(|error| format!("failed to persist {event_type}: {error}"))
    }

    fn mark_tool_failed(
        &self,
        authorities: &RuntimeAuthorities,
        run_id: &str,
        call: &ToolCall,
        failure_reason: &str,
        event: (&str, &str, Value),
    ) -> Result<(), String> {
        let (event_type, actor, payload) = event;
        {
            let writer = authorities.idempotency.lock().unwrap();
            writer
                .mark_failed(run_id, &call.id, failure_reason)
                .map_err(|error| format!("failed to persist failed side-effect state: {error}"))?;
        }
        self.ledger_append(event_type, actor, payload)
            .map_err(|error| format!("failed to persist {event_type}: {error}"))
    }

    fn finish_run_ledger(&self, result: &Result<String, String>) -> Result<(), String> {
        match result {
            Ok(status) if status == "completed" => {
                self.ledger_transition("run.completed", "system", json!({"kind": "run"}))
            }
            Ok(status) if status == "interrupted" => {
                self.ledger_transition("run.interrupted", "system", json!({"kind": "run"}))
            }
            Ok(status) => self.ledger_transition(
                "run.failed",
                "system",
                json!({"kind": "run", "reason": status}),
            ),
            Err(error) => self.ledger_transition(
                "run.failed",
                "system",
                json!({"reason": error, "kind": "run"}),
            ),
        }
    }

    fn tool_contract(&self, call: &ToolCall) -> Result<(Value, Option<ToolMetadata>), String> {
        let definitions = self
            .tools
            .as_ref()
            .and_then(Value::as_array)
            .ok_or_else(|| "no capability contracts are registered".to_string())?;
        let definition = definitions
            .iter()
            .find(|definition| {
                definition.get("name").and_then(Value::as_str) == Some(call.name.as_str())
                    || definition
                        .get("function")
                        .and_then(|function| function.get("name"))
                        .and_then(Value::as_str)
                        == Some(call.name.as_str())
            })
            .ok_or_else(|| format!("capability contract not found: {}", call.name))?;
        let function = definition.get("function").unwrap_or(definition);
        let schema = function
            .get("parameters")
            .or_else(|| definition.get("parameters"))
            .cloned()
            .unwrap_or_else(|| json!({"type": "object"}));
        validate_schema_value(&call.arguments, &schema, "arguments")?;
        let metadata = [
            definition.get("metadata"),
            definition.get("x-delta"),
            definition.get("x_delta"),
            function.get("metadata"),
            function.get("x-delta"),
            function.get("x_delta"),
        ]
        .into_iter()
        .flatten()
        .find_map(|value| serde_json::from_value::<ToolMetadata>(value.clone()).ok());
        Ok((schema, metadata))
    }

    fn policy_for(
        &self,
        call: &ToolCall,
        metadata: Option<ToolMetadata>,
    ) -> Result<(RiskLevel, Decision), String> {
        let level = policy::classify(&call.name, Some(&call.arguments), metadata.as_ref());
        let explicitly_gated = metadata
            .as_ref()
            .and_then(|item| item.requires_approval)
            .unwrap_or(true);
        let auto_allowed = level <= RiskLevel::L1 && !explicitly_gated;
        let workspace = self.config.workspace.clone().unwrap_or_default();
        let roots = if workspace.is_empty() {
            Vec::new()
        } else {
            vec![RootEntry {
                path: workspace.clone(),
                writable: true,
            }]
        };
        let evaluated = policy::evaluate(PolicyEvaluateInput {
            tool_name: call.name.clone(),
            arguments: Some(call.arguments.clone()),
            metadata,
            decision: Decision {
                allowed: auto_allowed,
                reason: if auto_allowed {
                    "auto-approved by Rust policy".to_string()
                } else {
                    format!("explicit approval required for {level:?}")
                },
                needs_user: !auto_allowed,
                rule: if auto_allowed {
                    "runtime.auto_low_risk".to_string()
                } else {
                    String::new()
                },
                grant: if auto_allowed {
                    "policy".to_string()
                } else {
                    String::new()
                },
            },
            level: level as i64,
            workspace_root: workspace,
            roots,
            mode: self.config.mode,
        })
        .map_err(|error| error.to_string())?;
        let evaluated_level = RiskLevel::from_i64(evaluated.level)
            .ok_or_else(|| "policy returned an invalid risk level".to_string())?;
        Ok((evaluated_level, evaluated.decision))
    }

    fn set_runtime_state(&self, state: RuntimeState) {
        if let Some(runtime_state) = &self.runtime_state {
            *runtime_state.lock().unwrap() = state;
        }
    }

    fn register_checkpoint(
        &self,
        phase: &str,
        pending_tool_call: Option<Value>,
        pending_inbox_item_id: Option<String>,
        artifacts: Vec<Value>,
        error: Option<String>,
    ) -> Result<(), String> {
        let Some(authorities) = &self.authorities else {
            return Ok(());
        };
        let run_id = self.run_id.clone().unwrap_or_else(uuid_v4);
        let workspace = self.config.workspace.clone().unwrap_or_default();
        let ledger = authorities.ledger.lock().unwrap();
        CheckpointWriter::new(&ledger)
            .register(
                CheckpointRegisterInput {
                    checkpoint_id: None,
                    run_id,
                    session_id: self.session_id.clone(),
                    phase: phase.to_string(),
                    pending_tool_call,
                    pending_inbox_item_id,
                    last_event_seq: Some(*self.sequence.lock().unwrap() as i64),
                    todo_summary: Vec::new(),
                    recent_artifacts: artifacts,
                    error,
                },
                now_ts(),
                &workspace,
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn run(&mut self, user_input: &str, source: Option<Value>) -> Result<Value, String> {
        self.run_with_attachments(user_input, &[], source)
    }

    pub fn run_with_attachments(
        &mut self,
        user_input: &str,
        attachments: &[Value],
        source: Option<Value>,
    ) -> Result<Value, String> {
        self.clear_event_error();
        self.emit_event(RuntimeEvent::TurnStart {
            input: Value::String(user_input.to_string()),
            attachments: attachments.to_vec(),
            source: source.clone(),
            run_id: self.run_id.clone(),
        });
        self.ensure_event_delivery()?;
        let mut message = json!({"role": "user", "content": user_input, "ts": now_ts()});
        if !attachments.is_empty() {
            message["attachments"] = Value::Array(attachments.to_vec());
        }
        if let Some(src) = &source {
            message["source"] = src.clone();
        }
        self.messages.push(message);
        self.ledger_transition("run.started", "user", json!({"kind": "run"}))?;
        let result = self.loop_turn();
        if result.is_err() {
            self.emit_event(RuntimeEvent::TurnEnd {
                status: "failed".to_string(),
                iterations: 0,
            });
        }
        self.finish_run_ledger(&result)?;
        result.map(|s| json!({"status": s}))
    }

    pub fn resume(&mut self) -> Result<Value, String> {
        self.clear_event_error();
        self.emit_event(RuntimeEvent::TurnStart {
            input: Value::String("(resumed)".to_string()),
            attachments: Vec::new(),
            source: None,
            run_id: self.run_id.clone(),
        });
        self.ensure_event_delivery()?;
        self.ledger_transition("run.started", "system", json!({"kind": "resume"}))?;
        let result = self.loop_turn();
        if result.is_err() {
            self.emit_event(RuntimeEvent::TurnEnd {
                status: "failed".to_string(),
                iterations: 0,
            });
        }
        self.finish_run_ledger(&result)?;
        result.map(|status| json!({"status": status}))
    }

    pub fn retry(&mut self) -> Result<Value, String> {
        self.clear_event_error();
        let tail_is_error = self.messages.iter().rev().any(|m| {
            m.get("role").and_then(|r| r.as_str()) == Some("notice")
                && m.get("kind").and_then(|k| k.as_str()) == Some("error")
        });
        if !tail_is_error {
            return Ok(json!({"status": "skipped"}));
        }
        if let Some(pos) = self.messages.iter().rposition(|m| {
            m.get("role").and_then(|r| r.as_str()) == Some("notice")
                && m.get("kind").and_then(|k| k.as_str()) == Some("error")
        }) {
            self.messages.remove(pos);
        }
        self.emit_event(RuntimeEvent::TurnStart {
            input: Value::String(String::new()),
            attachments: Vec::new(),
            source: None,
            run_id: self.run_id.clone(),
        });
        self.ensure_event_delivery()?;
        self.ledger_transition("run.started", "system", json!({"kind": "retry"}))?;
        let result = self.loop_turn();
        if result.is_err() {
            self.emit_event(RuntimeEvent::TurnEnd {
                status: "failed".to_string(),
                iterations: 0,
            });
        }
        self.finish_run_ledger(&result)?;
        result.map(|status| json!({"status": status}))
    }
}

#[cfg(test)]
mod tests;
