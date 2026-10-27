use super::*;
use crate::LedgerEvent;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicUsize;

/// Authorities for a test host, with the egress default chosen as `open`, so tests that are not
/// about egress (ADR-0057) behave as they did before it existed.
fn authorities_at(state_dir: impl AsRef<std::path::Path>) -> RuntimeAuthorities {
    let authorities = RuntimeAuthorities::open(state_dir.as_ref()).unwrap();
    crate::egress::set_default_level(state_dir.as_ref(), crate::egress::EgressLevel::Open, false)
        .unwrap();
    authorities
}

#[test]
fn test_runtime_config_default() {
    let cfg = RuntimeConfig::default();
    assert_eq!(cfg.protocol, "openai_chat");
    assert_eq!(cfg.max_iterations, DEFAULT_MAX_ITERATIONS);
    assert_eq!(cfg.max_retries, DEFAULT_MAX_RETRIES);
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
fn a_restored_transcript_keeps_the_session_system_prompt() {
    // The persisted transcript never carries the system message, so seeding a host from it must
    // not drop the prompt the host was built with.
    let config = RuntimeConfig {
        system_prompt: Some("agent rules".to_string()),
        ..Default::default()
    };
    let host = RuntimeHost::new("s1", config).with_messages(vec![
        json!({"role": "user", "content": "hello"}),
        json!({"role": "assistant", "content": "hi"}),
    ]);
    let outbound = host.outbound_messages();
    assert_eq!(outbound.len(), 3);
    assert_eq!(outbound[0]["role"], "system");
    assert_eq!(outbound[0]["content"], "agent rules");
    assert_eq!(outbound[1]["content"], "hello");

    // A transcript that does carry a system message gets the current prompt, not a second one.
    let config = RuntimeConfig {
        system_prompt: Some("new rules".to_string()),
        ..Default::default()
    };
    let host = RuntimeHost::new("s1", config).with_messages(vec![
        json!({"role": "system", "content": "old rules"}),
        json!({"role": "user", "content": "hello"}),
    ]);
    let systems: Vec<&Value> = host
        .messages()
        .iter()
        .filter(|m| m["role"] == "system")
        .collect();
    assert_eq!(systems.len(), 1);
    assert_eq!(systems[0]["content"], "new rules");
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

    // A gateway that wraps a rejected request in a 502 used to look like a transient upstream
    // fault here, so the runtime resent the identical body four times. The endpoint's own verdict
    // and its failure families now outrank the status code.
    let gateway_client = format!(
        "HTTP 502: {{\"error\":{{\"message\":\"All attempted nodes failed for model \\\"Max\\\".\",\"details\":{{\"failure_kinds\":{{\"client\":3}}}}}}}} {}",
        crate::provider::NO_RETRY_TAG
    );
    assert!(!is_retryable_error(&gateway_client));
    assert!(!is_retryable_error(
        "HTTP 502: {\"error\":{\"details\":{\"failure_kinds\":{\"client\":3}}}}"
    ));
    // …but a genuine transient 502 is untouched.
    assert!(is_retryable_error(
        "HTTP 502: {\"error\":{\"details\":{\"failure_kinds\":{\"server\":2,\"network\":1}}}}"
    ));
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
    let authorities = authorities_at(temp.path());
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

struct SecretOutputExecutor;

impl ToolExecutor for SecretOutputExecutor {
    fn execute(&self, call: &ToolCall, _context: &ToolExecutionContext) -> ToolResult {
        ToolResult::success(
            &call.id,
            json!({"stdout": "Authorization: Bearer abc123", "path": "report.md"}),
        )
    }
}

#[test]
fn history_keeps_what_happened_but_not_the_credentials_in_it() {
    let temp = tempfile::tempdir().unwrap();
    let authorities = authorities_at(temp.path());
    let mut host = RuntimeHost::new("session-s", RuntimeConfig::default())
        .with_authorities(authorities.clone())
        .with_tools(tool_contract("low", false))
        .with_tool_executor(Arc::new(SecretOutputExecutor));
    host.set_run_id("run-s".to_string());
    let result = host.execute_tool_call(&ToolCall {
        id: "call-s".to_string(),
        name: "write_report".to_string(),
        arguments: json!({
            "path": "report.md",
            "content": "api_token = s3cret-body",
            "note": "curl -H 'Authorization: Bearer abc123' https://h/x?token=xyz --password hunter2",
        }),
    });
    assert!(result.error.is_none(), "{:?}", result.error);

    let ledger = authorities.ledger.lock().unwrap();
    let reader = ledger.reader().unwrap();
    let events = reader.events("run-s").unwrap();
    let history =
        serde_json::to_string(&events.iter().map(|e| e.payload.clone()).collect::<Vec<_>>())
            .unwrap();
    // Tool events carry no credential or file body ...
    for event in events
        .iter()
        .filter(|e| e.r#type == "tool.proposed" || e.r#type == "tool.completed")
    {
        let text = event.payload.to_string();
        for (index, secret) in ["s3cret-body", "abc123", "xyz", "hunter2"]
            .iter()
            .enumerate()
        {
            assert!(
                !text.contains(secret),
                "sensitive value #{index} in a {} payload",
                event.r#type
            );
        }
    }
    let proposed = events.iter().find(|e| e.r#type == "tool.proposed").unwrap();
    assert_eq!(proposed.payload["tool"], "write_report");
    assert_eq!(proposed.payload["arguments"]["path"], "report.md");
    assert_eq!(proposed.payload["arguments"]["content"], "[redacted body]");
    let completed = events
        .iter()
        .find(|e| e.r#type == "tool.completed")
        .unwrap();
    // The ledger keeps a summary of the result (size, hash, a short preview), and the credentials
    // in the preview are redacted like everything else in it.
    let summary = &completed.payload["result"];
    assert!(summary["sha256"].is_string() && summary["bytes"].as_u64().unwrap() > 0);
    let preview = summary["preview"].as_str().unwrap();
    assert!(preview.contains("report.md"), "{preview}");
    assert!(preview.contains("Authorization: [redacted]"), "{preview}");
    assert!(
        history.contains("checkpoint.registered")
            || events.iter().any(|e| e.r#type == "checkpoint.registered")
    );
    // ... the chain still verifies, and so does the checkpoint.
    assert!(reader.verify("run-s").unwrap());
    let checkpoint = events
        .iter()
        .find(|e| e.r#type == "checkpoint.registered")
        .unwrap();
    let id = checkpoint.payload["id"].as_str().unwrap();
    let checkpoints =
        crate::checkpoint::CheckpointReader::open(temp.path().join("run_events.db")).unwrap();
    let validation = checkpoints.validate(id).unwrap();
    assert!(validation.valid, "{validation:?}");
    drop(ledger);

    // The audit table agrees.
    let approvals = authorities
        .approvals
        .lock()
        .unwrap()
        .list(10, Some("session-s"), None, Some("write_report"))
        .unwrap();
    let audit = serde_json::to_string(&approvals).unwrap();
    for (index, secret) in ["s3cret-body", "abc123", "xyz", "hunter2"]
        .iter()
        .enumerate()
    {
        assert!(
            !audit.contains(secret),
            "sensitive value #{index} in the audit table"
        );
    }
    assert_eq!(approvals[0]["tool"], "write_report");
}

#[test]
fn post_execution_artifact_failure_becomes_uncertain_and_never_reexecutes() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let staging = workspace.join(".delta/staging/run-postprocess");
    fs::create_dir_all(&staging).unwrap();
    let escaped = workspace.join("escaped.md");
    fs::write(&escaped, "candidate").unwrap();
    let authorities = authorities_at(temp.path().join("state"));
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
    let authorities = authorities_at(temp.path());
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
    let authorities = authorities_at(temp.path());
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
    let authorities = authorities_at(state_dir);
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

fn unattended_host(
    temp: &Path,
    wait: Option<Duration>,
    unattended: bool,
    calls: Arc<AtomicUsize>,
) -> (RuntimeHost, RuntimeAuthorities) {
    let authorities = authorities_at(temp);
    let mut host = RuntimeHost::new(
        "session-wait",
        RuntimeConfig {
            unattended,
            unattended_wait: wait,
            ..RuntimeConfig::default()
        },
    )
    .with_authorities(authorities.clone())
    .with_tools(tool_contract("medium", true))
    .with_tool_executor(Arc::new(CountingExecutor {
        calls,
        staged: None,
    }));
    host.set_run_id("run-wait".to_string());
    (host, authorities)
}

#[test]
fn an_unattended_approval_that_gets_no_answer_expires_and_closes_its_inbox_item() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (host, authorities) = unattended_host(
        temp.path(),
        Some(Duration::from_millis(100)),
        true,
        calls.clone(),
    );
    let result = host.execute_tool_call(&ToolCall {
        id: "call-expiry".to_string(),
        name: "write_report".to_string(),
        arguments: json!({"path": "report.md"}),
    });
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|e| e.contains("expired")),
        "{:?}",
        result.error
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "the tool never ran");
    assert!(host.approvals.pending_ids().is_empty());
    let items = authorities.inbox().list(Some("session-wait"), None);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].state, "resolved");
    assert_eq!(items[0].resolution.as_deref(), Some("expired"));
    let events = authorities
        .ledger()
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .events("run-wait")
        .unwrap();
    assert!(events.iter().any(|e| e.r#type == "approval.expired"));
    assert!(events.iter().any(|e| e.r#type == "tool.cancelled"));
}

#[test]
fn an_unattended_question_that_gets_no_answer_expires() {
    let temp = tempfile::tempdir().unwrap();
    let (host, authorities) = unattended_host(
        temp.path(),
        Some(Duration::from_millis(100)),
        true,
        Arc::new(AtomicUsize::new(0)),
    );
    let result = host
        .execute_interaction(&ToolCall {
            id: "call-question".to_string(),
            name: "ask_user".to_string(),
            arguments: json!({"question": "Which file?"}),
        })
        .expect("ask_user is an interaction");
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|e| e.contains("expired")),
        "{:?}",
        result.error
    );
    let items = authorities.inbox().list(Some("session-wait"), None);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].resolution.as_deref(), Some("expired"));
}

#[test]
fn an_attended_approval_is_never_timed_out() {
    let temp = tempfile::tempdir().unwrap();
    let (host, _authorities) = unattended_host(
        temp.path(),
        Some(Duration::from_millis(50)),
        false,
        Arc::new(AtomicUsize::new(0)),
    );
    let host = Arc::new(host);
    let worker = {
        let host = host.clone();
        std::thread::spawn(move || {
            host.execute_tool_call(&ToolCall {
                id: "call-attended".to_string(),
                name: "write_report".to_string(),
                arguments: json!({"path": "report.md"}),
            })
        })
    };
    // Well past the limit an unattended run would have given up at.
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(host.approvals.pending_ids(), ["call-attended"]);
    host.approvals
        .resolve(Some("call-attended"), ApprovalDecision::Deny)
        .unwrap();
    let result = worker.join().unwrap();
    assert_eq!(result.error.as_deref(), Some("tool call denied by user"));
}

#[test]
fn native_e2e_repeated_denials_in_an_automation_run_pause_it_and_leave_a_notice() {
    let temp = tempfile::tempdir().unwrap();
    let authorities = authorities_at(temp.path());
    let automations = crate::automation::AutomationStore::open(temp.path()).unwrap();
    let created = automations
        .create(&json!({"title": "Weekly report", "instructions": "Write it", "cron": "0 9 * * 1"}))
        .unwrap();
    let task_id = created["task"]["id"].as_str().unwrap().to_string();
    let session_id = automations.prepare_run(&task_id).unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut host = RuntimeHost::new(
        &session_id,
        RuntimeConfig {
            unattended: true,
            ..RuntimeConfig::default()
        },
    )
    .with_authorities(authorities.clone())
    .with_tools(tool_contract("medium", true))
    .with_tool_executor(Arc::new(CountingExecutor {
        calls: calls.clone(),
        staged: None,
    }));
    host.set_run_id("run-breaker-e2e".to_string());
    let host = Arc::new(host);

    for attempt in 1..=3 {
        let id = format!("call-breaker-{attempt}");
        let worker = {
            let host = host.clone();
            let id = id.clone();
            std::thread::spawn(move || {
                host.execute_tool_call(&ToolCall {
                    id,
                    name: "write_report".to_string(),
                    arguments: json!({"path": "report.md"}),
                })
            })
        };
        for _ in 0..200 {
            if host.approvals.pending_ids() == [id.as_str()] {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        host.approvals
            .resolve(Some(&id), ApprovalDecision::Deny)
            .unwrap();
        let result = worker.join().unwrap();
        assert_eq!(result.error.as_deref(), Some("tool call denied by user"));
        let enabled = automations.get(&task_id).unwrap()["task"]["enabled"].clone();
        assert_eq!(enabled, attempt < 3, "after denial {attempt}");
    }

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let task = automations.get(&task_id).unwrap()["task"].clone();
    assert_eq!(task["paused_reason"], "approval_denied");
    let notices: Vec<_> = authorities
        .inbox
        .list(None, Some("pending"))
        .into_iter()
        .filter(|item| item.kind == "notice")
        .collect();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].title, "Paused: Weekly report");
    assert!(notices[0].body.contains("write_report"));
    let events = authorities
        .ledger()
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .events("run-breaker-e2e")
        .unwrap();
    assert!(events
        .iter()
        .any(|event| event.r#type == "automation.paused"));
}

#[test]
fn native_e2e_edit_document_asks_first_then_replaces_the_file_and_keeps_the_original() {
    let state = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let authorities = authorities_at(state.path());
    let (_, original, _) = crate::document::create(
        "report.docx",
        &json!({"title": "Report", "blocks": [{"type": "paragraph", "text": "Revenue grew in Q3."}]}),
    )
    .unwrap();
    std::fs::write(workspace.path().join("report.docx"), &original).unwrap();
    let capabilities = crate::capability::CapabilityHost::product_defaults().unwrap();
    let tools = capabilities.tool_schemas();
    let mut host = RuntimeHost::new(
        "session-edit-doc",
        RuntimeConfig {
            workspace: Some(workspace.path().to_string_lossy().to_string()),
            ..RuntimeConfig::default()
        },
    )
    .with_authorities(authorities)
    .with_tools(tools)
    .with_tool_executor(Arc::new(capabilities));
    host.set_run_id("run-edit-doc".to_string());
    let host = Arc::new(host);
    let worker = {
        let host = host.clone();
        std::thread::spawn(move || {
            host.execute_tool_call(&ToolCall {
                id: "call-edit-doc".to_string(),
                name: "edit_document".to_string(),
                arguments: json!({"path": "report.docx", "edits": [{"find": "Q3", "replace": "Q4"}]}),
            })
        })
    };
    for _ in 0..400 {
        if host.approvals.pending_ids() == ["call-edit-doc"] {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        host.approvals.pending_ids(),
        ["call-edit-doc"],
        "an edit must ask first"
    );
    assert_eq!(
        std::fs::read(workspace.path().join("report.docx")).unwrap(),
        original,
        "nothing changes before the approval"
    );
    host.approvals
        .resolve(Some("call-edit-doc"), ApprovalDecision::Once)
        .unwrap();
    let result = worker.join().unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);

    let edited = std::fs::read(workspace.path().join("report.docx")).unwrap();
    let text = &crate::document::parse(crate::document::Kind::Docx, &edited)
        .unwrap()
        .blocks[0]
        .text;
    assert!(text.contains("Q4") && !text.contains("Q3"), "{text}");
    let kept = workspace
        .path()
        .join(".delta/versions/run-edit-doc/report.docx");
    assert_eq!(
        std::fs::read(kept).unwrap(),
        original,
        "the original is kept as a version"
    );
}

/// A host for a session, with one finished file staged for `write_report` to deliver.
fn completion_host(
    state: &Path,
    workspace: &Path,
    session_id: &str,
    run_id: &str,
) -> (RuntimeHost, RuntimeAuthorities) {
    let staging = workspace.join(".delta/staging").join(run_id);
    fs::create_dir_all(&staging).unwrap();
    let staged_path = staging.join("candidate.md");
    fs::write(&staged_path, "the report").unwrap();
    let authorities = authorities_at(state);
    let mut host = RuntimeHost::new(
        session_id,
        RuntimeConfig {
            workspace: Some(workspace.to_string_lossy().to_string()),
            unattended: true,
            ..RuntimeConfig::default()
        },
    )
    .with_authorities(authorities.clone())
    .with_tools(tool_contract("low", false))
    .with_tool_executor(Arc::new(CountingExecutor {
        calls: Arc::new(AtomicUsize::new(0)),
        staged: Some(StagedArtifact {
            staging_path: staged_path,
            relative_path: "reports/final.md".to_string(),
            kind: "markdown".to_string(),
            incomplete: false,
        }),
    }));
    host.set_run_id(run_id.to_string());
    (host, authorities)
}

fn deliver_report(host: &RuntimeHost) {
    let result = host.execute_tool_call(&ToolCall {
        id: "call-report".to_string(),
        name: "write_report".to_string(),
        arguments: json!({"path": "reports/final.md"}),
    });
    assert!(result.error.is_none(), "{:?}", result.error);
}

fn notices(authorities: &RuntimeAuthorities) -> Vec<crate::inbox::InboxItem> {
    authorities
        .inbox
        .list(None, Some("pending"))
        .into_iter()
        .filter(|item| item.kind == "notice")
        .collect()
}

#[test]
fn native_e2e_a_finished_automation_run_leaves_a_notice_that_names_its_files() {
    let state = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let automations = crate::automation::AutomationStore::open(state.path()).unwrap();
    let created = automations
        .create(&json!({"title": "Weekly report", "instructions": "Write it", "cron": "0 9 * * 1"}))
        .unwrap();
    let task_id = created["task"]["id"].as_str().unwrap().to_string();
    let session_id = automations.prepare_run(&task_id).unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (host, authorities) =
        completion_host(state.path(), workspace.path(), &session_id, "run-done");
    deliver_report(&host);

    host.announce_completion("All done.\n\nThe   report is ready.");

    let found = notices(&authorities);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].title, "Finished: Weekly report");
    assert_eq!(found[0].body, "All done. The report is ready.");
    assert_eq!(found[0].data["completion"], true);
    assert_eq!(
        found[0].data["artifacts"],
        json!([{"path": "reports/final.md", "name": "final.md"}])
    );
    // Announcing the same run again does not pile up a second notice.
    host.announce_completion("All done.");
    assert_eq!(notices(&authorities).len(), 1);
}

#[test]
fn native_e2e_an_automation_that_wants_no_notice_leaves_none() {
    let state = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let automations = crate::automation::AutomationStore::open(state.path()).unwrap();
    let created = automations
        .create(&json!({
            "title": "Quiet", "instructions": "Write it", "cron": "0 9 * * 1",
            "notify_on_completion": false
        }))
        .unwrap();
    let task_id = created["task"]["id"].as_str().unwrap().to_string();
    let session_id = automations.prepare_run(&task_id).unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (host, authorities) =
        completion_host(state.path(), workspace.path(), &session_id, "run-quiet");
    host.announce_completion("Done.");
    assert!(notices(&authorities).is_empty());
}

#[test]
fn native_e2e_a_chat_turn_leaves_no_inbox_notice() {
    let state = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let (host, authorities) =
        completion_host(state.path(), workspace.path(), "session-chat", "run-chat");
    deliver_report(&host);
    host.announce_completion("Done.");
    assert!(notices(&authorities).is_empty());
}

#[test]
fn native_e2e_a_finished_turn_posts_to_the_webhook_the_egress_level_allows() {
    let state = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let (url, received) = crate::webhook::tests::serve_once("HTTP/1.1 200 OK");
    crate::egress::set_default_level(state.path(), crate::egress::EgressLevel::PrivateOnly, true)
        .unwrap();
    crate::webhook::set_settings(state.path(), Some(&url), true, false).unwrap();
    let (host, _authorities) =
        completion_host(state.path(), workspace.path(), "session-hook", "run-hook");
    deliver_report(&host);

    host.announce_completion("Done.");

    let raw = received
        .recv_timeout(Duration::from_secs(5))
        .expect("the webhook was posted to");
    let body: Value = serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["event"], "turn_completed");
    assert_eq!(body["session_id"], "session-hook");
    assert_eq!(body["source"], "chat");
    assert_eq!(body["artifact_count"], 1);
    assert!(body.get("summary").is_none(), "no details unless asked for");
}

#[test]
fn native_e2e_a_stricter_egress_level_keeps_the_webhook_from_posting() {
    let state = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let (url, received) = crate::webhook::tests::serve_once("HTTP/1.1 200 OK");
    crate::egress::set_default_level(state.path(), crate::egress::EgressLevel::AskEachTime, true)
        .unwrap();
    crate::webhook::set_settings(state.path(), Some(&url), true, false).unwrap();
    let (host, _authorities) =
        completion_host(state.path(), workspace.path(), "session-hook2", "run-hook2");
    host.announce_completion("Done.");
    assert!(received.recv_timeout(Duration::from_millis(500)).is_err());
}

#[test]
fn native_e2e_a_standing_grant_does_not_override_a_hard_refusal() {
    // The user allowed the tool earlier; the session has since moved to Plan mode, which refuses
    // every mutation outright. A grant answers a question; it can not overrule a refusal.
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let authorities = authorities_at(temp.path());
    let mut host = RuntimeHost::new(
        "session-plan-grant",
        RuntimeConfig {
            mode: crate::policy::ExecutionMode::Plan,
            ..RuntimeConfig::default()
        },
    )
    .with_authorities(authorities)
    .with_tools(tool_contract("medium", true))
    .with_tool_executor(Arc::new(CountingExecutor {
        calls: calls.clone(),
        staged: None,
    }));
    host.set_run_id("run-plan-grant".to_string());
    host.approvals.remember(
        ApprovalDecision::AlwaysTool,
        "write_report",
        &json!({"path": "report.md"}),
    );

    let result = host.execute_tool_call(&ToolCall {
        id: "call-plan-grant".to_string(),
        name: "write_report".to_string(),
        arguments: json!({"path": "report.md"}),
    });

    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("plan mode")),
        "{:?}",
        result.error
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "the tool must not run");
}

/// A host in `workspace` for the approval-ladder tests: `write_report` needs approval.
fn ladder_host(
    state: &Path,
    workspace: &Path,
    session_id: &str,
    contract: Value,
) -> (Arc<RuntimeHost>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut host = RuntimeHost::new(
        session_id,
        RuntimeConfig {
            workspace: Some(workspace.to_string_lossy().to_string()),
            ..RuntimeConfig::default()
        },
    )
    .with_authorities(authorities_at(state))
    .with_tools(contract)
    .with_tool_executor(Arc::new(CountingExecutor {
        calls: calls.clone(),
        staged: None,
    }));
    host.set_run_id(format!("run-{session_id}"));
    (Arc::new(host), calls)
}

/// Call `write_report`. `Some(decision)` answers the question it asks; `None` expects no question.
/// Returns whether a question was asked.
fn ladder_call(host: &Arc<RuntimeHost>, id: &str, answer: Option<ApprovalDecision>) -> bool {
    let worker = {
        let host = host.clone();
        let id = id.to_string();
        std::thread::spawn(move || {
            host.execute_tool_call(&ToolCall {
                id,
                name: "write_report".to_string(),
                arguments: json!({"path": "report.md"}),
            })
        })
    };
    let mut asked = false;
    for _ in 0..100 {
        if host.approvals.pending_ids() == [id] {
            asked = true;
            break;
        }
        if worker.is_finished() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if asked {
        host.approvals
            .resolve(Some(id), answer.unwrap_or(ApprovalDecision::Deny))
            .unwrap();
    }
    let result = worker.join().unwrap();
    if answer.is_some() || !asked {
        assert!(result.error.is_none(), "{id}: {:?}", result.error);
    }
    asked
}

#[test]
fn native_e2e_session_grant_then_revoke_restores_the_question() {
    let state = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let (host, calls) = ladder_host(
        state.path(),
        workspace.path(),
        "s-ladder",
        tool_contract("medium", true),
    );

    assert!(
        ladder_call(&host, "c1", Some(ApprovalDecision::AlwaysTool)),
        "the first call asks"
    );
    assert!(
        !ladder_call(&host, "c2", None),
        "the session grant answers the second"
    );
    assert_eq!(
        host.approvals.grants(),
        vec![crate::approval::SessionGrant {
            tool: "write_report".into(),
            command: None
        }]
    );

    assert!(host.approvals.revoke_grant("write_report", None));
    assert!(host.approvals.grants().is_empty());
    assert!(
        ladder_call(&host, "c3", Some(ApprovalDecision::Once)),
        "after revoking, it asks again"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[test]
fn native_e2e_workspace_rule_outlives_the_session_until_it_is_revoked() {
    let state = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let ws = workspace.path().to_string_lossy().to_string();
    let (first, _) = ladder_host(
        state.path(),
        workspace.path(),
        "s-one",
        tool_contract("medium", true),
    );

    assert!(ladder_call(
        &first,
        "c1",
        Some(ApprovalDecision::AlwaysWorkspace)
    ));
    let rules = crate::workspace_rules::list(state.path(), &ws).unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].tool, "write_report");
    assert!(
        !ladder_call(&first, "c2", None),
        "the rule answers a later call"
    );

    // A different session in the same workspace is covered too.
    let (second, calls) = ladder_host(
        state.path(),
        workspace.path(),
        "s-two",
        tool_contract("medium", true),
    );
    assert!(!ladder_call(&second, "c3", None));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Another workspace is not.
    let elsewhere = tempfile::tempdir().unwrap();
    let (other, _) = ladder_host(
        state.path(),
        elsewhere.path(),
        "s-three",
        tool_contract("medium", true),
    );
    assert!(ladder_call(&other, "c4", Some(ApprovalDecision::Once)));

    assert!(crate::workspace_rules::revoke(state.path(), &ws, "write_report", None).unwrap());
    assert!(
        ladder_call(&second, "c5", Some(ApprovalDecision::Once)),
        "after revoking, it asks again"
    );
}

#[test]
fn native_e2e_a_call_that_deletes_is_never_made_a_workspace_rule() {
    let state = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let ws = workspace.path().to_string_lossy().to_string();
    let mut contract = tool_contract("medium", true);
    contract[0]["function"]["metadata"]["side_effect"] = json!("destructive");
    let (host, _) = ladder_host(state.path(), workspace.path(), "s-destroy", contract);

    assert!(
        ladder_call(&host, "c1", Some(ApprovalDecision::AlwaysWorkspace)),
        "the call itself is allowed once"
    );
    assert!(crate::workspace_rules::list(state.path(), &ws)
        .unwrap()
        .is_empty());
    assert!(
        ladder_call(&host, "c2", Some(ApprovalDecision::Once)),
        "and the next one asks"
    );
}

#[test]
fn native_e2e_a_workspace_rule_does_not_overrule_plan_mode() {
    let state = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    crate::workspace_rules::add(
        state.path(),
        &workspace.path().to_string_lossy(),
        "write_report",
        &json!({}),
    )
    .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut host = RuntimeHost::new(
        "s-plan-rule",
        RuntimeConfig {
            workspace: Some(workspace.path().to_string_lossy().to_string()),
            mode: crate::policy::ExecutionMode::Plan,
            ..RuntimeConfig::default()
        },
    )
    .with_authorities(authorities_at(state.path()))
    .with_tools(tool_contract("medium", true))
    .with_tool_executor(Arc::new(CountingExecutor {
        calls: calls.clone(),
        staged: None,
    }));
    host.set_run_id("run-plan-rule".to_string());
    let result = host.execute_tool_call(&ToolCall {
        id: "c1".to_string(),
        name: "write_report".to_string(),
        arguments: json!({"path": "report.md"}),
    });
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("plan mode")),
        "{:?}",
        result.error
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
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
    // The request is answerable as soon as it is registered; its Inbox item is written right after.
    for _ in 0..200 {
        if !authorities
            .inbox()
            .list(Some("session-approval-e2e"), Some("pending"))
            .is_empty()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

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
            verified_inputs: Vec::new(),
            state: ToolExitState::Cancelled,
        }
    }
}

#[test]
fn native_e2e_cancel_during_tool_marks_side_effect_uncertain() {
    let temp = tempfile::tempdir().unwrap();
    let authorities = authorities_at(temp.path());
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
    let authorities = authorities_at(temp.path().join("state"));
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

#[test]
fn replacing_a_file_keeps_the_previous_version_and_says_where() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    fs::create_dir_all(workspace.join("reports")).unwrap();
    fs::write(workspace.join("reports/final.md"), "the original").unwrap();
    let authorities = authorities_at(temp.path());
    let run = "run-replace";
    let staging = workspace
        .join(".delta")
        .join("staging")
        .join(run)
        .join("job");
    fs::create_dir_all(&staging).unwrap();
    let mut results = Vec::new();
    for (round, text) in ["second version", "third version"].into_iter().enumerate() {
        let staged_path = staging.join(format!("candidate-{round}"));
        fs::write(&staged_path, text).unwrap();
        let mut host = RuntimeHost::new(
            "session-replace",
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
                staging_path: staged_path,
                relative_path: "reports/final.md".to_string(),
                kind: "markdown".to_string(),
                incomplete: false,
            }),
        }));
        host.set_run_id(run.to_string());
        let result = host.execute_tool_call(&ToolCall {
            id: format!("call-{round}"),
            name: "write_report".to_string(),
            arguments: json!({"path": "reports/final.md"}),
        });
        assert!(result.error.is_none(), "{:?}", result.error);
        results.push(result);
    }
    assert_eq!(
        fs::read_to_string(workspace.join("reports/final.md")).unwrap(),
        "third version"
    );
    let kept = workspace
        .join(".delta/versions")
        .join(run)
        .join("reports/final.md");
    // The first replacement in a run keeps the original; later ones do not overwrite it.
    assert_eq!(fs::read_to_string(&kept).unwrap(), "the original");
    for result in &results {
        assert_eq!(
            result.output["artifacts"][0]["previous_version"],
            format!(".delta/versions/{run}/reports/final.md")
        );
    }
}

fn host_with_messages(
    protocol: &str,
    workspace: &std::path::Path,
    messages: Vec<Value>,
) -> RuntimeHost {
    RuntimeHost::new(
        "session-images",
        RuntimeConfig {
            protocol: protocol.to_string(),
            workspace: Some(workspace.to_string_lossy().to_string()),
            ..RuntimeConfig::default()
        },
    )
    .with_messages(messages)
}

fn screenshot_conversation(count: usize) -> Vec<Value> {
    let mut messages = vec![json!({"role": "user", "content": "what is on my screen?"})];
    for n in 1..=count {
        messages.push(json!({"role": "assistant", "content": null, "tool_calls": [
            {"id": format!("a{n}"), "type": "function", "function": {"name": "screenshot", "arguments": "{}"}},
            {"id": format!("b{n}"), "type": "function", "function": {"name": "clipboard_read", "arguments": "{}"}}]}));
        messages.push(json!({
            "role": "tool", "tool_call_id": format!("a{n}"),
            "content": {"ok": true, "images": [{"path": format!(".delta/screens/run/{n}.png")}]},
            "images": [{"path": format!(".delta/screens/run/{n}.png")}],
        }));
        messages.push(
            json!({"role": "tool", "tool_call_id": format!("b{n}"), "content": {"ok": true}}),
        );
    }
    messages
}

#[test]
fn a_tool_screenshot_reaches_the_model_as_one_image_message_after_all_tool_results() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".delta/screens/run")).unwrap();
    fs::write(
        temp.path().join(".delta/screens/run/1.png"),
        b"\x89PNG-bytes",
    )
    .unwrap();
    let host = host_with_messages("openai_chat", temp.path(), screenshot_conversation(1));
    let out = host.outbound_messages();
    let roles: Vec<&str> = out.iter().map(|m| m["role"].as_str().unwrap()).collect();
    // Both tool results stay together; the picture follows them as a user message.
    assert_eq!(roles, ["user", "assistant", "tool", "tool", "user"]);
    assert!(
        out[2].get("images").is_none(),
        "the file list is not sent as part of the tool message"
    );
    let parts = out[4]["content"].as_array().unwrap();
    assert_eq!(parts[1]["type"], "image_url");
    assert!(parts[1]["image_url"]["url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/png;base64,"));

    let anthropic = host_with_messages("anthropic", temp.path(), screenshot_conversation(1))
        .outbound_messages();
    assert_eq!(anthropic[4]["content"][1]["type"], "image");
    assert_eq!(anthropic[4]["content"][1]["source"]["type"], "base64");
    let responses = host_with_messages("openai_responses", temp.path(), screenshot_conversation(1))
        .outbound_messages();
    assert_eq!(responses[4]["content"][1]["type"], "input_image");
}

#[test]
fn only_the_newest_screenshots_are_sent_and_unsafe_or_missing_files_are_ignored() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".delta/screens/run")).unwrap();
    for n in 1..=3 {
        fs::write(
            temp.path().join(format!(".delta/screens/run/{n}.png")),
            b"png",
        )
        .unwrap();
    }
    let host = host_with_messages("openai_chat", temp.path(), screenshot_conversation(3));
    let out = host.outbound_messages();
    let image_messages: Vec<&Value> = out
        .iter()
        .filter(|m| m["role"] == "user" && m["content"].is_array())
        .collect();
    // Screenshots 2 and 3 are sent, number 1 is left on disk.
    assert_eq!(image_messages.len(), 2);

    // A path outside `.delta/screens`, a traversal and a missing file produce no image.
    fs::write(temp.path().join("secret.png"), b"s").unwrap();
    for path in [
        "secret.png",
        ".delta/screens/../../secret.png",
        ".delta/screens/run/missing.png",
        "/etc/passwd",
    ] {
        let messages = vec![
            json!({"role": "user", "content": "x"}),
            tool_call_message("t"),
            json!({"role": "tool", "tool_call_id": "t", "content": {}, "images": [{"path": path}]}),
        ];
        let out = host_with_messages("openai_chat", temp.path(), messages).outbound_messages();
        assert_eq!(out.len(), 3, "{path}");
    }
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
            retry_base_delay_ms: 1,
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
    let authorities = authorities_at(temp.path());
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
fn native_e2e_tool_call_past_the_run_budget_is_refused_and_the_run_stops() {
    // Two scripted replies only: a third model request would have no reply to read, so the
    // request count below also proves the run stopped instead of asking again.
    let provider = RetryMockProvider::start(vec![RetryReply::ToolCall, RetryReply::ToolCall]);
    let temp = tempfile::tempdir().unwrap();
    let authorities = authorities_at(temp.path());
    let calls = Arc::new(AtomicUsize::new(0));
    let sink = Arc::new(CaptureSink::default());
    let host = RuntimeHost::new(
        "session-tool-budget",
        RuntimeConfig {
            model: "test-model".to_string(),
            protocol: "openai_chat".to_string(),
            api_key: "test-key".to_string(),
            base_url: provider.base_url.clone(),
            max_iterations: 5,
            max_tool_calls: 1,
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

    assert_eq!(handle.state(), RuntimeState::Failed);
    assert_eq!(provider.finish(), 2);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "only the call within budget runs"
    );
    let frames = sink.frames.lock().unwrap();
    let turn_ends: Vec<&Value> = frames
        .iter()
        .filter(|frame| frame["type"] == "turn_end")
        .collect();
    assert_eq!(turn_ends.len(), 1, "{frames:?}");
    assert_eq!(turn_ends[0]["payload"]["status"], "tool_budget_exceeded");
    let events = authorities
        .ledger()
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .events(&run_id)
        .unwrap();
    for expected in ["tool.failed", "run.failed"] {
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
fn native_e2e_retry_is_reported_as_progress_and_leaves_no_error_in_the_transcript() {
    let provider = RetryMockProvider::start(vec![
        RetryReply::Status(503),
        RetryReply::Complete("recovered"),
    ]);
    let sink = Arc::new(CaptureSink::default());
    let handle = RuntimeHandle::spawn(retry_test_host(
        provider.base_url.clone(),
        sink.clone(),
        None,
    ))
    .unwrap();
    handle.run("flaky gateway".to_string(), None).unwrap();
    wait_for_terminal(&handle);
    assert_eq!(handle.state(), RuntimeState::Completed);
    provider.finish();

    let frames = sink.frames.lock().unwrap();
    let retry = frames
        .iter()
        .find(|frame| frame["type"] == "error")
        .expect("the retry is announced");
    assert_eq!(retry["payload"]["terminal"], false);
    assert_eq!(retry["payload"]["attempt"], 2);
    assert_eq!(retry["payload"]["max_attempts"], 2);
    assert!(retry["payload"]["retry_in_ms"].is_u64());
    // The transcript only holds what happened, not the plumbing: no retry marker, no error.
    assert!(!handle
        .messages()
        .iter()
        .any(|message| message["role"] == "notice"));
}

#[test]
fn native_e2e_cancel_ends_a_pending_provider_call_without_waiting_for_it() {
    // The gateway accepts the request and then says nothing for 1.5 s.
    let provider = RetryMockProvider::start(vec![RetryReply::Stall(Duration::from_millis(1500))]);
    let sink = Arc::new(CaptureSink::default());
    let handle = RuntimeHandle::spawn(retry_test_host(
        provider.base_url.clone(),
        sink.clone(),
        None,
    ))
    .unwrap();
    handle.run("hang".to_string(), None).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let started = std::time::Instant::now();
    assert!(handle.cancel());
    wait_for_terminal(&handle);
    assert!(
        started.elapsed() < Duration::from_millis(1000),
        "Stop took {:?}",
        started.elapsed()
    );
    assert_eq!(handle.state(), RuntimeState::Interrupted);
    assert!(sink
        .frames
        .lock()
        .unwrap()
        .iter()
        .any(|frame| frame["type"] == "interrupted"));
    provider.finish();
}

#[test]
fn native_e2e_cancel_ends_the_wait_between_retries() {
    let provider = RetryMockProvider::start(vec![RetryReply::Status(503)]);
    let sink = Arc::new(CaptureSink::default());
    let host = RuntimeHost::new(
        "session-backoff",
        RuntimeConfig {
            model: "test-model".to_string(),
            protocol: "openai_chat".to_string(),
            api_key: "test-key".to_string(),
            base_url: provider.base_url.clone(),
            max_retries: 3,
            retry_base_delay_ms: 5000,
            ..RuntimeConfig::default()
        },
    )
    .with_event_sink(sink.clone());
    let handle = RuntimeHandle::spawn(host).unwrap();
    handle.run("overloaded".to_string(), None).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let started = std::time::Instant::now();
    assert!(handle.cancel());
    wait_for_terminal(&handle);
    assert!(started.elapsed() < Duration::from_millis(1000));
    assert_eq!(handle.state(), RuntimeState::Interrupted);
    assert_eq!(provider.finish(), 1);
}

#[test]
fn retry_delay_doubles_and_is_capped() {
    assert_eq!(model_turn::retry_delay_ms(1000, 1), 1000);
    assert_eq!(model_turn::retry_delay_ms(1000, 2), 2000);
    assert_eq!(model_turn::retry_delay_ms(1000, 4), 8000);
    assert_eq!(model_turn::retry_delay_ms(1000, 9), 8000);
    assert_eq!(model_turn::retry_delay_ms(1, 3), 4);
}

#[test]
fn http_status_decides_retryability_not_numbers_in_the_body() {
    for status in [408, 429, 500, 502, 503, 504, 520, 524, 529] {
        assert!(is_retryable_error(&format!("HTTP {status}: x")), "{status}");
    }
    assert!(!is_retryable_error("HTTP 400: at most 500 tokens"));
    assert!(!is_retryable_error("HTTP 401: invalid key"));
    assert!(is_retryable_error(
        "transport error: Network Error: timed out reading response"
    ));
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
    let auth = authorities_at(dir.path());
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

struct ReadingExecutor {
    path: PathBuf,
    fails: bool,
}

impl ToolExecutor for ReadingExecutor {
    fn execute(&self, call: &ToolCall, _context: &ToolExecutionContext) -> ToolResult {
        if self.fails {
            return ToolResult::failure(&call.id, "read failed");
        }
        let bytes = std::fs::read(&self.path).unwrap();
        let mut result = ToolResult::success(&call.id, json!({"ok": true, "text": "hello"}));
        result.verified_inputs.push(VerifiedInput {
            path: self.path.canonicalize().unwrap(),
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            size: bytes.len() as u64,
        });
        result
    }
}

fn reading_contract(category: &str) -> Value {
    json!([{
        "type": "function",
        "function": {
            "name": "read_notes",
            "parameters": {
                "type": "object",
                "required": ["path"],
                "properties": {"path": {"type": "string"}}
            },
            "metadata": {
                "risk_level": "low",
                "requires_approval": false,
                "category": category,
                "capabilities": []
            }
        }
    }])
}

fn run_reading_tool(
    category: &str,
    fails: bool,
    calls: usize,
) -> (RuntimeAuthorities, tempfile::TempDir) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("work");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("notes.txt"), "hello").unwrap();
    let authorities = authorities_at(temp.path().join("state"));
    let mut host = RuntimeHost::new(
        "session-1",
        RuntimeConfig {
            workspace: Some(workspace.to_string_lossy().to_string()),
            ..RuntimeConfig::default()
        },
    )
    .with_authorities(authorities.clone())
    .with_tools(reading_contract(category))
    .with_tool_executor(Arc::new(ReadingExecutor {
        path: workspace.join("notes.txt"),
        fails,
    }));
    host.set_run_id("run-1".to_string());
    for _ in 0..calls {
        host.execute_tool_call(&ToolCall {
            id: "call-1".to_string(),
            name: "read_notes".to_string(),
            arguments: json!({"path": "notes.txt"}),
        });
    }
    (authorities, temp)
}

fn read_events(authorities: &RuntimeAuthorities) -> Vec<LedgerEvent> {
    authorities
        .ledger
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .events("run-1")
        .unwrap()
}

#[test]
fn a_read_tool_records_the_source_it_read_before_it_completes() {
    let (authorities, _temp) = run_reading_tool("read", false, 1);
    let events = read_events(&authorities);
    let types: Vec<&str> = events.iter().map(|event| event.r#type.as_str()).collect();
    let read = types
        .iter()
        .position(|kind| *kind == "source.read")
        .unwrap();
    let completed = types
        .iter()
        .position(|kind| *kind == "tool.completed")
        .unwrap();
    assert!(read < completed, "{types:?}");

    let payload = &events[read].payload;
    assert_eq!(payload["location"], "notes.txt");
    assert_eq!(payload["origin"], "file");
    assert_eq!(payload["tool"], "read_notes");
    assert_eq!(payload["tool_call_id"], "call-1");
    assert_eq!(
        payload["fingerprint"],
        format!("{:x}", Sha256::digest(b"hello"))
    );

    let reader = crate::SourceCitationReader::from_reader(
        authorities.ledger.lock().unwrap().reader().unwrap(),
    );
    let sources = reader.sources_read("run-1").unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].location, "notes.txt");
    assert!(!sources[0].cited);
    assert!(authorities
        .ledger
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .verify("run-1")
        .unwrap());
    // The register itself keeps the source, so its status can still be checked later.
    assert_eq!(reader.list_sources().unwrap().len(), 1);
}

#[test]
fn a_tool_that_is_not_a_read_tool_records_no_source() {
    let (authorities, _temp) = run_reading_tool("filesystem", false, 1);
    assert!(read_events(&authorities)
        .iter()
        .all(|event| event.r#type != "source.read"));
}

#[test]
fn a_failed_read_records_no_source() {
    let (authorities, _temp) = run_reading_tool("read", true, 1);
    assert!(read_events(&authorities)
        .iter()
        .all(|event| event.r#type != "source.read"));
}

#[test]
fn a_replayed_read_does_not_record_the_source_twice() {
    let (authorities, _temp) = run_reading_tool("read", false, 2);
    let count = read_events(&authorities)
        .iter()
        .filter(|event| event.r#type == "source.read")
        .count();
    assert_eq!(count, 1);
}

#[test]
fn recording_a_read_does_not_change_the_run_lifecycle() {
    let (authorities, _temp) = run_reading_tool("read", false, 1);
    let reader = authorities.ledger.lock().unwrap().reader().unwrap();
    // The run id was never started or finished here; the read must not make it look open.
    assert!(!reader.open_runs().unwrap().contains(&"$source".to_string()));
    assert_ne!(reader.run_status("run-1").unwrap(), "completed");
}

// -- Resume keeps the run identity (ADR-0053) ---------------------------------------------------

/// A run of `session-resume` as the ledger would hold it: started, checkpointed, then optionally
/// closed by `last_event` (for example `run.interrupted`).
fn seed_session_run(auth: &RuntimeAuthorities, run_id: &str, last_event: Option<&str>) {
    {
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
        CheckpointWriter::new(&ledger)
            .register(
                CheckpointRegisterInput {
                    checkpoint_id: None,
                    run_id: run_id.to_string(),
                    session_id: "session-resume".to_string(),
                    phase: "tool_completed".to_string(),
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
        if let Some(event) = last_event {
            ledger
                .transition(
                    run_id,
                    event,
                    "system",
                    now_ts(),
                    &json!({"kind": "run"}),
                    "",
                )
                .unwrap();
        }
    }
    // Checkpoint timestamps have one-second resolution; keep the seeded runs in order.
    std::thread::sleep(Duration::from_millis(1100));
}

fn resume_host(base_url: String, authorities: &RuntimeAuthorities) -> RuntimeHost {
    RuntimeHost::new(
        "session-resume",
        RuntimeConfig {
            model: "test-model".to_string(),
            protocol: "openai_chat".to_string(),
            api_key: "test-key".to_string(),
            base_url,
            max_iterations: 2,
            ..RuntimeConfig::default()
        },
    )
    .with_authorities(authorities.clone())
    .with_messages(vec![json!({"role": "user", "content": "hello"})])
}

fn run_events(auth: &RuntimeAuthorities, run_id: &str) -> Vec<LedgerEvent> {
    auth.ledger
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .events(run_id)
        .unwrap()
}

#[test]
fn resume_continues_the_interrupted_run_under_its_own_run_id() {
    let temp = tempfile::tempdir().unwrap();
    let auth = authorities_at(temp.path());
    seed_session_run(&auth, "run-old", Some("run.interrupted"));
    let provider = MockProvider::start(1);
    let handle = RuntimeHandle::spawn(resume_host(provider.base_url.clone(), &auth)).unwrap();

    let run_id = handle.resume().unwrap();
    assert_eq!(run_id, "run-old");
    provider
        .first_response_started
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    provider.release_first_response.send(()).unwrap();
    wait_for_terminal(&handle);
    assert_eq!(handle.state(), RuntimeState::Completed);
    provider.finish();

    let events = run_events(&auth, "run-old");
    let types: Vec<&str> = events.iter().map(|event| event.r#type.as_str()).collect();
    assert_eq!(
        types.iter().filter(|kind| **kind == "run.started").count(),
        1,
        "{types:?}"
    );
    assert_eq!(
        types.iter().filter(|kind| **kind == "run.resumed").count(),
        1,
        "{types:?}"
    );
    let resumed = types
        .iter()
        .position(|kind| *kind == "run.resumed")
        .unwrap();
    let interrupted = types
        .iter()
        .position(|kind| *kind == "run.interrupted")
        .unwrap();
    assert!(interrupted < resumed, "{types:?}");
    assert_eq!(types.last(), Some(&"run.completed"));
    assert_eq!(events[resumed].payload["previous_status"], "interrupted");
    let reader = auth.ledger.lock().unwrap().reader().unwrap();
    assert!(reader.verify("run-old").unwrap());
    assert_eq!(reader.run_status("run-old").unwrap(), "ok"); // the ledger's name for a completed run
}

fn assert_resume_is_rejected(auth: &RuntimeAuthorities, expected: &str) {
    let handle = RuntimeHandle::spawn(resume_host("http://127.0.0.1:9".to_string(), auth)).unwrap();
    let before = auth
        .ledger
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .all_events()
        .unwrap()
        .len();
    let error = handle.resume().unwrap_err();
    assert!(error.contains(expected), "{error}");
    assert_eq!(handle.state(), RuntimeState::Idle);
    let after = auth
        .ledger
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .all_events()
        .unwrap()
        .len();
    assert_eq!(before, after, "a rejected resume must write nothing");
}

#[test]
fn resume_with_no_recorded_run_is_rejected_and_writes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let auth = authorities_at(temp.path());
    assert_resume_is_rejected(&auth, "no recorded run");
}

#[test]
fn a_finished_run_cannot_be_resumed() {
    for closing in ["run.completed", "run.failed", "run.cancelled"] {
        let temp = tempfile::tempdir().unwrap();
        let auth = authorities_at(temp.path());
        seed_session_run(&auth, "run-done", Some(closing));
        assert_resume_is_rejected(&auth, "not interrupted");
    }
}

#[test]
fn a_run_still_open_on_a_human_decision_is_not_resumed() {
    let temp = tempfile::tempdir().unwrap();
    let auth = authorities_at(temp.path());
    seed_session_run(&auth, "run-waiting", None);
    assert_resume_is_rejected(&auth, "not interrupted");
}

#[test]
fn an_older_interrupted_run_is_not_resumed_after_a_newer_run_finished() {
    let temp = tempfile::tempdir().unwrap();
    let auth = authorities_at(temp.path());
    seed_session_run(&auth, "run-old", Some("run.interrupted"));
    seed_session_run(&auth, "run-new", Some("run.completed"));
    assert_resume_is_rejected(&auth, "not interrupted");
}

#[test]
fn resume_without_a_run_ledger_is_rejected() {
    let handle =
        RuntimeHandle::spawn(RuntimeHost::new("session-resume", RuntimeConfig::default())).unwrap();
    assert!(handle
        .resume()
        .unwrap_err()
        .contains("ledger is unavailable"));
}

#[test]
fn resume_after_a_restart_builds_the_runtime_from_the_persisted_session() {
    let temp = tempfile::tempdir().unwrap();
    let state_dir = temp.path();
    let provider = MockProvider::start(1);
    let models = crate::model_authority::ModelAuthority::open(state_dir).unwrap();
    models
        .set_provider(
            "gateway",
            Some("openai"),
            &json!({"api_key": "k", "base_url": provider.base_url}),
        )
        .unwrap();
    models.add_model("gateway:model-a").unwrap();

    // The state of a session whose process died mid-run: a persisted session row, its messages and
    // an interrupted run. No runtime handle exists; the new process builds one on resume.
    let plane = crate::CoreControlPlane::open(state_dir).unwrap();
    crate::egress::set_default_level(state_dir, crate::egress::EgressLevel::Open, false).unwrap();
    crate::control_plane::ensure_session(state_dir, "session-resume", None, "gateway:model-a")
        .unwrap();
    seed_session_run(
        plane.runtime_authorities(),
        "run-old",
        Some("run.interrupted"),
    );

    assert_eq!(
        plane.runtime_resumable("session-resume"),
        json!({"resumable": true, "runId": "run-old"})
    );
    let accepted = plane.resume_runtime("session-resume", Arc::new(crate::NullSink));
    assert_eq!(accepted["ok"], true, "{accepted}");
    assert_eq!(accepted["runId"], "run-old");
    provider
        .first_response_started
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    provider.release_first_response.send(()).unwrap();
    for _ in 0..500 {
        let status = plane
            .runtime_authorities()
            .ledger
            .lock()
            .unwrap()
            .reader()
            .unwrap()
            .run_status("run-old")
            .unwrap();
        if status == "ok" {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    provider.finish();

    let types: Vec<String> = run_events(plane.runtime_authorities(), "run-old")
        .into_iter()
        .map(|event| event.r#type)
        .collect();
    assert_eq!(
        types.iter().filter(|kind| *kind == "run.started").count(),
        1,
        "{types:?}"
    );
    assert_eq!(
        types.iter().filter(|kind| *kind == "run.resumed").count(),
        1,
        "{types:?}"
    );
    assert_eq!(types.last().map(String::as_str), Some("run.completed"));
    assert_eq!(
        plane.runtime_resumable("session-resume")["resumable"],
        false
    );
}

#[test]
fn resume_through_the_control_plane_is_rejected_without_writing_when_nothing_is_interrupted() {
    let temp = tempfile::tempdir().unwrap();
    let plane = crate::CoreControlPlane::open(temp.path()).unwrap();
    seed_session_run(
        plane.runtime_authorities(),
        "run-done",
        Some("run.completed"),
    );
    let before = plane
        .runtime_authorities()
        .ledger
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .all_events()
        .unwrap()
        .len();
    let rejected = plane.resume_runtime("session-resume", Arc::new(crate::NullSink));
    assert_eq!(rejected["ok"], false);
    assert!(
        rejected["error"]
            .as_str()
            .unwrap()
            .contains("not interrupted"),
        "{rejected}"
    );
    let after = plane
        .runtime_authorities()
        .ledger
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .all_events()
        .unwrap()
        .len();
    assert_eq!(before, after);
    assert_eq!(
        plane.runtime_resumable("session-resume")["resumable"],
        false
    );
}

// --- Egress levels on model requests (ADR-0057) ---

fn egress_host(base_url: String, authorities: RuntimeAuthorities) -> RuntimeHost {
    RuntimeHost::new(
        "session-egress",
        RuntimeConfig {
            model: "test-model".to_string(),
            protocol: "openai_chat".to_string(),
            api_key: "test-key".to_string(),
            base_url,
            max_iterations: 3,
            max_retries: 0,
            ..RuntimeConfig::default()
        },
    )
    .with_authorities(authorities)
}

fn ledger_events(authorities: &RuntimeAuthorities, run_id: &str) -> Vec<LedgerEvent> {
    authorities
        .ledger()
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .events(run_id)
        .unwrap()
}

#[test]
fn nothing_is_sent_before_the_user_chooses_an_egress_level() {
    let provider = RetryMockProvider::start(Vec::new());
    let temp = tempfile::tempdir().unwrap();
    let authorities = RuntimeAuthorities::open(temp.path()).unwrap();
    let mut host = egress_host(provider.base_url.clone(), authorities.clone());
    host.set_run_id("run-egress-unset".to_string());
    let result = host.run("the batch data", None);
    let error = result.unwrap_err();
    assert!(error.contains("Choose where"), "{error}");
    assert_eq!(provider.finish(), 0);
    let events = ledger_events(&authorities, "run-egress-unset");
    assert!(events.iter().any(|e| e.r#type == "model.request_refused"));
    assert!(!events.iter().any(|e| e.r#type == "model.request"));
    assert!(events.iter().any(|e| e.r#type == "run.failed"));
}

#[test]
fn a_private_only_workspace_may_use_a_local_model_and_records_the_request_without_content() {
    let provider = RetryMockProvider::start(vec![RetryReply::Complete("ok")]);
    let temp = tempfile::tempdir().unwrap();
    let authorities = RuntimeAuthorities::open(temp.path()).unwrap();
    crate::egress::set_default_level(temp.path(), crate::egress::EgressLevel::PrivateOnly, false)
        .unwrap();
    let mut host = egress_host(provider.base_url.clone(), authorities.clone());
    host.set_run_id("run-egress-private".to_string());
    host.run("the confidential formulation", None).unwrap();
    assert_eq!(provider.finish(), 1);
    let events = ledger_events(&authorities, "run-egress-private");
    let request = events
        .iter()
        .find(|e| e.r#type == "model.request")
        .expect("model.request recorded");
    assert_eq!(request.payload["endpoint"], "127.0.0.1");
    assert_eq!(request.payload["endpoint_class"], "private");
    assert_eq!(request.payload["level"], "private_only");
    assert!(request.payload["bytes"].as_u64().unwrap() > 0);
    // The emitted fields are exactly the ones runtime-public-contract.md defines.
    let mut keys: Vec<&str> = request
        .payload
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "bytes",
            "endpoint",
            "endpoint_class",
            "iteration",
            "level",
            "model",
            "protocol",
            "sources"
        ]
    );
    assert!(!request
        .payload
        .to_string()
        .contains("confidential formulation"));
}

#[test]
fn a_private_only_workspace_refuses_an_endpoint_that_is_not_private() {
    let provider = RetryMockProvider::start(Vec::new());
    let temp = tempfile::tempdir().unwrap();
    let authorities = RuntimeAuthorities::open(temp.path()).unwrap();
    crate::egress::set_default_level(temp.path(), crate::egress::EgressLevel::PrivateOnly, false)
        .unwrap();
    // An approved mark is not a private one: the endpoint stops counting as private.
    crate::egress::set_endpoint_mark(
        temp.path(),
        "127.0.0.1",
        Some(crate::egress::EndpointMark::Approved),
        true,
    )
    .unwrap();
    let mut host = egress_host(provider.base_url.clone(), authorities.clone());
    host.set_run_id("run-egress-denied".to_string());
    let error = host.run("the batch data", None).unwrap_err();
    assert!(error.contains("Not sent to 127.0.0.1"), "{error}");
    assert_eq!(provider.finish(), 0);
    let events = ledger_events(&authorities, "run-egress-denied");
    assert!(!events.iter().any(|e| e.r#type == "model.request"));
}

#[test]
fn ask_each_time_asks_once_per_run_and_a_denial_sends_nothing() {
    // Approved: two model requests in one run, one question.
    let provider =
        RetryMockProvider::start(vec![RetryReply::ToolCall, RetryReply::Complete("done")]);
    let temp = tempfile::tempdir().unwrap();
    let authorities = authorities_at(temp.path());
    crate::egress::set_default_level(temp.path(), crate::egress::EgressLevel::AskEachTime, false)
        .unwrap();
    let host = egress_host(provider.base_url.clone(), authorities.clone())
        .with_tools(tool_contract("low", false))
        .with_tool_executor(Arc::new(CountingExecutor {
            calls: Arc::new(AtomicUsize::new(0)),
            staged: None,
        }));
    let handle = RuntimeHandle::spawn(host).unwrap();
    let run_id = handle.run("write the report".to_string(), None).unwrap();
    let mut asked = 0;
    for _ in 0..500 {
        if let Some(id) = handle.pending_approvals().first().cloned() {
            assert!(id.starts_with("egress-"), "{id}");
            handle.resolve_approval(Some(&id), "once").unwrap();
            asked += 1;
        }
        if !handle.state().is_active() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    wait_for_terminal(&handle);
    assert_eq!(handle.state(), RuntimeState::Completed);
    assert_eq!(asked, 1);
    assert_eq!(provider.finish(), 2);
    let requests = ledger_events(&authorities, &run_id)
        .into_iter()
        .filter(|e| e.r#type == "model.request")
        .count();
    assert_eq!(requests, 2);

    // Denied: nothing is sent.
    let provider = RetryMockProvider::start(Vec::new());
    let host = egress_host(provider.base_url.clone(), authorities.clone());
    let handle = RuntimeHandle::spawn(host).unwrap();
    handle.run("again".to_string(), None).unwrap();
    for _ in 0..500 {
        if let Some(id) = handle.pending_approvals().first().cloned() {
            handle.resolve_approval(Some(&id), "deny").unwrap();
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    wait_for_terminal(&handle);
    assert_eq!(handle.state(), RuntimeState::Failed);
    assert_eq!(provider.finish(), 0);
}

#[test]
fn an_unattended_run_without_an_egress_level_leaves_one_inbox_notice() {
    let temp = tempfile::tempdir().unwrap();
    let authorities = RuntimeAuthorities::open(temp.path()).unwrap();
    for run in ["run-auto-1", "run-auto-2"] {
        let provider = RetryMockProvider::start(Vec::new());
        let mut host = RuntimeHost::new(
            "session-auto",
            RuntimeConfig {
                model: "test-model".to_string(),
                protocol: "openai_chat".to_string(),
                api_key: "test-key".to_string(),
                base_url: provider.base_url.clone(),
                unattended: true,
                ..RuntimeConfig::default()
            },
        )
        .with_authorities(authorities.clone());
        host.set_run_id(run.to_string());
        assert!(host.run("nightly summary", None).is_err());
        assert_eq!(provider.finish(), 0);
    }
    let notices: Vec<_> = authorities
        .inbox()
        .list(Some("session-auto"), Some("pending"))
        .into_iter()
        .filter(|item| item.kind == "notice")
        .collect();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].body.contains("Nothing was sent"));
}

#[test]
fn a_notice_is_dismissed_without_a_running_session() {
    let temp = tempfile::tempdir().unwrap();
    let plane = crate::CoreControlPlane::open(temp.path()).unwrap();
    let notice = plane
        .runtime_authorities()
        .inbox()
        .add_notice("session-gone", "egress-not_chosen", "Title", "Body")
        .unwrap();
    let out = plane.resolve_inbox(&notice.id, "dismissed");
    assert_eq!(out["ok"], true, "{out}");
    assert_eq!(
        plane
            .runtime_authorities()
            .inbox()
            .get(&notice.id)
            .unwrap()
            .state,
        "resolved"
    );
}

/// A tool declared the way the product declares an outbound tool (ADR-0061).
fn outbound_contract(
    name: &str,
    category: &str,
    requires_approval: bool,
    destination: &str,
) -> Value {
    json!([{
        "type": "function",
        "function": {
            "name": name,
            "parameters": {
                "type": "object",
                "properties": {"url": {"type": "string"}}
            },
            "metadata": {
                "risk_level": if requires_approval { "medium" } else { "low" },
                "requires_approval": requires_approval,
                "category": category,
                "capabilities": [],
                "destination": destination
            }
        }
    }])
}

/// A host whose workspace level is `level` (`None`: the user has not chosen one yet).
fn outbound_host(
    state_dir: &Path,
    level: Option<crate::egress::EgressLevel>,
    contract: Value,
    calls: Arc<AtomicUsize>,
) -> (RuntimeHost, RuntimeAuthorities) {
    let authorities = RuntimeAuthorities::open(state_dir).unwrap();
    if let Some(level) = level {
        crate::egress::set_default_level(state_dir, level, false).unwrap();
    }
    let mut host = RuntimeHost::new("session-outbound", RuntimeConfig::default())
        .with_authorities(authorities.clone())
        .with_tools(contract)
        .with_tool_executor(Arc::new(CountingExecutor {
            calls,
            staged: None,
        }));
    host.set_run_id("run-outbound".to_string());
    (host, authorities)
}

fn outbound_events(authorities: &RuntimeAuthorities) -> Vec<crate::ledger::LedgerEvent> {
    authorities
        .ledger
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .events("run-outbound")
        .unwrap()
}

#[test]
fn private_only_refuses_a_public_fetch_before_anything_runs() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (host, authorities) = outbound_host(
        temp.path(),
        Some(crate::egress::EgressLevel::PrivateOnly),
        outbound_contract("web_fetch", "web", true, "any_public"),
        calls.clone(),
    );
    let result = host.execute_tool_call(&ToolCall {
        id: "call-fetch".to_string(),
        name: "web_fetch".to_string(),
        arguments: json!({"url": "https://docs.example.test/page"}),
    });
    let error = result.error.expect("the call must be refused");
    assert!(error.contains("Private only"), "{error}");
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let events = outbound_events(&authorities);
    let proposed = events
        .iter()
        .find(|event| event.r#type == "tool.proposed")
        .expect("the proposal is recorded");
    assert_eq!(proposed.payload["egress"]["class"], "any_public");
    assert_eq!(proposed.payload["egress"]["host"], "docs.example.test");
    assert_eq!(proposed.payload["egress"]["level"], "private_only");
    assert_eq!(proposed.payload["egress"]["decision"], "blocked");
    let failed = events
        .iter()
        .find(|event| event.r#type == "tool.failed")
        .expect("the refusal is recorded");
    assert_eq!(failed.payload["stage"], "egress");
}

#[test]
fn nothing_outbound_runs_before_the_user_chooses_a_level() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (host, _authorities) = outbound_host(
        temp.path(),
        None,
        outbound_contract("web_search", "web", true, "any_public"),
        calls.clone(),
    );
    let result = host.execute_tool_call(&ToolCall {
        id: "call-search".to_string(),
        name: "web_search".to_string(),
        arguments: json!({"query": "shelf life"}),
    });
    let error = result.error.expect("the call must be refused");
    assert!(error.contains("Choose where"), "{error}");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn ask_each_time_asks_for_an_outbound_call_even_when_the_tool_needs_no_approval() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (host, _authorities) = outbound_host(
        temp.path(),
        Some(crate::egress::EgressLevel::AskEachTime),
        // A local server the user has set to run without approval: normally automatic.
        outbound_contract("local_notes", "mcp", false, "local_process"),
        calls.clone(),
    );
    let host = Arc::new(host);
    let worker_host = host.clone();
    let worker = std::thread::spawn(move || {
        worker_host.execute_tool_call(&ToolCall {
            id: "call-ask".to_string(),
            name: "local_notes".to_string(),
            arguments: json!({}),
        })
    });
    for _ in 0..200 {
        if host.approvals.pending_ids() == ["call-ask"] {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(host.approvals.pending_ids(), ["call-ask"]);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    host.approvals
        .resolve(Some("call-ask"), ApprovalDecision::Once)
        .unwrap();
    let result = worker.join().unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn a_local_process_tool_is_refused_under_private_only_and_runs_after_approval_when_open() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (host, _authorities) = outbound_host(
        temp.path(),
        Some(crate::egress::EgressLevel::PrivateOnly),
        outbound_contract("local_notes", "mcp", false, "local_process"),
        calls.clone(),
    );
    let result = host.execute_tool_call(&ToolCall {
        id: "call-local".to_string(),
        name: "local_notes".to_string(),
        arguments: json!({}),
    });
    assert!(result.error.is_some());
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let open_temp = tempfile::tempdir().unwrap();
    let open_calls = Arc::new(AtomicUsize::new(0));
    let (open_host, _open_authorities) = outbound_host(
        open_temp.path(),
        Some(crate::egress::EgressLevel::Open),
        outbound_contract("local_notes", "mcp", false, "local_process"),
        open_calls.clone(),
    );
    // MCP tools ask for approval under every egress level, even when the manifest
    // declares no approval and the server runs on this machine.
    let open_host = Arc::new(open_host);
    let worker_host = open_host.clone();
    let worker = std::thread::spawn(move || {
        worker_host.execute_tool_call(&ToolCall {
            id: "call-local-open".to_string(),
            name: "local_notes".to_string(),
            arguments: json!({}),
        })
    });
    for _ in 0..200 {
        if open_host.approvals.pending_ids() == ["call-local-open"] {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(open_host.approvals.pending_ids(), ["call-local-open"]);
    assert_eq!(open_calls.load(Ordering::SeqCst), 0);
    open_host
        .approvals
        .resolve(Some("call-local-open"), ApprovalDecision::Once)
        .unwrap();
    let result = worker.join().unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(open_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn a_tool_that_sends_nothing_off_the_machine_runs_under_private_only() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (host, authorities) = outbound_host(
        temp.path(),
        Some(crate::egress::EgressLevel::PrivateOnly),
        tool_contract("low", false),
        calls.clone(),
    );
    let result = host.execute_tool_call(&ToolCall {
        id: "call-local-write".to_string(),
        name: "write_report".to_string(),
        arguments: json!({"path": "report.md"}),
    });
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let events = outbound_events(&authorities);
    let proposed = events
        .iter()
        .find(|event| event.r#type == "tool.proposed")
        .expect("the proposal is recorded");
    assert!(proposed.payload["egress"].is_null());
}

#[test]
fn a_local_server_the_user_marked_local_only_runs_under_private_only() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let contract = json!([{
        "type": "function",
        "function": {
            "name": "notes_search",
            "parameters": {"type": "object", "properties": {}},
            "metadata": {
                "risk_level": "low",
                "requires_approval": false,
                "category": "mcp",
                "capabilities": ["mcp"],
                "destination": "local_process",
                "server": "notes"
            }
        }
    }]);
    let (host, _authorities) = outbound_host(
        temp.path(),
        Some(crate::egress::EgressLevel::PrivateOnly),
        contract,
        calls.clone(),
    );
    crate::egress::set_local_only(temp.path(), "notes", true, true).unwrap();
    // The egress level lets a local server run; the MCP category still asks.
    let host = Arc::new(host);
    let worker_host = host.clone();
    let worker = std::thread::spawn(move || {
        worker_host.execute_tool_call(&ToolCall {
            id: "call-notes".to_string(),
            name: "notes_search".to_string(),
            arguments: json!({}),
        })
    });
    for _ in 0..200 {
        if host.approvals.pending_ids() == ["call-notes"] {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(host.approvals.pending_ids(), ["call-notes"]);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    host.approvals
        .resolve(Some("call-notes"), ApprovalDecision::Once)
        .unwrap();
    let result = worker.join().unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn a_confirmed_allow_rule_lets_a_refused_search_run_after_its_approval() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (host, authorities) = outbound_host(
        temp.path(),
        Some(crate::egress::EgressLevel::PrivateOnly),
        outbound_contract("web_search", "web", true, "any_public"),
        calls.clone(),
    );
    crate::egress::set_path_rule(
        temp.path(),
        "",
        "web_search",
        "any_public",
        Some(crate::egress::PathRule::Allow),
        true,
    )
    .unwrap();
    let host = Arc::new(host);
    let worker_host = host.clone();
    let worker = std::thread::spawn(move || {
        worker_host.execute_tool_call(&ToolCall {
            id: "call-search-allowed".to_string(),
            name: "web_search".to_string(),
            arguments: json!({"query": "shelf life"}),
        })
    });
    for _ in 0..200 {
        if host.approvals.pending_ids() == ["call-search-allowed"] {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(host.approvals.pending_ids(), ["call-search-allowed"]);
    host.approvals
        .resolve(Some("call-search-allowed"), ApprovalDecision::Once)
        .unwrap();
    let result = worker.join().unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let events = outbound_events(&authorities);
    let proposed = events
        .iter()
        .find(|event| event.r#type == "tool.proposed")
        .expect("the proposal is recorded");
    assert_eq!(proposed.payload["egress"]["override"], "allow");
    assert_eq!(proposed.payload["egress"]["path"], "web_search");
    assert_eq!(proposed.payload["egress"]["decision"], "allowed");
}

#[test]
fn a_connector_tool_without_a_declared_destination_is_refused_under_private_only() {
    let temp = tempfile::tempdir().unwrap();
    let contract = json!([{
        "type": "function",
        "function": {
            "name": "mail_send",
            "parameters": {"type": "object", "properties": {}},
            "metadata": {
                "risk_level": "low",
                "requires_approval": false,
                "category": "connector",
                "capabilities": ["integration"],
                "connector": "mail"
            }
        }
    }]);
    let calls = Arc::new(AtomicUsize::new(0));
    let (host, _authorities) = outbound_host(
        temp.path(),
        Some(crate::egress::EgressLevel::PrivateOnly),
        contract,
        calls.clone(),
    );
    let result = host.execute_tool_call(&ToolCall {
        id: "call-mail".to_string(),
        name: "mail_send".to_string(),
        arguments: json!({}),
    });
    assert!(result.error.is_some());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

// Compaction tests -----------------------------------------------------------

/// A minimal mock provider that answers `provider::complete`'s non-streaming
/// OpenAI Chat Completions request with a fixed message body.
struct MockJsonProvider {
    base_url: String,
    /// Keeps the accept-loop thread alive until the test drops the provider.
    _worker: std::thread::JoinHandle<()>,
}

impl MockJsonProvider {
    fn start(body: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind json provider");
        let address = listener.local_addr().expect("json provider address");
        let served = body.clone();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept json request");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let _ = read_http_request(&mut stream);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            write!(stream, "{served}").unwrap();
            stream.flush().unwrap();
        });
        Self {
            base_url: format!("http://{address}/v1"),
            _worker: worker,
        }
    }
}

#[test]
fn a_compacted_session_replaces_the_prefix_with_the_summary() {
    let messages = vec![
        json!({"role": "system", "content": "you are helpful"}),
        json!({"role": "user", "content": "part one"}),
        json!({"role": "assistant", "content": "reply one"}),
        json!({"role": "user", "content": "the live question"}),
    ];
    let mut host = RuntimeHost::new("s", RuntimeConfig::default()).with_messages(messages);
    host.compaction_summary = Some("SUPER".to_string());
    host.compaction_boundary = 3; // history 0..3 -> system + summary, keep [3] (live)
    let out = host.outbound_messages();
    let roles: Vec<&str> = out.iter().map(|m| m["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["system", "user", "user"]);
    assert_eq!(out[0]["content"], "you are helpful");
    assert!(out[1]["content"].as_str().unwrap().contains("SUPER"));
    assert_eq!(out[2]["content"], "the live question");

    // A stale boundary from a prior session clamps to the transcript instead of
    // swallowing it.
    host.compaction_boundary = 100;
    let out = host.outbound_messages();
    assert!(out
        .iter()
        .any(|m| m.get("role").and_then(|r| r.as_str()) == Some("assistant")));
}

#[test]
fn compaction_estimate_counts_text_tokens_only() {
    let messages = vec![
        json!({"role": "system", "content": "s"}),
        json!({"role": "user", "content": "a".repeat(400)}),
        json!({"role": "assistant", "content": "b".repeat(400)}),
    ];
    let host = RuntimeHost::new("s", RuntimeConfig::default()).with_messages(messages);
    let estimate = host.estimate_context_tokens();
    assert!(estimate >= 200, "chars/4 ~ 200; got {estimate}");
    assert!(estimate < 300, "overhead is bounded; got {estimate}");
}

#[test]
fn compaction_estimate_counts_a_chinese_character_as_a_token() {
    let messages = vec![
        json!({"role": "system", "content": "s"}),
        json!({"role": "user", "content": "汉".repeat(400)}),
    ];
    let host = RuntimeHost::new("s", RuntimeConfig::default()).with_messages(messages);
    let estimate = host.estimate_context_tokens();
    assert!(estimate >= 400, "one token per character; got {estimate}");
    assert!(estimate < 450, "overhead is bounded; got {estimate}");
}

#[test]
fn tool_call_ids_are_unique_within_a_transcript() {
    let host = RuntimeHost::new("s", RuntimeConfig::default()).with_messages(vec![
        json!({"role": "user", "content": "go"}),
        tool_call_message("call_0"),
        json!({"role": "tool", "tool_call_id": "call_0", "content": "done"}),
    ]);
    let call = |id: &str| ToolCall {
        id: id.to_string(),
        name: "t".to_string(),
        arguments: json!({}),
    };
    // An id used earlier, a missing id, a repeat within the reply, and one that is fine.
    let mut calls = vec![call("call_0"), call(""), call("call_0"), call("fresh")];
    host.unique_call_ids(&mut calls);
    let ids: Vec<&str> = calls.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids[3], "fresh", "a unique id is kept");
    let distinct: std::collections::HashSet<&&str> = ids.iter().collect();
    assert_eq!(distinct.len(), 4);
    assert!(ids.iter().all(|id| !id.is_empty() && *id != "call_0"));
}

#[test]
fn a_tool_result_read_back_from_storage_goes_out_as_the_model_first_saw_it() {
    let host = RuntimeHost::new("s", RuntimeConfig::default()).with_messages(vec![
        json!({"role": "user", "content": "go"}),
        tool_call_message("call_1"),
        // The shape the stored transcript gives a result: structured output, a name, an error slot.
        json!({"role": "tool", "tool_call_id": "call_1", "name": "t",
               "content": {"ok": true}, "error": null}),
        // A result for a call the model never made (the text-search step).
        json!({"role": "tool", "tool_call_id": "text-search-1", "name": "web_search",
               "content": {"hits": []}, "error": null}),
    ]);
    let out = host.outbound_messages();
    let roles: Vec<&str> = out.iter().map(|m| m["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["user", "assistant", "tool"]);
    assert_eq!(out[2]["content"], "{\"ok\":true}");
    assert!(out[2].get("name").is_none() && out[2].get("error").is_none());
    assert_eq!(out[2]["tool_call_id"], "call_1");
}

#[test]
fn a_short_context_does_not_compact() {
    let mut host = RuntimeHost::new("s", RuntimeConfig::default()).with_messages(vec![
        json!({"role": "system", "content": "s"}),
        json!({"role": "user", "content": "hi"}),
    ]);
    let mut confirmed = false;
    // window None, cap 250000 -> threshold 250000; estimate is tiny.
    assert!(host.maybe_compact(&mut confirmed).is_ok());
    assert!(!confirmed);
    assert!(!host.compacted_in_turn);
}

#[test]
fn a_full_context_triggers_one_compaction_per_turn() {
    let prompt = "x".repeat(200_000);
    let config = RuntimeConfig {
        context_window_tokens: Some(20_000),
        compaction_threshold_pct: 0.5, // threshold 10k tokens; estimate ~50k -> must compact.
        ..RuntimeConfig::default()
    };
    let mut host = RuntimeHost::new("s", config).with_messages(vec![
        json!({"role": "system", "content": "s"}),
        json!({"role": "user", "content": &prompt}),
        json!({"role": "assistant", "content": &prompt}),
    ]);
    let mut confirmed = false;
    // No base_url authority: the summarizer HTTP call fails, but the trigger
    // still fires and marks the turn as compacted so it is not retried.
    let result = host.maybe_compact(&mut confirmed);
    assert!(result.is_err(), "a summarizer failure is propagated");
    assert!(host.compacted_in_turn);
    // The second attempt in the same turn is a no-op regardless of failure.
    let before = host.compaction_boundary;
    let _ = host.maybe_compact(&mut confirmed);
    assert_eq!(before, host.compaction_boundary);
}

#[test]
fn a_successful_compaction_replaces_future_messages() {
    let prompt = "x".repeat(200_000);
    let provider = MockJsonProvider::start(
        r#"{"choices":[{"message":{"content":"THE SUMMARY"}}]}"#.to_string(),
    );
    let config = RuntimeConfig {
        base_url: provider.base_url.clone(),
        api_key: "test-key".to_string(),
        context_window_tokens: Some(20_000),
        compaction_threshold_pct: 0.5,
        ..RuntimeConfig::default()
    };
    let mut host = RuntimeHost::new("s", config).with_messages(vec![
        json!({"role": "system", "content": "s"}),
        json!({"role": "user", "content": &prompt}),
        json!({"role": "assistant", "content": &prompt}),
        json!({"role": "user", "content": "live input"}),
    ]);
    let mut confirmed = false;
    assert!(host.maybe_compact(&mut confirmed).is_ok());
    assert_eq!(
        host.compaction_summary.as_deref(),
        Some("THE SUMMARY"),
        "the persisted summary is installed for outbound requests"
    );
    let out = host.outbound_messages();
    let last = out.last().unwrap();
    assert_eq!(
        last["content"], "live input",
        "the live user input is kept verbatim"
    );
    assert!(
        out.iter()
            .any(|m| m["content"].as_str().unwrap().contains("THE SUMMARY")),
        "the summary replaces the collapsed history"
    );
    drop(provider);
}

fn compacting_config(provider: &MockJsonProvider) -> RuntimeConfig {
    RuntimeConfig {
        base_url: provider.base_url.clone(),
        api_key: "test-key".to_string(),
        system_prompt: Some("rules".to_string()),
        context_window_tokens: Some(20_000),
        compaction_threshold_pct: 0.5,
        ..RuntimeConfig::default()
    }
}

fn tool_call_message(id: &str) -> Value {
    json!({"role": "assistant", "tool_calls": [
        {"id": id, "type": "function", "function": {"name": "t", "arguments": "{}"}}
    ]})
}

#[test]
fn compacting_in_the_middle_of_a_tool_exchange_keeps_each_result_after_its_call() {
    let prompt = "x".repeat(200_000);
    let provider = MockJsonProvider::start(
        r#"{"choices":[{"message":{"content":"THE SUMMARY"}}]}"#.to_string(),
    );
    let mut host = RuntimeHost::new("s", compacting_config(&provider)).with_messages(vec![
        json!({"role": "user", "content": &prompt}),
        tool_call_message("call_1"),
        json!({"role": "tool", "tool_call_id": "call_1", "content": "one"}),
        tool_call_message("call_2"),
        json!({"role": "tool", "tool_call_id": "call_2", "content": "two"}),
    ]);
    let mut confirmed = false;
    host.maybe_compact(&mut confirmed).unwrap();

    let out = host.outbound_messages();
    let roles: Vec<&str> = out.iter().map(|m| m["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["system", "user", "assistant", "tool"]);
    assert!(out[1]["content"].as_str().unwrap().contains("THE SUMMARY"));
    assert_eq!(out[3]["tool_call_id"], "call_2");
    drop(provider);
}

#[test]
fn a_persisted_compaction_is_restored_on_the_same_message() {
    let temp = tempfile::tempdir().unwrap();
    crate::control_plane::ensure_session(temp.path(), "s", None, "m").unwrap();
    let prompt = "x".repeat(200_000);
    let provider = MockJsonProvider::start(
        r#"{"choices":[{"message":{"content":"THE SUMMARY"}}]}"#.to_string(),
    );
    let transcript = vec![
        json!({"role": "user", "content": &prompt}),
        json!({"role": "assistant", "content": &prompt}),
        json!({"role": "user", "content": "live input"}),
    ];
    let mut host = RuntimeHost::new("s", compacting_config(&provider))
        .with_authorities(authorities_at(temp.path()))
        .with_messages(transcript.clone());
    let mut confirmed = false;
    host.maybe_compact(&mut confirmed).unwrap();
    let live = host.outbound_messages();

    // A new host is built from the stored transcript (which has no system message) and the
    // stored compaction.
    let saved = crate::control_plane::get_session_compaction(temp.path(), "s")
        .unwrap()
        .unwrap();
    let restored = RuntimeHost::new("s", compacting_config(&provider))
        .with_messages(transcript.clone())
        .with_persisted_compaction(
            saved["summary"].as_str().unwrap().to_string(),
            saved["boundary"].as_u64().unwrap() as usize,
            saved["anchor"].as_str().unwrap(),
        );
    assert_eq!(restored.outbound_messages(), live);

    // A transcript that no longer lines up with the stored boundary drops the compaction.
    let mut drifted = transcript;
    drifted.insert(
        0,
        json!({"role": "user", "content": "an extra earlier message"}),
    );
    let restored = RuntimeHost::new("s", compacting_config(&provider))
        .with_messages(drifted)
        .with_persisted_compaction(
            saved["summary"].as_str().unwrap().to_string(),
            saved["boundary"].as_u64().unwrap() as usize,
            saved["anchor"].as_str().unwrap(),
        );
    assert!(restored.compaction_summary.is_none());
    drop(provider);
}

/// Poison `mutex` the way a panic inside a contained run would: a thread dies holding it.
fn poison<T: Send + 'static>(mutex: &Arc<Mutex<T>>) {
    let held = mutex.clone();
    let _ = std::thread::spawn(move || {
        let _guard = held.lock().unwrap();
        panic!("a panic while the lock is held");
    })
    .join();
    assert!(mutex.is_poisoned());
}

#[test]
fn a_contained_panic_does_not_lock_the_session_out_of_its_own_state() {
    let temp = tempfile::tempdir().unwrap();
    let authorities = authorities_at(temp.path());
    let host =
        RuntimeHost::new("s", RuntimeConfig::default()).with_authorities(authorities.clone());
    let handle = RuntimeHandle::spawn(host).unwrap();

    poison(&handle.state);
    poison(&handle.steering);
    poison(&authorities.ledger);

    assert_eq!(handle.state(), RuntimeState::Idle);
    assert!(
        handle.steer("more", None).is_err(),
        "idle: refused, not a panic"
    );
    assert!(handle.pending_approvals().is_empty());
    // Reading the ledger for a resumable run still works, and says why there is none.
    let error = handle.resume().unwrap_err();
    assert!(error.contains("nothing to resume"), "{error}");
}

#[test]
fn each_run_may_compact_again() {
    let config = RuntimeConfig {
        base_url: "http://127.0.0.1:9/v1".to_string(),
        api_key: "k".to_string(),
        max_retries: 0,
        ..RuntimeConfig::default()
    };
    let mut host = RuntimeHost::new("s", config);
    // The flag a previous run left behind.
    host.compacted_in_turn = true;
    let _ = host.run("hello", None);
    assert!(
        !host.compacted_in_turn,
        "a new run starts with its compaction allowance"
    );
}

#[test]
fn truncating_below_the_compaction_boundary_discards_the_summary() {
    let mut host = RuntimeHost::new("s", RuntimeConfig::default()).with_messages(vec![
        json!({"role": "user", "content": "a"}),
        json!({"role": "assistant", "content": "b"}),
        json!({"role": "user", "content": "c"}),
    ]);
    host.compaction_summary = Some("S".to_string());
    host.compaction_boundary = 2;
    host.truncate_messages(1);
    assert!(host.compaction_summary.is_none());
    assert_eq!(host.compaction_boundary, 0);
}

#[test]
fn the_registry_round_trips_a_session_compaction() {
    let temp = tempfile::tempdir().unwrap();
    let state_dir = temp.path();
    crate::control_plane::ensure_session(state_dir, "session-c", None, "test-model").unwrap();
    crate::control_plane::set_session_compaction(
        state_dir,
        "session-c",
        &json!({
            "summary": "A summary",
            "boundary": 7,
            "created_at": 1.0,
        }),
    )
    .unwrap();
    let persisted = crate::control_plane::get_session_compaction(state_dir, "session-c")
        .unwrap()
        .unwrap();
    assert_eq!(persisted["summary"], json!("A summary"));
    assert_eq!(persisted["boundary"], json!(7));
    let missing = crate::control_plane::get_session_compaction(state_dir, "session-missing");
    assert!(
        matches!(missing, Ok(None)),
        "missing session has no compaction"
    );
}

/// Panics once, on the first error frame, then behaves like a capturing sink: stands for any panic
/// inside a run (a tool, a document parser) after the run has started.
struct PanicOnErrorSink {
    is_armed: std::sync::atomic::AtomicBool,
    frames: Mutex<Vec<Value>>,
}

impl EventSink for PanicOnErrorSink {
    fn emit(&self, frame: Value) -> Result<(), String> {
        let is_error = frame.get("type").and_then(Value::as_str) == Some("error");
        if is_error && self.is_armed.swap(false, Ordering::SeqCst) {
            panic!("simulated panic inside a run");
        }
        self.frames.lock().unwrap().push(frame);
        Ok(())
    }
}

#[test]
fn a_panic_inside_a_run_fails_that_run_and_keeps_the_session_usable() {
    let temp = tempfile::tempdir().unwrap();
    let authorities = authorities_at(temp.path());
    let ledger = authorities.ledger();
    let sink = Arc::new(PanicOnErrorSink {
        is_armed: std::sync::atomic::AtomicBool::new(true),
        frames: Mutex::new(Vec::new()),
    });
    // Nothing listens on port 1, so every model request fails at once and no run waits on a network.
    let config = RuntimeConfig {
        model: "test-model".to_string(),
        api_key: "test-key".to_string(),
        base_url: "http://127.0.0.1:1".to_string(),
        max_retries: 0,
        ..RuntimeConfig::default()
    };
    let host = RuntimeHost::new("session-panic", config)
        .with_authorities(authorities)
        .with_event_sink(sink.clone());
    let handle = RuntimeHandle::spawn(host).unwrap();

    let run_id = handle.run("first".to_string(), None).unwrap();
    wait_for_terminal(&handle);
    assert_eq!(handle.state(), RuntimeState::Failed);

    // The ledger run is closed, not left open for the next restart to call a crash.
    let status = ledger
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .run_status(&run_id)
        .unwrap();
    assert_eq!(status, "error");

    // The product surface is told the run ended, with the reason.
    let frames = sink.frames.lock().unwrap().clone();
    let last_error = frames
        .iter()
        .rev()
        .find(|frame| frame["type"] == "error")
        .expect("the failure is reported as an error frame");
    assert_eq!(last_error["payload"]["terminal"], json!(true));
    assert!(last_error["payload"]["error"]
        .as_str()
        .unwrap_or_default()
        .contains("simulated panic inside a run"));
    assert!(frames.iter().any(|frame| frame["type"] == "turn_end"));

    // The worker survived: another run is accepted and also reaches a terminal state.
    handle
        .run("second".to_string(), None)
        .expect("the worker accepts a run after the panic");
    wait_for_terminal(&handle);
}

struct BigOutputExecutor {
    text: String,
}

impl ToolExecutor for BigOutputExecutor {
    fn execute(&self, call: &ToolCall, _context: &ToolExecutionContext) -> ToolResult {
        ToolResult::success(
            &call.id,
            json!({"ok": true, "text": self.text, "images": [{"path": "shot.png"}]}),
        )
    }
}

#[test]
fn a_huge_tool_output_is_cut_down_before_it_reaches_the_conversation_and_the_stores() {
    let temp = tempfile::tempdir().unwrap();
    let authorities = authorities_at(temp.path());
    let mut host = RuntimeHost::new("session-1", RuntimeConfig::default())
        .with_authorities(authorities.clone())
        .with_tools(tool_contract("low", false))
        .with_tool_executor(Arc::new(BigOutputExecutor {
            text: "payload ".repeat(250_000), // 2 MB
        }));
    host.set_run_id("run-big".to_string());
    let result = host.execute_tool_call(&ToolCall {
        id: "call-big".to_string(),
        name: "write_report".to_string(),
        arguments: json!({"path": "report.md"}),
    });
    assert!(result.error.is_none(), "{:?}", result.error);

    // What the model, the event stream and the transcript get is bounded and says it was cut.
    assert!(result.output.to_string().len() <= tool_execution::MAX_TOOL_OUTPUT_BYTES);
    assert_eq!(result.output["truncated"], json!(true));
    assert!(result.output["original_bytes"].as_u64().unwrap() > 2_000_000);
    assert_eq!(result.output["ok"], json!(true));
    assert_eq!(result.output["images"][0]["path"], json!("shot.png"));

    // The ledger keeps what happened (size, hash, a short preview), not the content.
    let events = authorities
        .ledger
        .lock()
        .unwrap()
        .reader()
        .unwrap()
        .events("run-big")
        .unwrap();
    let completed = events
        .iter()
        .find(|event| event.r#type == "tool.completed")
        .unwrap();
    let payload = completed.payload.to_string();
    assert!(
        payload.len() < 4_000,
        "the ledger kept {} bytes",
        payload.len()
    );
    assert!(completed.payload["result"]["sha256"].is_string());
    assert!(completed.payload["result"]["bytes"].as_u64().unwrap() <= 128 * 1024);
    assert!(
        completed.payload["result"]["original_bytes"]
            .as_u64()
            .unwrap()
            > 2_000_000
    );

    // A replay needs the result; it gets the bounded one.
    let entry = authorities
        .idempotency
        .lock()
        .unwrap()
        .get("run-big", "call-big")
        .unwrap()
        .unwrap();
    assert!(entry.result.to_string().len() <= tool_execution::MAX_TOOL_OUTPUT_BYTES);
}

#[test]
fn limiting_a_tool_output_leaves_small_ones_alone_and_cuts_on_a_character_boundary() {
    let small = json!({"ok": true, "text": "hello"});
    assert_eq!(tool_execution::limit_tool_output(small.clone()), small);

    let chinese = json!({"text": "中".repeat(100_000)}); // 300 KB of three-byte characters
    let limited = tool_execution::limit_tool_output(chinese);
    assert!(limited.to_string().len() <= tool_execution::MAX_TOOL_OUTPUT_BYTES);
    let preview = limited["preview"].as_str().unwrap();
    assert!(!preview.contains('\u{FFFD}'));
    assert!(limited["note"].as_str().unwrap().contains("limit"));
}

/// Serves one non-streaming chat completion whose assistant text is `reply`, as the auto-approve
/// reviewer (ADR-0084) receives it from the model endpoint.
fn serve_judge_reply(reply: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind judge");
    let address = listener.local_addr().expect("judge address");
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept judge request");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let _ = read_http_request(&mut stream);
        let body = json!({"choices": [{"message": {"role": "assistant", "content": reply}, "finish_reason": "stop"}]})
            .to_string();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    format!("http://{address}/v1")
}

/// A host whose newest user message is `request`, pointed at `base_url` for the judge.
fn reviewer_test_host(base_url: String, request: &str) -> RuntimeHost {
    let mut host = RuntimeHost::new(
        "session-review",
        RuntimeConfig {
            model: "judge-model".to_string(),
            protocol: "openai_chat".to_string(),
            api_key: "judge-key".to_string(),
            base_url,
            reviewer_enabled: true,
            ..RuntimeConfig::default()
        },
    );
    host.messages
        .push(json!({"role": "user", "content": request}));
    host
}

fn rename_call() -> ToolCall {
    ToolCall {
        id: "call-review-1".to_string(),
        name: "rename_file".to_string(),
        arguments: json!({"from": "draft.txt", "to": "final.txt"}),
    }
}

#[test]
fn the_reviewer_allows_a_call_the_request_covers() {
    let host = reviewer_test_host(
        serve_judge_reply(r#"{"verdict":"allow"}"#),
        "rename draft.txt to final.txt",
    );

    assert_eq!(
        host.review_call(&rename_call()),
        crate::reviewer::Verdict::Allow
    );
}

#[test]
fn the_reviewer_asks_when_the_judge_asks() {
    let host = reviewer_test_host(
        serve_judge_reply(r#"{"verdict":"ask"}"#),
        "rename draft.txt to final.txt",
    );

    assert_eq!(
        host.review_call(&rename_call()),
        crate::reviewer::Verdict::Ask
    );
}

#[test]
fn the_reviewer_asks_when_the_judge_answers_in_prose() {
    let host = reviewer_test_host(
        serve_judge_reply("Sure, this looks fine: {\"verdict\":\"allow\"}"),
        "rename draft.txt to final.txt",
    );

    assert_eq!(
        host.review_call(&rename_call()),
        crate::reviewer::Verdict::Ask
    );
}

#[test]
fn the_reviewer_asks_when_the_judge_is_unreachable() {
    let host = reviewer_test_host(
        "http://127.0.0.1:9/v1".to_string(),
        "rename draft.txt to final.txt",
    );

    assert_eq!(
        host.review_call(&rename_call()),
        crate::reviewer::Verdict::Ask
    );
}

#[test]
fn the_reviewer_asks_when_there_is_no_user_request_to_judge_against() {
    let host = RuntimeHost::new(
        "session-review-empty",
        RuntimeConfig {
            reviewer_enabled: true,
            base_url: "http://127.0.0.1:9/v1".to_string(),
            ..RuntimeConfig::default()
        },
    );

    assert_eq!(
        host.review_call(&rename_call()),
        crate::reviewer::Verdict::Ask
    );
}
