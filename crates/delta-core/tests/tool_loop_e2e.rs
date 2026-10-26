//! The agent's tool loop through the real product stack: the control plane, the product tool
//! library and the runtime, with only the model replaced by a local server. The model asks for
//! `read_file`; the tool must really run, and its result must come back in the next model request.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use delta_core::egress::{set_default_level, EgressLevel};
use delta_core::{CoreControlPlane, EventSink, RuntimeStartRequest};
use serde_json::{json, Value};

#[derive(Default)]
struct Frames(Mutex<Vec<Value>>);

impl EventSink for Frames {
    fn emit(&self, frame: Value) -> Result<(), String> {
        self.0.lock().unwrap().push(frame);
        Ok(())
    }
}

/// Answers `replies.len()` chat requests in order and records each request body.
fn model_server(replies: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let seen = bodies.clone();
    std::thread::spawn(move || {
        for reply in replies {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0_usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0_u8; length];
            reader.read_exact(&mut body).unwrap();
            seen.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&body).into_owned());
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{reply}"
            )
            .unwrap();
            let _ = stream.flush();
        }
    });
    (base_url, bodies)
}

fn tool_call_reply(name: &str, arguments: &str) -> String {
    let call = json!({"choices": [{"delta": {"tool_calls": [{
        "index": 0, "id": "call-e2e", "function": {"name": name, "arguments": arguments}
    }]}, "finish_reason": null}]});
    format!(
        "data: {call}\n\ndata: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"tool_calls\"}}]}}\n\ndata: [DONE]\n\n"
    )
}

fn text_reply(text: &str) -> String {
    let delta = json!({"choices": [{"delta": {"content": text}, "finish_reason": null}]});
    format!(
        "data: {delta}\n\ndata: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}\n\ndata: [DONE]\n\n"
    )
}

struct Stack {
    control: CoreControlPlane,
    frames: Arc<Frames>,
    bodies: Arc<Mutex<Vec<String>>>,
    _temp: tempfile::TempDir,
    workspace: std::path::PathBuf,
}

/// The real control plane with a local model server that gives `replies` in turn, speaking the
/// OpenAI protocol.
fn stack(replies: Vec<String>) -> Stack {
    stack_for("openai", replies)
}

fn stack_for(protocol: &str, replies: Vec<String>) -> Stack {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    let workspace = temp.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    set_default_level(&state, EgressLevel::Open, false).unwrap();
    let (base_url, bodies) = model_server(replies);
    let control = CoreControlPlane::open(&state).unwrap();
    {
        let models = control.models().lock().unwrap();
        models
            .set_provider(
                "local-gateway",
                Some(protocol),
                &json!({"api_key": "test-key", "base_url": base_url}),
            )
            .unwrap();
        models.add_model("local-gateway:test-model").unwrap();
    }
    Stack {
        control,
        frames: Arc::new(Frames::default()),
        bodies,
        _temp: temp,
        workspace,
    }
}

impl Stack {
    fn start(&self, input: &str) {
        let started = self.control.start_runtime(
            RuntimeStartRequest {
                session_id: "session-tool-loop".to_string(),
                model_id: "local-gateway:test-model".to_string(),
                user_input: input.to_string(),
                workspace: Some(self.workspace.to_string_lossy().into_owned()),
                attachments: None,
                skill: None,
                mode: None,
                max_iterations: Some(4),
                max_retries: Some(0),
                source: None,
            },
            self.frames.clone(),
        );
        assert_ne!(
            started["ok"],
            json!(false),
            "the run did not start: {started}"
        );
    }

    fn frames(&self) -> Vec<Value> {
        self.frames.0.lock().unwrap().clone()
    }

    fn wait_for(&self, kind: &str) -> Value {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(frame) = self.frames().into_iter().find(|f| f["type"] == kind) {
                return frame;
            }
            assert!(
                Instant::now() < deadline,
                "no {kind} frame; frames: {:?}",
                self.frames()
                    .iter()
                    .map(|f| f["type"].clone())
                    .collect::<Vec<_>>()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[test]
fn the_model_reads_a_workspace_file_through_the_product_tool_library() {
    let stack = stack(vec![
        tool_call_reply("read_file", r#"{"path":"notes.txt"}"#),
        text_reply("The file says alpha-bravo-42."),
    ]);
    fs::write(stack.workspace.join("notes.txt"), "alpha-bravo-42").unwrap();
    stack.start("what does notes.txt say?");

    let finished = stack.wait_for("tool_finished");
    assert_eq!(finished["payload"]["name"], json!("read_file"));
    assert!(
        finished["payload"]["error"].is_null(),
        "the tool failed: {finished}"
    );
    assert!(finished["payload"]["result"]
        .to_string()
        .contains("alpha-bravo-42"));

    // The result went back to the model, which answered from it, and the run ended well.
    let end = stack.wait_for("turn_end");
    assert_eq!(end["payload"]["status"], json!("completed"));
    let bodies = stack.bodies.lock().unwrap();
    assert_eq!(bodies.len(), 2, "the model should be called twice");
    assert!(
        bodies[0].contains("read_file"),
        "the tool was not offered to the model"
    );
    assert!(
        bodies[1].contains("alpha-bravo-42"),
        "the tool result did not reach the model"
    );
}

#[cfg(unix)]
#[test]
fn a_shell_command_waits_for_approval_then_runs_and_its_output_reaches_the_model() {
    let stack = stack(vec![
        tool_call_reply("run_shell", r#"{"command":"echo shell-e2e-ok"}"#),
        text_reply("It printed shell-e2e-ok."),
    ]);
    stack.start("run echo");

    // The run stops and asks; nothing has been executed yet.
    let asked = stack.wait_for("permission_required");
    assert_eq!(asked["payload"]["name"], json!("run_shell"));
    assert!(stack.frames().iter().all(|f| f["type"] != "tool_started"));

    let resolved =
        stack
            .control
            .runtime_resolve_approval("session-tool-loop", Some("call-e2e"), "once");
    assert_eq!(resolved["ok"], json!(true), "{resolved}");

    let finished = stack.wait_for("tool_finished");
    assert!(
        finished["payload"]["error"].is_null(),
        "the tool failed: {finished}"
    );
    assert!(finished["payload"]["result"]
        .to_string()
        .contains("shell-e2e-ok"));
    let end = stack.wait_for("turn_end");
    assert_eq!(end["payload"]["status"], json!("completed"));
    assert!(stack.bodies.lock().unwrap()[1].contains("shell-e2e-ok"));
}

#[cfg(unix)]
#[test]
fn a_denied_shell_command_never_runs() {
    let stack = stack(vec![
        tool_call_reply("run_shell", r#"{"command":"touch denied-marker"}"#),
        text_reply("Understood, I will not run it."),
    ]);
    stack.start("create the marker");
    stack.wait_for("permission_required");
    let resolved =
        stack
            .control
            .runtime_resolve_approval("session-tool-loop", Some("call-e2e"), "deny");
    assert_eq!(resolved["ok"], json!(true), "{resolved}");
    let end = stack.wait_for("turn_end");
    assert_eq!(end["payload"]["status"], json!("completed"));
    assert!(
        !stack.workspace.join("denied-marker").exists(),
        "a denied command ran"
    );
    assert!(stack.frames().iter().all(|f| f["type"] != "tool_started"));
}

fn anthropic_sse(events: &[Value]) -> String {
    events
        .iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            )
        })
        .collect()
}

fn anthropic_tool_reply(name: &str, arguments: &str) -> String {
    anthropic_sse(&[
        json!({"type": "message_start", "message": {"usage": {"input_tokens": 1, "output_tokens": 1}}}),
        json!({"type": "content_block_start", "index": 0,
               "content_block": {"type": "tool_use", "id": "call-e2e", "name": name, "input": {}}}),
        json!({"type": "content_block_delta", "index": 0,
               "delta": {"type": "input_json_delta", "partial_json": arguments}}),
        json!({"type": "content_block_stop", "index": 0}),
        json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 5}}),
        json!({"type": "message_stop"}),
    ])
}

fn anthropic_text_reply(text: &str) -> String {
    anthropic_sse(&[
        json!({"type": "message_start", "message": {"usage": {"input_tokens": 1, "output_tokens": 1}}}),
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": text}}),
        json!({"type": "content_block_stop", "index": 0}),
        json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 5}}),
        json!({"type": "message_stop"}),
    ])
}

#[test]
fn the_anthropic_protocol_runs_the_same_tool_loop() {
    let stack = stack_for(
        "anthropic",
        vec![
            anthropic_tool_reply("read_file", r#"{"path":"notes.txt"}"#),
            anthropic_text_reply("The file says alpha-bravo-42."),
        ],
    );
    fs::write(stack.workspace.join("notes.txt"), "alpha-bravo-42").unwrap();
    stack.start("what does notes.txt say?");

    let finished = stack.wait_for("tool_finished");
    assert!(
        finished["payload"]["error"].is_null(),
        "the tool failed: {finished}"
    );
    assert!(finished["payload"]["result"]
        .to_string()
        .contains("alpha-bravo-42"));
    let end = stack.wait_for("turn_end");
    assert_eq!(end["payload"]["status"], json!("completed"));
    let bodies = stack.bodies.lock().unwrap();
    assert_eq!(bodies.len(), 2);
    assert!(bodies[1].contains("tool_result") && bodies[1].contains("alpha-bravo-42"));
}
