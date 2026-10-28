//! The model-facing todo tool: the structured task list the UI renders.
//!
//! `todo_write` replaces the session's visible plan with the list the model
//! sends. The desktop surface picks the list up from the tool-proposal event
//! and renders it in the right rail, so the tool itself stays a pure
//! normalizer: it validates the shape, aliases the status spellings models
//! actually produce, and echoes the canonical list back. Low risk and
//! auto-approved — it writes nothing outside the conversation.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::capability::{
    CapabilityControl, CapabilityGrants, CapabilityJob, CapabilityRegistration, CapabilityResult,
    NativeCapabilityRunner,
};

const TOOL_NAME: &str = "todo_write";
const CAPABILITY_ID: &str = "todo.write";
const STATUSES: [&str; 3] = ["pending", "in_progress", "done"];

pub fn write_registration() -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: CAPABILITY_ID.to_string(),
        tool_name: TOOL_NAME.to_string(),
        description: "Replace the session's visible task list. Provide the full list of todos on \
            each call; each todo is an object with `content` and a `status` of pending, \
            in_progress or done. The user sees the list while you work."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["todos"],
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "The complete task list; an empty list clears it.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {"type": "string"},
                            "status": {"type": "string", "enum": ["pending", "in_progress", "done"]}
                        },
                        "required": ["content", "status"]
                    }
                }
            }
        }),
        metadata: serde_json::json!({
            "risk_level": "low", "requires_approval": false,
            "category": "planning", "capabilities": [CAPABILITY_ID]
        }),
        // The list lives inside the conversation; nothing outside changes.
        side_effect: crate::skills::SideEffectClass::None,
        grants: CapabilityGrants::default(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(write_job)),
    }
}

/// Normalize one call into the canonical list. `items` stays accepted for
/// histories that predate the rename, `completed` aliases `done`, and a bare
/// string entry becomes a pending todo.
fn write_job(job: &CapabilityJob, _control: &CapabilityControl) -> CapabilityResult {
    let raw = job
        .arguments
        .get("todos")
        .or_else(|| job.arguments.get("items"))
        .and_then(Value::as_array);
    let Some(entries) = raw else {
        return CapabilityResult::failed(
            &job.job_id,
            "todos is required: an array of {content, status}",
            Some("arguments"),
        );
    };
    let todos: Vec<Value> = entries
        .iter()
        .map(|entry| {
            if let Some(object) = entry.as_object() {
                let content = object
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                let status = object
                    .get("status")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    // `completed` is the common model alias for our `done`.
                    .map(|status| {
                        if status == "completed" {
                            "done"
                        } else {
                            status
                        }
                    })
                    .unwrap_or("pending");
                let status = if STATUSES.contains(&status) {
                    status
                } else {
                    "pending"
                };
                json!({"content": content, "status": status})
            } else {
                json!({"content": entry.as_str().unwrap_or_default().trim(), "status": "pending"})
            }
        })
        .collect();
    CapabilityResult::completed(
        &job.job_id,
        json!({"ok": true, "count": todos.len(), "todos": todos}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilityExitState, CapabilityProgress};
    use std::sync::atomic::AtomicBool;

    fn job(arguments: Value) -> CapabilityJob {
        CapabilityJob::new(CAPABILITY_ID, "job-todo-test", arguments)
    }

    fn control() -> CapabilityControl {
        let progress: Arc<dyn Fn(CapabilityProgress) + Send + Sync> = Arc::new(|_| {});
        CapabilityControl::new(Arc::new(AtomicBool::new(false)), progress)
    }

    fn todos_of(arguments: Value) -> Vec<Value> {
        write_job(&job(arguments), &control())
            .result
            .expect("completed result carries output")["todos"]
            .as_array()
            .expect("todos array")
            .clone()
    }

    #[test]
    fn a_call_normalizes_statuses_and_entries() {
        let todos = todos_of(json!({"todos": [
            {"content": "Ship it", "status": "in_progress"},
            {"content": "Done thing", "status": "completed"},
            {"content": "Odd", "status": "weird"},
            {"content": "No status"},
            "a bare string",
            {"content": "  padded  "}
        ]}));
        assert_eq!(todos[0]["status"], "in_progress");
        assert_eq!(todos[1]["status"], "done", "`completed` aliases `done`");
        assert_eq!(
            todos[2]["status"], "pending",
            "an unknown status falls back"
        );
        assert_eq!(todos[3]["status"], "pending");
        assert_eq!(todos[4]["content"], "a bare string");
        assert_eq!(todos[4]["status"], "pending");
        assert_eq!(todos[5]["content"], "padded");
    }

    #[test]
    fn the_legacy_items_key_still_works() {
        let todos = todos_of(json!({"items": [{"content": "Old plan", "status": "pending"}]}));
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0]["content"], "Old plan");
    }

    #[test]
    fn an_empty_list_clears_the_plan() {
        let output = write_job(&job(json!({"todos": []})), &control())
            .result
            .expect("completed");
        assert_eq!(output["count"], json!(0));
        assert_eq!(output["todos"], json!([]));
    }

    #[test]
    fn a_missing_list_is_an_argument_error() {
        let result = write_job(&job(json!({})), &control());
        assert_eq!(result.state, CapabilityExitState::Failed);
        let diagnostics = result.diagnostics.expect("failure carries diagnostics");
        assert_eq!(diagnostics.error_code.as_deref(), Some("arguments"));
    }

    #[test]
    fn the_registration_is_low_risk_and_auto_approved() {
        let registration = write_registration();
        assert_eq!(registration.tool_name, TOOL_NAME);
        assert_eq!(registration.metadata["risk_level"], json!("low"));
        assert_eq!(registration.metadata["requires_approval"], json!(false));
        assert!(!registration.grants.exec);
        assert!(!registration.workspace_write);
    }
}
