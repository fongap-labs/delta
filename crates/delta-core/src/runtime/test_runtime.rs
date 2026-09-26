use super::*;
use std::io::{BufRead, BufReader, Read};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicUsize;

#[test]
fn test_runtime_config_default() {
    let cfg = RuntimeConfig::default();
    assert_eq!(cfg.protocol, "openai_chat");
    assert_eq!(cfg.max_iterations, DEFAULT_MAX_ITERATIONS);
    assert_eq!(cfg.max_retries, 2);
}

#[test]
fn test_event_to_frame() {
    let event = RuntimeEvent::TurnStart {
        input: Value::String("hello".to_string()),
        attachments: Vec::new(),
        source: None,
        run_id: Some("run-1".to_string()),
    };
    let frame = event.to_frame("session-1", 1);
    assert_eq!(frame["type"], "turn_start");
    assert_eq!(frame["version"], 1);
    assert_eq!(frame["sessionId"], "session-1");
    assert_eq!(frame["sequence"], 1);
    assert_eq!(frame["payload"]["run_id"], "run-1");
}

#[test]
fn test_cancel_flag() {
    let host = RuntimeHost::new("s1", RuntimeConfig::default());
    assert!(!host.cancel.load(Ordering::Relaxed));
    host.cancel();
    assert!(host.cancel.load(Ordering::Relaxed));
    host.reset_cancel();
    assert!(!host.cancel.load(Ordering::Relaxed));
}

#[test]
fn test_steering_queue() {
    let mut host = RuntimeHost::new("s1", RuntimeConfig::default());
    host.steer("change direction", None);
    assert!(host.drain_steering());
    assert!(!host.drain_steering());
}

#[test]
fn test_follow_up_queue_belongs_to_active_handle() {
    let handle = RuntimeHandle::spawn(RuntimeHost::new("s1", RuntimeConfig::default()))
        .expect("spawn runtime");
    *handle.state.lock().unwrap() = RuntimeState::Running;
    let run_id = handle
        .follow_up("then do this", None)
        .expect("queue follow-up");
    let queued = handle.follow_ups.lock().unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].input, "then do this");
    assert_eq!(queued[0].run_id, run_id);
}

#[test]
fn test_switch_model() {
    let mut host = RuntimeHost::new(
        "s1",
        RuntimeConfig {
            model: "gpt-5.5".to_string(),
            ..Default::default()
        },
    );
    host.messages.push(json!({"role": "user", "content": "hi"}));
    let notice = host.switch_model("claude-sonnet-4-6");
    assert!(notice.is_some());
    assert_eq!(host.model(), "claude-sonnet-4-6");
    assert!(host.switch_model("claude-sonnet-4-6").is_none());
}

#[test]
fn test_replace_tools() {
    let mut host = RuntimeHost::new("s1", RuntimeConfig::default())
        .with_tools(json!([{"type": "function", "function": {"name": "old"}}]));
    host.replace_tools(json!([{"type": "function", "function": {"name": "new"}}]));
    assert_eq!(
        host.tools,
        Some(json!([{"type": "function", "function": {"name": "new"}}]))
    );
}

#[test]
fn test_truncate_messages() {
    let mut host = RuntimeHost::new("s1", RuntimeConfig::default());
    host.messages.push(json!({"role": "user", "content": "a"}));
    host.messages
        .push(json!({"role": "assistant", "content": "b"}));
    host.messages.push(json!({"role": "user", "content": "c"}));
    host.truncate_messages(1);
    assert_eq!(host.messages().len(), 1);
}

#[test]
fn test_outbound_strips_sidecars() {
    let host = RuntimeHost::new("s1", RuntimeConfig::default());
    let msgs = vec![
        json!({"role": "user", "content": "hello", "ts": 123.0, "source": {"connector": "slack"}}),
        json!({"role": "assistant", "content": "hi"}),
        json!({"role": "notice", "kind": "error", "text": "bad"}),
    ];
    let host = host.with_messages(msgs);
    let outbound = host.outbound_messages();
    assert_eq!(outbound.len(), 2);
    assert!(outbound[0].get("ts").is_none());
    assert!(outbound[0].get("source").is_none());
    assert!(outbound[1].get("ts").is_none());
}

#[test]
fn canonical_attachments_convert_only_at_provider_boundary() {
    let host = RuntimeHost::new(
        "s1",
        RuntimeConfig {
            protocol: "anthropic".to_string(),
            ..Default::default()
        },
    )
    .with_messages(vec![json!({
        "role": "user", "content": "Review",
        "attachments": [{"kind": "text", "name": "facts.txt", "text": "one"}]
    })]);
    let outbound = host.outbound_messages();
    assert!(outbound[0].get("attachments").is_none());
    assert_eq!(outbound[0]["content"][0]["text"], "Review");
    assert!(outbound[0]["content"][1]["text"]
        .as_str()
        .unwrap()
        .contains("facts.txt"));
}

#[test]
fn interaction_controller_delivers_one_typed_response() {
    let controller = InteractionController::default();
    let receiver = controller.begin("call-1", "question").unwrap();
    controller
        .resolve("question", Some("call-1"), json!({"answer": "A"}))
        .unwrap();
    assert_eq!(receiver.recv().unwrap()["answer"], "A");
    assert!(controller
        .resolve("question", Some("call-1"), json!({"answer": "B"}))
        .is_err());
}

#[test]
fn test_is_retryable() {
    assert!(is_retryable_error("HTTP 429: rate limited"));
    assert!(is_retryable_error("HTTP 502: bad gateway"));
    assert!(is_retryable_error("HTTP 503: service unavailable"));
    assert!(is_retryable_error("HTTP 504: gateway timeout"));
    assert!(is_retryable_error("connection refused"));
    assert!(is_retryable_error("timeout waiting for response"));
    assert!(!is_retryable_error("HTTP 400: bad request"));
    assert!(!is_retryable_error("invalid api key"));
}

#[test]
fn tool_schema_validation_rejects_missing_required_arguments() {
    let schema = json!({
        "type": "object",
        "required": ["path"],
        "properties": {"path": {"type": "string"}}
    });
    assert!(validate_schema_value(&json!({}), &schema, "arguments").is_err());
    assert!(validate_schema_value(&json!({"path": "out.txt"}), &schema, "arguments").is_ok());
}

struct CountingExecutor {
    calls: Arc<AtomicUsize>,
    staged: Option<StagedArtifact>,
}

impl ToolExecutor for CountingExecutor {
    fn execute(&self, call: &ToolCall, _context: &ToolExecutionContext) -> ToolResult {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut result = ToolResult::success(&call.id, json!({"ok": true}));
        if let Some(staged) = &self.staged {
            result.staged_artifacts.push(staged.clone());
        }
        result
    }
}

struct ProgressLedgerFailureExecutor {
    ledger_db: PathBuf,
    calls: Arc<AtomicUsize>,
}

impl ToolExecutor for ProgressLedgerFailureExecutor {
    fn execute(&self, call: &ToolCall, context: &ToolExecutionContext) -> ToolResult {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let connection = rusqlite::Connection::open(&self.ledger_db).unwrap();
        connection.execute_batch("DROP TABLE run_events").unwrap();
        (context.progress)(CapabilityProgress {
            job_id: "job-progress-failure".to_string(),
            fraction: 0.5,
            stage: Some("working".to_string()),
            message: Some("halfway".to_string()),
        });
        ToolResult::success(&call.id, json!({"ok": true}))
    }
}

fn tool_contract(risk: &str, requires_approval: bool) -> Value {
    json!([{
        "type": "function",
        "function": {
            "name": "write_report",
            "parameters": {
                "type": "object",
                "required": ["path"],
                "properties": {"path": {"type": "string"}}
            },
            "metadata": {
                "risk_level": risk,
                "requires_approval": requires_approval,
                "category": if risk == "low" { "read" } else { "filesystem" },
                "capabilities": []
            }
        }
    }])
}

#[test]
fn tool_execution_composes_policy_lifecycle_validation_checkpoint_and_ledger() {
    let temp = tempfile::tempdir().unwrap();
    let authorities = RuntimeAuthorities::open(temp.path()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut host = RuntimeHost::new("session-1", RuntimeConfig::default())
        .with_authorities(authorities.clone())
        .with_tools(tool_contract("low", false))
        .with_tool_executor(Arc::new(CountingExecutor {
            calls: calls.clone(),
            staged: None,
        }));
    host.set_run_id("run-1".to_string());
    let result = host.execute_tool_call(&ToolCall {
        id: "call-1".to_string(),
        name: "write_report".to_string(),
        arguments: json!({"path": "report.md"}),
    });
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let events = authorities
        .ledger
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .events("run-1")
        .unwrap();
    let event_types: Vec<&str> = events.iter().map(|event| event.r#type.as_str()).collect();
    for expected in [
        "tool.proposed",
        "tool.approved",
        "tool.started",
        "validation.registered",
        "tool.completed",
        "checkpoint.registered",
    ] {
        assert!(
            event_types.contains(&expected),
            "missing {expected}: {event_types:?}"
        );
    }
    let entry = authorities
        .idempotency
        .lock()
        .unwrap()
        .get("run-1", "call-1")
        .unwrap()
        .unwrap();
    assert_eq!(entry.state, crate::idemlog::SideEffectState::Committed);
    let approvals = authorities
        .approvals
        .lock()
        .unwrap()
        .list(10, Some("session-1"), None, Some("write_report"))
        .unwrap();
    assert_eq!(approvals[0]["status"], "auto_approved");
}

#[test]
fn post_execution_artifact_failure_becomes_uncertain_and_never_reexecutes() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let staging = workspace.join(".delta/staging/run-postprocess");
    fs::create_dir_all(&staging).unwrap();
    let escaped = workspace.join("escaped.md");
    fs::write(&escaped, "candidate").unwrap();
    let authorities = RuntimeAuthorities::open(temp.path().join("state")).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut host = RuntimeHost::new(
        "session-postprocess",
        RuntimeConfig {
            workspace: Some(workspace.to_string_lossy().to_string()),
            ..RuntimeConfig::default()
        },
    )
    .with_authorities(authorities.clone())
    .with_tools(tool_contract("low", false))
    .with_tool_executor(Arc::new(CountingExecutor {
        calls: calls.clone(),
        staged: Some(StagedArtifact {
            staging_path: escaped,
            relative_path: "reports/final.md".to_string(),
            kind: "markdown".to_string(),
            incomplete: false,
        }),
    }));
    host.set_run_id("run-postprocess".to_string());
    let call = ToolCall {
        id: "call-postprocess".to_string(),
        name: "write_report".to_string(),
        arguments: json!({"path": "reports/final.md"}),
    };

    let first = host.execute_tool_call(&call);
    assert!(first.error.is_some());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let entry = authorities
        .idempotency
        .lock()
        .unwrap()
        .get("run-postprocess", "call-postprocess")
        .unwrap()
        .unwrap();
    assert_eq!(entry.state, crate::idemlog::SideEffectState::Uncertain);

    let second = host.execute_tool_call(&call);
    assert!(second
        .error
        .as_deref()
        .is_some_and(|error| error.contains("uncertain")));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn progress_ledger_failure_marks_side_effect_uncertain() {
    let temp = tempfile::tempdir().unwrap();
    let authorities = RuntimeAuthorities::open(temp.path()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut host = RuntimeHost::new("session-progress-failure", RuntimeConfig::default())
        .with_authorities(authorities.clone())
        .with_tools(tool_contract("low", false))
        .with_tool_executor(Arc::new(ProgressLedgerFailureExecutor {
            ledger_db: temp.path().join("run_events.db"),
            calls: calls.clone(),
        }));
    host.set_run_id("run-progress-failure".to_string());

    let result = host.execute_tool_call(&ToolCall {
        id: "call-progress-failure".to_string(),
        name: "write_report".to_string(),
        arguments: json!({"path": "report.md"}),
    });
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| { error.contains("tool progress ledger persistence failed") }));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let entry = authorities
        .idempotency
        .lock()
        .unwrap()
        .get("run-progress-failure", "call-progress-failure")
        .unwrap()
        .unwrap();
    assert_eq!(entry.state, crate::idemlog::SideEffectState::Uncertain);
}

#[test]
fn approval_authority_blocks_execution_until_ipc_resolution() {
    let temp = tempfile::tempdir().unwrap();
    let authorities = RuntimeAuthorities::open(temp.path()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut host = RuntimeHost::new("session-1", RuntimeConfig::default())
        .with_authorities(authorities)
        .with_tools(tool_contract("medium", true))
        .with_tool_executor(Arc::new(CountingExecutor {
            calls: calls.clone(),
            staged: None,
        }));
    host.set_run_id("run-approval".to_string());
    let host = Arc::new(host);
    let worker_host = host.clone();
    let worker = std::thread::spawn(move || {
        worker_host.execute_tool_call(&ToolCall {
            id: "call-approval".to_string(),
            name: "write_report".to_string(),
            arguments: json!({"path": "report.md"}),
        })
    });
    for _ in 0..100 {
        if host.approvals.pending_ids() == ["call-approval"] {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    host.approvals
        .resolve(Some("call-approval"), ApprovalDecision::Once)
        .unwrap();
    let result = worker.join().unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

fn approval_test_host(
    state_dir: &Path,
    calls: Arc<AtomicUsize>,
    unattended: bool,
) -> (Arc<RuntimeHost>, RuntimeAuthorities) {
    let authorities = RuntimeAuthorities::open(state_dir).unwrap();
    let mut host = RuntimeHost::new(
        "session-approval-e2e",
        RuntimeConfig {
            unattended,
            ..RuntimeConfig::default()
        },
    )
    .with_authorities(authorities.clone())
    .with_tools(tool_contract("medium", true))
    .with_tool_executor(Arc::new(CountingExecutor {
        calls,
        staged: None,
    }));
    host.set_run_id("run-approval-e2e".to_string());
    (Arc::new(host), authorities)
}

fn execute_approval_tool(host: Arc<RuntimeHost>) -> std::thread::JoinHandle<ToolResult> {
    std::thread::spawn(move || {
        host.execute_tool_call(&ToolCall {
            id: "call-approval-e2e".to_string(),
            name: "write_report".to_string(),
            arguments: json!({"path": "report.md"}),
        })
    })
}

fn wait_for_approval(host: &RuntimeHost) {
    for _ in 0..200 {
        if host.approvals.pending_ids() == ["call-approval-e2e"] {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("approval request did not become pending");
}

#[test]
fn native_e2e_approval_rejection_never_enters_capability() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (host, authorities) = approval_test_host(temp.path(), calls.clone(), false);
    let worker = execute_approval_tool(host.clone());
    wait_for_approval(&host);

    host.approvals
        .resolve(Some("call-approval-e2e"), ApprovalDecision::Deny)
        .unwrap();
    let result = worker.join().unwrap();

    assert_eq!(result.error.as_deref(), Some("tool call denied by user"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let events = authorities
        .ledger()
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .events("run-approval-e2e")
        .unwrap();
    assert!(events.iter().any(|event| event.r#type == "approval.denied"));
    assert!(events.iter().any(|event| event.r#type == "tool.cancelled"));
}

#[test]
fn native_e2e_cancel_while_approval_pending_is_terminal() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (host, authorities) = approval_test_host(temp.path(), calls.clone(), false);
    let worker = execute_approval_tool(host.clone());
    wait_for_approval(&host);

    host.cancel();
    let result = worker.join().unwrap();

    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("approval was pending")));
    assert!(host.approvals.pending_ids().is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let events = authorities
        .ledger()
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .events("run-approval-e2e")
        .unwrap();
    assert!(events
        .iter()
        .any(|event| event.r#type == "approval.cancelled"));
}

#[test]
fn native_e2e_unattended_approval_is_durable_in_inbox() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (host, authorities) = approval_test_host(temp.path(), calls.clone(), true);
    let worker = execute_approval_tool(host.clone());
    wait_for_approval(&host);

    let pending = authorities
        .inbox()
        .list(Some("session-approval-e2e"), Some("pending"));
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].kind, "approval");
    assert_eq!(
        pending[0].tool_call_id.as_deref(),
        Some("call-approval-e2e")
    );

    authorities.inbox().resolve(&pending[0].id, "once").unwrap();
    host.approvals
        .resolve(Some("call-approval-e2e"), ApprovalDecision::Once)
        .unwrap();
    let result = worker.join().unwrap();

    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        authorities.inbox().get(&pending[0].id).unwrap().state,
        "resolved"
    );
}

struct CancelAwareExecutor {
    started: Mutex<Option<mpsc::Sender<()>>>,
}

impl ToolExecutor for CancelAwareExecutor {
    fn execute(&self, call: &ToolCall, context: &ToolExecutionContext) -> ToolResult {
        if let Some(sender) = self.started.lock().unwrap().take() {
            let _ = sender.send(());
        }
        while !context.cancel.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(5));
        }
        ToolResult {
            tool_call_id: call.id.clone(),
            output: json!({"ok": false}),
            error: Some("capability cancelled".to_string()),
            staged_artifacts: Vec::new(),
            validation_criteria: None,
            state: ToolExitState::Cancelled,
        }
    }
}

#[test]
fn native_e2e_cancel_during_tool_marks_side_effect_uncertain() {
    let temp = tempfile::tempdir().unwrap();
    let authorities = RuntimeAuthorities::open(temp.path()).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let mut host = RuntimeHost::new("session-tool-cancel", RuntimeConfig::default())
        .with_authorities(authorities.clone())
        .with_tools(tool_contract("low", false))
        .with_tool_executor(Arc::new(CancelAwareExecutor {
            started: Mutex::new(Some(started_tx)),
        }));
    host.set_run_id("run-tool-cancel".to_string());
    let host = Arc::new(host);
    let worker_host = host.clone();
    let worker = std::thread::spawn(move || {
        worker_host.execute_tool_call(&ToolCall {
            id: "call-tool-cancel".to_string(),
            name: "write_report".to_string(),
            arguments: json!({"path": "report.md"}),
        })
    });
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    host.cancel();
    let result = worker.join().unwrap();

    assert!(result.error.is_some());
    let entry = authorities
        .idempotency
        .lock()
        .unwrap()
        .get("run-tool-cancel", "call-tool-cancel")
        .unwrap()
        .unwrap();
    assert_eq!(entry.state, crate::idemlog::SideEffectState::Uncertain);
    let events = authorities
        .ledger()
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .events("run-tool-cancel")
        .unwrap();
    assert!(events.iter().any(|event| event.r#type == "tool.cancelled"));
}

#[test]
fn runtime_promotes_and_hashes_only_staged_worker_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let staging = workspace.join(".delta/staging/run-artifact");
    fs::create_dir_all(&staging).unwrap();
    let staged_path = staging.join("candidate.md");
    fs::write(&staged_path, "authoritative artifact").unwrap();
    let authorities = RuntimeAuthorities::open(temp.path().join("state")).unwrap();
    let mut host = RuntimeHost::new(
        "session-1",
        RuntimeConfig {
            workspace: Some(workspace.to_string_lossy().to_string()),
            ..RuntimeConfig::default()
        },
    )
    .with_authorities(authorities.clone())
    .with_tools(tool_contract("low", false))
    .with_tool_executor(Arc::new(CountingExecutor {
        calls: Arc::new(AtomicUsize::new(0)),
        staged: Some(StagedArtifact {
            staging_path: staged_path.clone(),
            relative_path: "reports/final.md".to_string(),
            kind: "markdown".to_string(),
            incomplete: false,
        }),
    }));
    host.set_run_id("run-artifact".to_string());
    let result = host.execute_tool_call(&ToolCall {
        id: "call-artifact".to_string(),
        name: "write_report".to_string(),
        arguments: json!({"path": "reports/final.md"}),
    });
    assert!(result.error.is_none(), "{:?}", result.error);
    assert!(!staged_path.exists());
    assert_eq!(
        fs::read_to_string(workspace.join("reports/final.md")).unwrap(),
        "authoritative artifact"
    );
    let artifacts = authorities
        .ledger
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .events("run-artifact")
        .unwrap();
    assert!(artifacts
        .iter()
        .any(|event| event.r#type == "artifact.registered"));
    assert!(result.output["artifacts"][0]["sha256"]
        .as_str()
        .is_some_and(|hash| hash.len() == 64));
}

#[derive(Default)]
struct CaptureSink {
    frames: Mutex<Vec<Value>>,
}

struct FailOnTurnEndSink {
    frames: Mutex<Vec<Value>>,
}

impl EventSink for FailOnTurnEndSink {
    fn emit(&self, frame: Value) -> Result<(), String> {
        if frame.get("type").and_then(Value::as_str) == Some("turn_end") {
            return Err("simulated persistence failure".to_string());
        }
        self.frames.lock().unwrap().push(frame);
        Ok(())
    }
}

struct MockProvider {
    base_url: String,
    first_response_started: mpsc::Receiver<()>,
    release_first_response: mpsc::Sender<()>,
    request_bodies: Arc<Mutex<Vec<String>>>,
    worker: std::thread::JoinHandle<()>,
}

impl MockProvider {
    fn start(response_count: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock provider");
        let address = listener.local_addr().expect("mock provider address");
        let (started_tx, first_response_started) = mpsc::channel();
        let (release_first_response, release_rx) = mpsc::channel();
        let request_bodies = Arc::new(Mutex::new(Vec::new()));
        let captured = request_bodies.clone();
        let worker = std::thread::spawn(move || {
            for response_index in 0..response_count {
                let (mut stream, _) = listener.accept().expect("accept provider request");
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let body = read_http_request(&mut stream);
                captured.lock().unwrap().push(body);
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
                if response_index == 0 {
                    write_openai_delta(&mut stream, "first");
                    stream.flush().unwrap();
                    started_tx.send(()).unwrap();
                    release_rx
                        .recv_timeout(Duration::from_secs(5))
                        .expect("release first provider response");
                } else {
                    write_openai_delta(&mut stream, "second");
                }
                write!(
                    stream,
                    "data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}\n\ndata: [DONE]\n\n"
                )
                .unwrap();
                stream.flush().unwrap();
            }
        });
        Self {
            base_url: format!("http://{address}/v1"),
            first_response_started,
            release_first_response,
            request_bodies,
            worker,
        }
    }

    fn finish(self) -> Vec<String> {
        self.worker.join().expect("mock provider worker");
        self.request_bodies.lock().unwrap().clone()
    }
}

enum RetryReply {
    Status(u16),
    Disconnect,
    Stall(Duration),
    Complete(&'static str),
    PartialThenClose(&'static str),
    ToolCall,
}

struct RetryMockProvider {
    base_url: String,
    requests: Arc<AtomicUsize>,
    worker: std::thread::JoinHandle<()>,
}

impl RetryMockProvider {
    fn start(replies: Vec<RetryReply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind retry provider");
        let address = listener.local_addr().expect("retry provider address");
        let requests = Arc::new(AtomicUsize::new(0));
        let captured = requests.clone();
        let worker = std::thread::spawn(move || {
            let mut handlers = Vec::new();
            for reply in replies {
                let (mut stream, _) = listener.accept().expect("accept retry request");
                captured.fetch_add(1, Ordering::SeqCst);
                handlers.push(std::thread::spawn(move || {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let _ = read_http_request(&mut stream);
                    match reply {
                        RetryReply::Status(code) => {
                            write!(
                                stream,
                                "HTTP/1.1 {code} Transient\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            )
                            .unwrap();
                        }
                        RetryReply::Disconnect => {}
                        RetryReply::Stall(duration) => {
                            std::thread::sleep(duration);
                        }
                        RetryReply::Complete(text) => {
                            write!(
                                stream,
                                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
                            )
                            .unwrap();
                            write_openai_delta(&mut stream, text);
                            write!(
                                stream,
                                "data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}\n\ndata: [DONE]\n\n"
                            )
                            .unwrap();
                        }
                        RetryReply::PartialThenClose(text) => {
                            write!(
                                stream,
                                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
                            )
                            .unwrap();
                            write_openai_delta(&mut stream, text);
                        }
                        RetryReply::ToolCall => {
                            write!(
                                stream,
                                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
                            )
                            .unwrap();
                            writeln!(
                                stream,
                                "data: {}\n",
                                json!({
                                    "choices": [{
                                        "delta": {"tool_calls": [{
                                            "index": 0,
                                            "id": "call-native-chain",
                                            "function": {
                                                "name": "write_report",
                                                "arguments": "{\"path\":\"report.md\"}"
                                            }
                                        }]},
                                        "finish_reason": null
                                    }]
                                })
                            )
                            .unwrap();
                            write!(
                                stream,
                                "data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"tool_calls\"}}]}}\n\ndata: [DONE]\n\n"
                            )
                            .unwrap();
                        }
                    }
                    let _ = stream.flush();
                }));
            }
            for handler in handlers {
                handler.join().expect("retry response handler");
            }
        });
        Self {
            base_url: format!("http://{address}/v1"),
            requests,
            worker,
        }
    }

    fn finish(self) -> usize {
        self.worker.join().expect("retry provider worker");
        self.requests.load(Ordering::SeqCst)
    }
}

fn read_http_request(stream: &mut TcpStream) -> String {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read request header");
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some(length) = line
            .to_ascii_lowercase()
            .strip_prefix("content-length:")
            .and_then(|value| value.trim().parse::<usize>().ok())
        {
            content_length = length;
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).expect("read request body");
    String::from_utf8(body).expect("request body is utf-8")
}

fn write_openai_delta(stream: &mut TcpStream, text: &str) {
    writeln!(
        stream,
        "data: {}\n",
        json!({"choices": [{"delta": {"content": text}, "finish_reason": null}]})
    )
    .unwrap();
}

fn provider_test_host(base_url: String, sink: Arc<CaptureSink>) -> RuntimeHost {
    RuntimeHost::new(
        "session-1",
        RuntimeConfig {
            model: "test-model".to_string(),
            protocol: "openai_chat".to_string(),
            api_key: "test-key".to_string(),
            base_url,
            max_iterations: 2,
            ..RuntimeConfig::default()
        },
    )
    .with_event_sink(sink)
}

fn retry_test_host(
    base_url: String,
    sink: Arc<CaptureSink>,
    timeout_secs: Option<f64>,
) -> RuntimeHost {
    RuntimeHost::new(
        "session-retry",
        RuntimeConfig {
            model: "test-model".to_string(),
            protocol: "openai_chat".to_string(),
            api_key: "test-key".to_string(),
            base_url,
            max_iterations: 3,
            max_retries: 1,
            ttft_timeout: timeout_secs,
            ..RuntimeConfig::default()
        },
    )
    .with_event_sink(sink)
}

fn wait_for_terminal(handle: &RuntimeHandle) {
    for _ in 0..500 {
        if !handle.state().is_active() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!(
        "runtime did not reach a terminal state: {:?}",
        handle.state()
    );
}

impl EventSink for CaptureSink {
    fn emit(&self, frame: Value) -> Result<(), String> {
        self.frames.lock().unwrap().push(frame);
        Ok(())
    }
}

#[test]
fn native_e2e_basic_answer_stream_reaches_product_event_protocol() {
    let provider = MockProvider::start(1);
    let sink = Arc::new(CaptureSink::default());
    let handle =
        RuntimeHandle::spawn(provider_test_host(provider.base_url.clone(), sink.clone())).unwrap();
    let run_id = handle.run("hello".to_string(), None).unwrap();
    provider
        .first_response_started
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    provider.release_first_response.send(()).unwrap();
    wait_for_terminal(&handle);

    assert_eq!(handle.state(), RuntimeState::Completed);
    assert_eq!(provider.finish().len(), 1);
    let frames = sink.frames.lock().unwrap();
    let types: Vec<&str> = frames
        .iter()
        .filter_map(|frame| frame["type"].as_str())
        .collect();
    assert_eq!(types.first(), Some(&"turn_start"));
    assert!(types.contains(&"assistant_delta"));
    assert!(types.contains(&"assistant_message"));
    assert_eq!(types.last(), Some(&"turn_end"));
    assert_eq!(frames[0]["payload"]["run_id"], run_id);
    assert!(frames.iter().all(|frame| frame["sessionId"] == "session-1"));
}

#[test]
fn native_e2e_model_tool_policy_capability_and_model_continue() {
    let provider = RetryMockProvider::start(vec![
        RetryReply::ToolCall,
        RetryReply::Complete("report complete"),
    ]);
    let temp = tempfile::tempdir().unwrap();
    let authorities = RuntimeAuthorities::open(temp.path()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let sink = Arc::new(CaptureSink::default());
    let host = RuntimeHost::new(
        "session-native-chain",
        RuntimeConfig {
            model: "test-model".to_string(),
            protocol: "openai_chat".to_string(),
            api_key: "test-key".to_string(),
            base_url: provider.base_url.clone(),
            max_iterations: 3,
            ..RuntimeConfig::default()
        },
    )
    .with_authorities(authorities.clone())
    .with_tools(tool_contract("low", false))
    .with_tool_executor(Arc::new(CountingExecutor {
        calls: calls.clone(),
        staged: None,
    }))
    .with_event_sink(sink.clone());
    let handle = RuntimeHandle::spawn(host).unwrap();
    let run_id = handle.run("write the report".to_string(), None).unwrap();
    wait_for_terminal(&handle);

    assert_eq!(handle.state(), RuntimeState::Completed);
    assert_eq!(provider.finish(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let frames = sink.frames.lock().unwrap();
    let types: Vec<&str> = frames
        .iter()
        .filter_map(|frame| frame["type"].as_str())
        .collect();
    for expected in [
        "tool_proposed",
        "tool_started",
        "tool_finished",
        "assistant_message",
        "turn_end",
    ] {
        assert!(types.contains(&expected), "missing {expected}: {types:?}");
    }
    let events = authorities
        .ledger()
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .events(&run_id)
        .unwrap();
    for expected in [
        "run.started",
        "tool.proposed",
        "tool.approved",
        "tool.started",
        "tool.completed",
        "run.completed",
    ] {
        assert!(
            events.iter().any(|event| event.r#type == expected),
            "missing ledger event {expected}"
        );
    }
}

#[test]
fn native_e2e_retries_all_transient_provider_failures_before_output() {
    for status in [429, 502, 503, 504] {
        let provider = RetryMockProvider::start(vec![
            RetryReply::Status(status),
            RetryReply::Complete("recovered"),
        ]);
        let sink = Arc::new(CaptureSink::default());
        let handle = RuntimeHandle::spawn(retry_test_host(
            provider.base_url.clone(),
            sink.clone(),
            None,
        ))
        .unwrap();
        handle.run(format!("retry {status}"), None).unwrap();
        wait_for_terminal(&handle);
        assert_eq!(handle.state(), RuntimeState::Completed, "HTTP {status}");
        assert_eq!(provider.finish(), 2, "HTTP {status}");
        let expected_error_type = if status == 429 {
            "RateLimit"
        } else {
            "ServerError"
        };
        assert!(sink.frames.lock().unwrap().iter().any(|frame| {
            frame["type"] == "error" && frame["payload"]["error_type"] == expected_error_type
        }));
    }

    let provider = RetryMockProvider::start(vec![
        RetryReply::Disconnect,
        RetryReply::Complete("recovered"),
    ]);
    let sink = Arc::new(CaptureSink::default());
    let handle =
        RuntimeHandle::spawn(retry_test_host(provider.base_url.clone(), sink, None)).unwrap();
    handle.run("retry connection".to_string(), None).unwrap();
    wait_for_terminal(&handle);
    assert_eq!(handle.state(), RuntimeState::Completed);
    assert_eq!(provider.finish(), 2);

    let provider = RetryMockProvider::start(vec![
        RetryReply::Stall(Duration::from_millis(100)),
        RetryReply::Complete("recovered"),
    ]);
    let sink = Arc::new(CaptureSink::default());
    let handle =
        RuntimeHandle::spawn(retry_test_host(provider.base_url.clone(), sink, Some(0.05))).unwrap();
    handle.run("retry timeout".to_string(), None).unwrap();
    wait_for_terminal(&handle);
    assert_eq!(handle.state(), RuntimeState::Completed);
    assert_eq!(provider.finish(), 2);
}

#[test]
fn native_e2e_never_retries_after_visible_partial_output() {
    let provider = RetryMockProvider::start(vec![RetryReply::PartialThenClose("visible")]);
    let sink = Arc::new(CaptureSink::default());
    let handle = RuntimeHandle::spawn(retry_test_host(
        provider.base_url.clone(),
        sink.clone(),
        None,
    ))
    .unwrap();
    handle.run("do not duplicate".to_string(), None).unwrap();
    wait_for_terminal(&handle);

    assert_eq!(handle.state(), RuntimeState::Failed);
    assert_eq!(provider.finish(), 1);
    let frames = sink.frames.lock().unwrap();
    assert!(frames.iter().any(|frame| {
        frame["type"] == "assistant_delta" && frame["payload"]["text"] == "visible"
    }));
    assert!(!frames.iter().any(|frame| {
        frame["type"] == "error"
            && frame["payload"]["error"] == "Transient model failure - retrying."
    }));
}

#[test]
fn provider_frames_are_converted_to_runtime_envelopes() {
    let sink = Arc::new(CaptureSink::default());
    let emitter = RuntimeEventEmitter {
        session_id: "session-1".to_string(),
        sequence: Arc::new(Mutex::new(0)),
        sink: sink.clone(),
        event_error: Arc::new(Mutex::new(None)),
    };
    let mut writer = ProviderEventWriter::new(emitter);
    writer
        .write_all(
            b"{\"ok\":true,\"stream\":\"delta\",\"request_id\":\"private\",\"data\":{\"text_delta\":\"hi\"}}\n",
        )
        .unwrap();
    let (text, reasoning) = writer.finish();
    assert_eq!(text, "hi");
    assert!(reasoning.is_empty());

    let frames = sink.frames.lock().unwrap();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["type"], "assistant_delta");
    assert_eq!(frames[0]["version"], 1);
    assert_eq!(frames[0]["sessionId"], "session-1");
    assert_eq!(frames[0]["sequence"], 1);
    assert_eq!(frames[0]["payload"]["text"], "hi");
    assert!(frames[0].get("stream").is_none());
    assert!(frames[0].get("request_id").is_none());
}

#[test]
fn event_sink_failure_fails_run_and_suppresses_later_terminal_events() {
    let sink = Arc::new(FailOnTurnEndSink {
        frames: Mutex::new(Vec::new()),
    });
    let mut host = RuntimeHost::new(
        "session-persistence-failure",
        RuntimeConfig {
            max_iterations: 0,
            ..RuntimeConfig::default()
        },
    )
    .with_event_sink(sink.clone());

    let result = host.run("hello", None);
    assert!(result
        .as_ref()
        .err()
        .is_some_and(|error| { error.contains("runtime event persistence failed") }));
    let frames = sink.frames.lock().unwrap();
    let event_types = frames
        .iter()
        .filter_map(|frame| frame.get("type").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(event_types, vec!["turn_start"]);
}

#[test]
fn runtime_handle_runs_in_background_and_rejects_parallel_run() {
    let sink = Arc::new(CaptureSink::default());
    let host = RuntimeHost::new(
        "session-1",
        RuntimeConfig {
            max_iterations: 0,
            ..RuntimeConfig::default()
        },
    )
    .with_event_sink(sink.clone());
    let handle = RuntimeHandle::spawn(host).expect("spawn runtime");
    let run_id = handle.run("hello".to_string(), None).expect("start run");
    assert!(handle.run("parallel".to_string(), None).is_err());

    for _ in 0..100 {
        if !handle.state().is_active() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(handle.state(), RuntimeState::Failed);
    let frames = sink.frames.lock().unwrap();
    assert_eq!(frames[0]["type"], "turn_start");
    assert_eq!(frames[0]["payload"]["run_id"], run_id);
    assert_eq!(frames[1]["type"], "turn_end");
    assert_eq!(frames[1]["sequence"], 2);
}

#[test]
fn steering_interrupts_provider_and_changes_the_same_run() {
    let provider = MockProvider::start(2);
    let sink = Arc::new(CaptureSink::default());
    let handle = RuntimeHandle::spawn(provider_test_host(provider.base_url.clone(), sink.clone()))
        .expect("spawn runtime");
    let run_id = handle.run("initial".to_string(), None).expect("start run");
    provider
        .first_response_started
        .recv_timeout(Duration::from_secs(5))
        .expect("first provider delta");
    handle.steer("change direction", None).expect("steer run");
    provider.release_first_response.send(()).unwrap();
    wait_for_terminal(&handle);
    assert_eq!(handle.state(), RuntimeState::Completed);

    let request_bodies = provider.finish();
    assert_eq!(request_bodies.len(), 2);
    assert!(request_bodies[1].contains("change direction"));
    let frames = sink.frames.lock().unwrap();
    let turn_starts: Vec<&Value> = frames
        .iter()
        .filter(|frame| frame["type"] == "turn_start")
        .collect();
    assert_eq!(turn_starts.len(), 1);
    assert_eq!(turn_starts[0]["payload"]["run_id"], run_id);
    assert!(frames.iter().all(|frame| frame.get("stream").is_none()));
    assert!(frames
        .windows(2)
        .all(|pair| pair[1]["sequence"].as_u64() > pair[0]["sequence"].as_u64()));
}

#[test]
fn follow_up_is_scheduled_as_a_distinct_run() {
    let provider = MockProvider::start(2);
    let sink = Arc::new(CaptureSink::default());
    let handle = RuntimeHandle::spawn(provider_test_host(provider.base_url.clone(), sink.clone()))
        .expect("spawn runtime");
    let first_run_id = handle.run("first task".to_string(), None).unwrap();
    provider
        .first_response_started
        .recv_timeout(Duration::from_secs(5))
        .expect("first provider delta");
    let follow_up_run_id = handle.follow_up("second task", None).unwrap();
    assert_ne!(first_run_id, follow_up_run_id);
    provider.release_first_response.send(()).unwrap();
    wait_for_terminal(&handle);
    assert_eq!(handle.state(), RuntimeState::Completed);
    assert_eq!(provider.finish().len(), 2);

    let frames = sink.frames.lock().unwrap();
    let run_ids: Vec<&str> = frames
        .iter()
        .filter(|frame| frame["type"] == "turn_start")
        .filter_map(|frame| frame["payload"]["run_id"].as_str())
        .collect();
    assert_eq!(
        run_ids,
        vec![first_run_id.as_str(), follow_up_run_id.as_str()]
    );
}

#[test]
fn cancel_transitions_active_run_to_interrupted() {
    let provider = MockProvider::start(1);
    let sink = Arc::new(CaptureSink::default());
    let handle = RuntimeHandle::spawn(provider_test_host(provider.base_url.clone(), sink.clone()))
        .expect("spawn runtime");
    handle.run("cancel me".to_string(), None).unwrap();
    provider
        .first_response_started
        .recv_timeout(Duration::from_secs(5))
        .expect("first provider delta");
    assert!(handle.cancel());
    assert_eq!(handle.state(), RuntimeState::Cancelling);
    provider.release_first_response.send(()).unwrap();
    wait_for_terminal(&handle);
    assert_eq!(handle.state(), RuntimeState::Interrupted);
    assert_eq!(provider.finish().len(), 1);
    assert!(sink
        .frames
        .lock()
        .unwrap()
        .iter()
        .any(|frame| frame["type"] == "interrupted"));
}

// ------------------------------------------------------------------
// R7 Task 2: Runtime restart recovery closure.
// The ledger is the sole Run authority; recover_interrupted_runs()
// rebuilds from it without ever auto-replaying a real side effect.
// ------------------------------------------------------------------

use crate::checkpoint::CheckpointRegisterInput;
use crate::idemlog::SideEffectState;

fn open_authorities() -> (tempfile::TempDir, RuntimeAuthorities) {
    let dir = tempfile::tempdir().unwrap();
    let auth = RuntimeAuthorities::open(dir.path()).unwrap();
    (dir, auth)
}

fn start_run(auth: &RuntimeAuthorities, run_id: &str) {
    let ledger = auth.ledger.lock().unwrap();
    ledger
        .transition(
            run_id,
            "run.started",
            "user",
            now_ts(),
            &json!({"kind": "run"}),
            "",
        )
        .unwrap();
}

fn side_effect(
    auth: &RuntimeAuthorities,
    run_id: &str,
    tool_call_id: &str,
    state: SideEffectState,
) {
    let idem = auth.idempotency.lock().unwrap();
    let args = json!({"path": "a.txt"});
    idem.record_planned(run_id, tool_call_id, "write_file", &args)
        .unwrap();
    if state != SideEffectState::Planned {
        idem.mark_executing(run_id, tool_call_id).unwrap();
    }
    match state {
        SideEffectState::Committed => {
            idem.commit(
                run_id,
                tool_call_id,
                "write_file",
                &args,
                &json!({"ok": true}),
            )
            .unwrap();
        }
        SideEffectState::Failed => {
            idem.mark_failed(run_id, tool_call_id, "boom").unwrap();
        }
        SideEffectState::Uncertain => {
            idem.mark_uncertain(run_id, tool_call_id).unwrap();
        }
        _ => {}
    }
}

fn register_checkpoint(auth: &RuntimeAuthorities, run_id: &str, phase: &str) {
    let ledger = auth.ledger.lock().unwrap();
    CheckpointWriter::new(&ledger)
        .register(
            CheckpointRegisterInput {
                checkpoint_id: None,
                run_id: run_id.to_string(),
                session_id: "s1".to_string(),
                phase: phase.to_string(),
                pending_tool_call: None,
                pending_inbox_item_id: None,
                last_event_seq: None,
                todo_summary: Vec::new(),
                recent_artifacts: Vec::new(),
                error: None,
            },
            now_ts(),
            "",
        )
        .unwrap();
}

#[test]
fn restart_sweeps_executing_side_effect_to_uncertain_without_replay() {
    let (_dir, auth) = open_authorities();
    start_run(&auth, "run_exec");
    side_effect(&auth, "run_exec", "tc_1", SideEffectState::Executing);

    let report = auth.recover_interrupted_runs().unwrap();
    assert_eq!(report.interrupted_runs, vec!["run_exec".to_string()]);
    assert!(report.recovered_waiting.is_empty());
    assert_eq!(report.swept_side_effects.len(), 1);

    let ledger = auth.ledger.lock().unwrap();
    let reader = ledger.reader().unwrap();
    assert_eq!(reader.run_status("run_exec").unwrap(), "interrupted");

    let idem = auth.idempotency.lock().unwrap();
    let entry = idem.get("run_exec", "tc_1").unwrap().unwrap();
    assert_eq!(entry.state, SideEffectState::Uncertain);
}

#[test]
fn restart_sweeps_planned_side_effect_and_closes_run_before_execution() {
    let (_dir, auth) = open_authorities();
    start_run(&auth, "run_planned");
    side_effect(&auth, "run_planned", "tc_1", SideEffectState::Planned);

    let report = auth.recover_interrupted_runs().unwrap();
    assert_eq!(report.interrupted_runs, vec!["run_planned".to_string()]);
    assert_eq!(report.swept_side_effects.len(), 1);

    let idem = auth.idempotency.lock().unwrap();
    let entry = idem.get("run_planned", "tc_1").unwrap().unwrap();
    assert_eq!(entry.state, SideEffectState::Uncertain);
}

#[test]
fn restart_preserves_committed_side_effect_after_crash() {
    let (_dir, auth) = open_authorities();
    start_run(&auth, "run_committed");
    side_effect(&auth, "run_committed", "tc_1", SideEffectState::Committed);

    let report = auth.recover_interrupted_runs().unwrap();
    assert_eq!(report.interrupted_runs, vec!["run_committed".to_string()]);
    assert!(report.swept_side_effects.is_empty());

    let idem = auth.idempotency.lock().unwrap();
    let entry = idem.get("run_committed", "tc_1").unwrap().unwrap();
    assert_eq!(entry.state, SideEffectState::Committed);
}

#[test]
fn restart_preserves_terminal_run_without_reexecution() {
    let (_dir, auth) = open_authorities();
    start_run(&auth, "run_done");
    let ledger = auth.ledger.lock().unwrap();
    ledger
        .transition(
            "run_done",
            "run.completed",
            "system",
            now_ts(),
            &json!({"status": "completed"}),
            "",
        )
        .unwrap();
    drop(ledger);

    let report = auth.recover_interrupted_runs().unwrap();
    assert!(report.interrupted_runs.is_empty());
    assert!(report.recovered_waiting.is_empty());
}

#[test]
fn restart_restores_run_paused_on_approval() {
    let (_dir, auth) = open_authorities();
    start_run(&auth, "run_approve");
    register_checkpoint(&auth, "run_approve", "awaiting_approval");

    let report = auth.recover_interrupted_runs().unwrap();
    assert!(report.interrupted_runs.is_empty());
    assert_eq!(report.recovered_waiting, vec!["run_approve".to_string()]);
    assert!(report.swept_side_effects.is_empty());

    let ledger = auth.ledger.lock().unwrap();
    let reader = ledger.reader().unwrap();
    assert_eq!(reader.run_status("run_approve").unwrap(), "running");
}

#[test]
fn restart_restores_run_paused_on_user_interaction() {
    let (_dir, auth) = open_authorities();
    start_run(&auth, "run_interact");
    register_checkpoint(&auth, "run_interact", "awaiting_user");

    let report = auth.recover_interrupted_runs().unwrap();
    assert!(report.interrupted_runs.is_empty());
    assert_eq!(report.recovered_waiting, vec!["run_interact".to_string()]);

    let ledger = auth.ledger.lock().unwrap();
    let reader = ledger.reader().unwrap();
    assert_eq!(reader.run_status("run_interact").unwrap(), "running");
}

#[test]
fn restart_does_not_sweep_side_effects_of_recoverable_waiting_run() {
    let (_dir, auth) = open_authorities();
    start_run(&auth, "run_wait");
    side_effect(&auth, "run_wait", "tc_1", SideEffectState::Planned);
    register_checkpoint(&auth, "run_wait", "awaiting_approval");

    let report = auth.recover_interrupted_runs().unwrap();
    assert!(report.interrupted_runs.is_empty());
    assert!(report.swept_side_effects.is_empty());

    let idem = auth.idempotency.lock().unwrap();
    let entry = idem.get("run_wait", "tc_1").unwrap().unwrap();
    assert_eq!(entry.state, SideEffectState::Planned);
}

#[test]
fn duplicate_restart_recovery_is_idempotent() {
    let (_dir, auth) = open_authorities();
    start_run(&auth, "run_dup");
    side_effect(&auth, "run_dup", "tc_1", SideEffectState::Executing);

    let first = auth.recover_interrupted_runs().unwrap();
    assert_eq!(first.interrupted_runs.len(), 1);
    assert_eq!(first.swept_side_effects.len(), 1);

    let second = auth.recover_interrupted_runs().unwrap();
    assert!(second.interrupted_runs.is_empty());
    assert!(second.recovered_waiting.is_empty());
    assert!(second.swept_side_effects.is_empty());
}

#[test]
fn restart_recovery_partitions_mixed_open_runs() {
    let (_dir, auth) = open_authorities();
    start_run(&auth, "run_exec");
    side_effect(&auth, "run_exec", "tc_1", SideEffectState::Executing);
    start_run(&auth, "run_approve");
    register_checkpoint(&auth, "run_approve", "awaiting_approval");
    start_run(&auth, "run_done");
    let ledger = auth.ledger.lock().unwrap();
    ledger
        .transition(
            "run_done",
            "run.completed",
            "system",
            now_ts(),
            &json!({"status": "completed"}),
            "",
        )
        .unwrap();
    drop(ledger);

    let report = auth.recover_interrupted_runs().unwrap();
    assert_eq!(report.interrupted_runs, vec!["run_exec".to_string()]);
    assert_eq!(report.recovered_waiting, vec!["run_approve".to_string()]);
    assert_eq!(report.swept_side_effects.len(), 1);
}
