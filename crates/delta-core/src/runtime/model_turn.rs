//! Model-turn execution and provider streaming for RuntimeHost.
//!
//! The parent runtime owns run lifecycle, authorities and recovery. This module
//! owns the per-turn provider request/stream/assistant continuation loop.

use super::*;

impl RuntimeHost {
    pub(super) fn outbound_messages(&self) -> Vec<Value> {
        let sidecars = [
            "source",
            "_display",
            "ts",
            "reasoning",
            "usage",
            "attachments",
            "run_id",
        ];
        self.messages
            .iter()
            .filter(|m| m.get("role").and_then(|r| r.as_str()) != Some("notice"))
            .map(|m| {
                let has_sidecar = sidecars.iter().any(|s| m.get(*s).is_some());
                let mut message = if has_sidecar {
                    let mut out = serde_json::Map::new();
                    if let Some(obj) = m.as_object() {
                        for (k, v) in obj {
                            if !sidecars.contains(&k.as_str()) {
                                out.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    Value::Object(out)
                } else {
                    m.clone()
                };
                if m.get("role").and_then(Value::as_str) == Some("user") {
                    if let Some(attachments) = m.get("attachments").and_then(Value::as_array) {
                        if !attachments.is_empty() {
                            message["content"] = provider_user_content(
                                &self.config.protocol,
                                m.get("content").and_then(Value::as_str).unwrap_or_default(),
                                attachments,
                            );
                        }
                    }
                }
                message
            })
            .collect()
    }

    pub(super) fn build_request(&self) -> ProviderRequest {
        ProviderRequest {
            protocol: self.config.protocol.clone(),
            model: self.config.model.clone(),
            messages: Value::Array(self.outbound_messages()),
            tools: self.tools.clone(),
            settings: Some(self.config.model_settings.clone()),
            api_key: self.config.api_key.clone(),
            base_url: self.config.base_url.clone(),
            timeout_secs: self.config.ttft_timeout,
        }
    }

    pub(super) fn assistant_message(
        &self,
        text: Option<&str>,
        reasoning: Option<&str>,
        tool_calls: &[ToolCall],
    ) -> Value {
        let mut msg = json!({"role": "assistant", "ts": now_ts(), "model": self.config.model});
        if let Some(t) = text {
            msg["content"] = Value::String(t.to_string());
        }
        if let Some(r) = reasoning {
            msg["reasoning"] = Value::String(r.to_string());
        }
        if !tool_calls.is_empty() {
            let tc_json: Vec<Value> = tool_calls
                .iter()
                .map(|tc| {
                    json!({
                        "id": &tc.id,
                        "type": "function",
                        "function": {
                            "name": &tc.name,
                            "arguments": serde_json::to_string(&tc.arguments).unwrap_or_default(),
                        }
                    })
                })
                .collect();
            msg["tool_calls"] = Value::Array(tc_json);
        }
        msg
    }

    pub(super) fn drain_steering(&mut self) -> bool {
        let mut q = self.steering.lock().unwrap();
        if q.is_empty() {
            return false;
        }
        for (text, source) in q.iter() {
            let mut message = json!({"role": "user", "content": text, "ts": now_ts()});
            if let Some(src) = source {
                message["source"] = src.clone();
            }
            self.messages.push(message);
        }
        q.clear();
        true
    }

    pub(super) fn stream_provider(
        &self,
        req: &ProviderRequest,
        stream_id: &str,
        turn: &mut Option<AssistantTurn>,
        streamed_text: &mut Vec<String>,
        streamed_reasoning: &mut Vec<String>,
    ) -> Result<ProviderStreamOutcome, String> {
        let mut writer = ProviderEventWriter::new(self.event_emitter());
        let result = provider::stream(req, &mut writer, stream_id, &self.provider_cancel);
        let (partial_text, partial_reasoning) = writer.finish();
        if !partial_text.is_empty() {
            streamed_text.push(partial_text);
        }
        if !partial_reasoning.is_empty() {
            streamed_reasoning.push(partial_reasoning);
        }
        let result = result?;
        if result
            .get("cancelled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return Ok(ProviderStreamOutcome::Interrupted);
        }
        let text = result
            .get("text")
            .and_then(|t| t.as_str())
            .map(String::from);
        let reasoning = result
            .get("reasoning")
            .and_then(|r| r.as_str())
            .map(String::from);
        let finish_reason = result
            .get("finish_reason")
            .and_then(|f| f.as_str())
            .map(String::from);
        let usage = result.get("usage").cloned();
        let tool_calls: Vec<ToolCall> = result
            .get("tool_calls")
            .and_then(|tcs| tcs.as_array())
            .map(|tcs| {
                tcs.iter()
                    .map(|tc| ToolCall {
                        id: tc
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        name: tc
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        arguments: tc.get("arguments").cloned().unwrap_or(json!({})),
                    })
                    .collect()
            })
            .unwrap_or_default();
        *turn = Some(AssistantTurn {
            text,
            reasoning,
            tool_calls,
            finish_reason,
            usage,
        });
        Ok(ProviderStreamOutcome::Completed)
    }

    pub(super) fn loop_turn(&mut self) -> Result<String, String> {
        let mut iterations = 0usize;
        let mut turn_retries = 0u32;
        loop {
            self.ensure_event_delivery()?;
            if iterations >= self.config.max_iterations {
                self.emit_event(RuntimeEvent::TurnEnd {
                    status: "max_iterations_exceeded".to_string(),
                    iterations,
                });
                self.ensure_event_delivery()?;
                return Ok("max_iterations_exceeded".to_string());
            }
            iterations += 1;
            if self.cancel.load(Ordering::Relaxed) {
                self.messages
                    .push(json!({"role": "notice", "kind": "interrupted", "ts": now_ts()}));
                self.emit_event(RuntimeEvent::Interrupted { iterations });
                self.ensure_event_delivery()?;
                return Ok("interrupted".to_string());
            }
            // Steering received before the request begins is incorporated in
            // this request; steering received after this reset flips the
            // provider-only token and interrupts the live stream.
            self.provider_cancel.store(false, Ordering::SeqCst);
            self.drain_steering();
            let req = self.build_request();
            let mut turn: Option<AssistantTurn> = None;
            let mut streamed_text: Vec<String> = Vec::new();
            let mut streamed_reasoning: Vec<String> = Vec::new();
            let stream_id = uuid_v4();
            match self.stream_provider(
                &req,
                &stream_id,
                &mut turn,
                &mut streamed_text,
                &mut streamed_reasoning,
            ) {
                Ok(ProviderStreamOutcome::Completed) => {
                    self.ensure_event_delivery()?;
                }
                Ok(ProviderStreamOutcome::Interrupted) => {
                    self.ensure_event_delivery()?;
                    if !streamed_text.is_empty() || !streamed_reasoning.is_empty() {
                        let partial_text =
                            (!streamed_text.is_empty()).then(|| streamed_text.join(""));
                        let partial_reasoning =
                            (!streamed_reasoning.is_empty()).then(|| streamed_reasoning.join(""));
                        let partial = self.assistant_message(
                            partial_text.as_deref(),
                            partial_reasoning.as_deref(),
                            &[],
                        );
                        self.messages.push(partial.clone());
                        self.emit_event(RuntimeEvent::AssistantMessage {
                            message: partial,
                            text: partial_text,
                            tool_calls: Vec::new(),
                            reasoning: partial_reasoning,
                            usage: None,
                        });
                    }
                    if self.cancel.load(Ordering::Relaxed) {
                        self.messages
                            .push(json!({"role": "notice", "kind": "interrupted", "ts": now_ts()}));
                        self.emit_event(RuntimeEvent::Interrupted { iterations });
                        self.ensure_event_delivery()?;
                        return Ok("interrupted".to_string());
                    }
                    if self.drain_steering() {
                        continue;
                    }
                    continue;
                }
                Err(e) => {
                    self.ensure_event_delivery()?;
                    if turn_retries < self.config.max_retries
                        && streamed_text.is_empty()
                        && streamed_reasoning.is_empty()
                        && !self.cancel.load(Ordering::Relaxed)
                        && is_retryable_error(&e)
                    {
                        turn_retries += 1;
                        self.messages.push(json!({
                            "role": "notice", "kind": "retrying",
                            "text": format!("Retrying (attempt {turn_retries}/{})", self.config.max_retries),
                            "ts": now_ts()
                        }));
                        self.emit_event(RuntimeEvent::Error {
                            error: "Transient model failure - retrying.".to_string(),
                            error_type: classify_transient_error(&e),
                            terminal: false,
                        });
                        continue;
                    }
                    if !streamed_text.is_empty() || !streamed_reasoning.is_empty() {
                        let partial_text = streamed_text.join("");
                        let partial_reasoning = streamed_reasoning.join("");
                        let partial = self.assistant_message(
                            Some(&partial_text),
                            Some(&partial_reasoning),
                            &[],
                        );
                        self.messages.push(partial.clone());
                        self.emit_event(RuntimeEvent::AssistantMessage {
                            message: partial,
                            text: Some(partial_text),
                            tool_calls: Vec::new(),
                            reasoning: Some(partial_reasoning),
                            usage: None,
                        });
                    }
                    self.messages.push(
                        json!({"role": "notice", "kind": "error", "text": &e, "ts": now_ts()}),
                    );
                    self.emit_event(RuntimeEvent::Error {
                        error: e.clone(),
                        error_type: classify_transient_error(&e),
                        terminal: true,
                    });
                    return Err(e);
                }
            }
            if self.cancel.load(Ordering::Relaxed) && turn.is_none() {
                if !streamed_text.is_empty() || !streamed_reasoning.is_empty() {
                    let partial_text = streamed_text.join("");
                    let partial_reasoning = streamed_reasoning.join("");
                    let partial =
                        self.assistant_message(Some(&partial_text), Some(&partial_reasoning), &[]);
                    self.messages.push(partial.clone());
                    self.emit_event(RuntimeEvent::AssistantMessage {
                        message: partial,
                        text: Some(partial_text),
                        tool_calls: Vec::new(),
                        reasoning: Some(partial_reasoning),
                        usage: None,
                    });
                }
                self.messages
                    .push(json!({"role": "notice", "kind": "interrupted", "ts": now_ts()}));
                self.emit_event(RuntimeEvent::Interrupted { iterations });
                self.ensure_event_delivery()?;
                return Ok("interrupted".to_string());
            }
            let turn = turn.unwrap_or_default();
            let assistant_message = self.assistant_message(
                turn.text.as_deref(),
                turn.reasoning.as_deref(),
                &turn.tool_calls,
            );
            self.messages.push(assistant_message.clone());
            let tool_call_names: Vec<String> =
                turn.tool_calls.iter().map(|tc| tc.name.clone()).collect();
            self.emit_event(RuntimeEvent::AssistantMessage {
                message: assistant_message,
                text: turn.text.clone(),
                tool_calls: tool_call_names,
                reasoning: turn.reasoning.clone(),
                usage: turn.usage.clone(),
            });
            self.ensure_event_delivery()?;
            if turn.tool_calls.is_empty() {
                if self.drain_steering() {
                    continue;
                }
                self.emit_event(RuntimeEvent::TurnEnd {
                    status: "completed".to_string(),
                    iterations,
                });
                self.ensure_event_delivery()?;
                return Ok("completed".to_string());
            }
            for tc in &turn.tool_calls {
                if self.cancel.load(Ordering::Relaxed) {
                    break;
                }
                let result = self.execute_tool_call(tc);
                self.messages.push(json!({
                    "role": "tool", "tool_call_id": &tc.id, "content": &result.output
                }));
                self.emit_event(RuntimeEvent::ToolFinished {
                    tool_call_id: tc.id.clone(),
                    name: tc.name.clone(),
                    result: result.output.clone(),
                    error: result.error.clone(),
                });
                self.ensure_event_delivery()?;
            }
            self.emit_event(RuntimeEvent::IterationEnd {
                iteration: iterations,
            });
            self.ensure_event_delivery()?;

            if self.cancel.load(Ordering::Relaxed) {
                self.messages
                    .push(json!({"role": "notice", "kind": "interrupted", "ts": now_ts()}));
                self.emit_event(RuntimeEvent::Interrupted { iterations });
                self.ensure_event_delivery()?;
                return Ok("interrupted".to_string());
            }
            self.drain_steering();
        }
    }
}
