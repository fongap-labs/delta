//! User interaction coordination for the Runtime.
//!
//! Owns typed interaction requests, response delivery, unattended inbox
//! persistence, waiting state, cancellation, and interaction ledger events.

use super::*;

#[derive(Default)]
pub(super) struct InteractionController {
    pending: Mutex<HashMap<String, (String, mpsc::Sender<Value>)>>,
}

impl InteractionController {
    pub(super) fn begin(&self, id: &str, kind: &str) -> Result<mpsc::Receiver<Value>, String> {
        let (sender, receiver) = mpsc::channel();
        let mut pending = self.pending.lock().unwrap();
        if pending.contains_key(id) {
            return Err(format!("interaction is already pending: {id}"));
        }
        pending.insert(id.to_string(), (kind.to_string(), sender));
        Ok(receiver)
    }

    pub(super) fn resolve(
        &self,
        kind: &str,
        id: Option<&str>,
        value: Value,
    ) -> Result<String, String> {
        let mut pending = self.pending.lock().unwrap();
        let key = if let Some(id) = id {
            id.to_string()
        } else {
            pending
                .iter()
                .find(|(_, (pending_kind, _))| pending_kind == kind)
                .map(|(id, _)| id.clone())
                .ok_or_else(|| format!("no pending {kind} interaction"))?
        };
        let (pending_kind, sender) = pending
            .remove(&key)
            .ok_or_else(|| format!("interaction not found: {key}"))?;
        if pending_kind != kind {
            pending.insert(key.clone(), (pending_kind, sender));
            return Err(format!("interaction {key} is not a {kind} request"));
        }
        sender
            .send(value)
            .map_err(|_| format!("interaction {key} is no longer active"))?;
        Ok(key)
    }

    fn cancel(&self, id: &str) {
        self.pending.lock().unwrap().remove(id);
    }
}

impl RuntimeHost {
    pub(super) fn execute_interaction(&self, call: &ToolCall) -> Option<ToolResult> {
        let kind = match call.name.as_str() {
            "request_directory" => "directory",
            "ask_user" => "question",
            "propose_plan" => "plan",
            _ => return None,
        };
        let receiver = match self.interactions.begin(&call.id, kind) {
            Ok(receiver) => receiver,
            Err(error) => return Some(ToolResult::failure(&call.id, error)),
        };
        match kind {
            "directory" => self.emit_event(RuntimeEvent::DirectoryRequested {
                tool_call_id: call.id.clone(),
                reason: call
                    .arguments
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                path: call
                    .arguments
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                writable: call
                    .arguments
                    .get("writable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }),
            "question" => self.emit_event(RuntimeEvent::QuestionRequested {
                tool_call_id: call.id.clone(),
                arguments: call.arguments.clone(),
            }),
            "plan" => self.emit_event(RuntimeEvent::PlanProposed {
                tool_call_id: call.id.clone(),
                plan: call
                    .arguments
                    .get("plan")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            }),
            _ => unreachable!(),
        }
        if let Err(error) = self.ensure_event_delivery() {
            self.interactions.cancel(&call.id);
            return Some(ToolResult::failure(&call.id, error));
        }
        let inbox_id = if self.config.unattended {
            let Some(authorities) = self.authorities.as_ref() else {
                self.interactions.cancel(&call.id);
                return Some(ToolResult::failure(
                    &call.id,
                    "inbox authority is unavailable",
                ));
            };
            match authorities.inbox.add_interaction(
                &self.session_id,
                &call.id,
                kind,
                &call.arguments,
            ) {
                Ok(item) => Some(item.id),
                Err(error) => {
                    self.interactions.cancel(&call.id);
                    return Some(ToolResult::failure(
                        &call.id,
                        format!("failed to persist inbox interaction: {error}"),
                    ));
                }
            }
        } else {
            None
        };
        if let Err(error) = self.register_checkpoint(
            "awaiting_user",
            Some(json!({"id": call.id, "name": call.name, "arguments": call.arguments})),
            inbox_id,
            Vec::new(),
            None,
        ) {
            self.interactions.cancel(&call.id);
            return Some(ToolResult::failure(&call.id, error));
        }
        self.set_runtime_state(RuntimeState::WaitingUser);
        loop {
            if self.cancel.load(Ordering::Acquire) {
                self.interactions.cancel(&call.id);
                let primary = "run cancelled while user input was pending";
                if let Err(error) = self.ledger_append(
                    "interaction.cancelled",
                    "user",
                    json!({"tool_call_id": call.id, "kind": kind}),
                ) {
                    return Some(ToolResult::failure(
                        &call.id,
                        format!("{primary}; failed to persist interaction.cancelled: {error}"),
                    ));
                }
                return Some(ToolResult::failure(&call.id, primary));
            }
            match receiver.recv_timeout(Duration::from_millis(25)) {
                Ok(value) => {
                    self.set_runtime_state(RuntimeState::Running);
                    if let Err(error) = self.ledger_append(
                        "interaction.resolved",
                        "user",
                        json!({"tool_call_id": call.id, "kind": kind, "response": value.clone()}),
                    ) {
                        return Some(ToolResult::failure(
                            &call.id,
                            format!("failed to persist interaction.resolved: {error}"),
                        ));
                    }
                    return Some(ToolResult::success(&call.id, value));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Some(ToolResult::failure(
                        &call.id,
                        "user interaction was abandoned",
                    ));
                }
            }
        }
    }
}
