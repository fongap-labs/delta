//! Trusted tool execution pipeline for the Rust Runtime.
//!
//! This module owns approval resolution, artifact finalization, validation and
//! side-effect execution sequencing. The parent runtime keeps turn orchestration
//! and delegates tool calls through this boundary.

use super::*;
use std::path::{Component, Path};

use sha2::{Digest, Sha256};

use crate::approval::ApprovalRecordInput;
use crate::artifact::{ArtifactInput, ArtifactRegistryWriter};
use crate::tool_lifecycle::{self, PlanAction, ToolLifecyclePlanInput};
use crate::validation::ValidationWriter;

impl RuntimeHost {
    fn record_approval(
        &self,
        call: &ToolCall,
        stage: &str,
        status: &str,
        approval: Option<&str>,
        reason: &str,
        level: RiskLevel,
    ) -> Result<(), String> {
        let Some(authorities) = &self.authorities else {
            return Ok(());
        };
        let workspace = self.config.workspace.clone().unwrap_or_default();
        authorities
            .approvals
            .lock()
            .unwrap()
            .record(
                ApprovalRecordInput {
                    session_id: self.session_id.clone(),
                    agent: Some("delta".to_string()),
                    workspace: Some(workspace.clone()),
                    connector: None,
                    tool: call.name.clone(),
                    stage: stage.to_string(),
                    status: Some(status.to_string()),
                    approval: approval.map(str::to_string),
                    arguments: Some(call.arguments.clone()),
                    result_preview: None,
                    reason: Some(reason.to_string()),
                    resource: None,
                    level: Some(format!("{level:?}")),
                    isolation: Some("runtime".to_string()),
                    ts: Some(now_ts()),
                },
                now_ts(),
                &workspace,
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn require_approval(
        &self,
        call: &ToolCall,
        reason: &str,
        level: RiskLevel,
    ) -> Result<ApprovalDecision, String> {
        self.emit_event(RuntimeEvent::PermissionRequired {
            tool_call_id: call.id.clone(),
            name: call.name.clone(),
            arguments: call.arguments.clone(),
            reason: reason.to_string(),
        });
        self.ensure_event_delivery()?;
        self.ledger_append(
            "approval.required",
            "runtime",
            json!({"tool_call_id": call.id, "tool": call.name, "reason": reason, "level": format!("{level:?}")}),
        )?;
        self.record_approval(call, "approval_required", "pending", None, reason, level)?;
        let pending_inbox_item_id = if self.config.unattended {
            Some(
                self.authorities
                    .as_ref()
                    .ok_or_else(|| "inbox authority is unavailable".to_string())?
                    .inbox
                    .add_approval(
                        &self.session_id,
                        &call.id,
                        &call.name,
                        &call.arguments,
                        reason,
                    )
                    .map_err(|error| error.to_string())?
                    .id,
            )
        } else {
            None
        };
        self.register_checkpoint(
            "awaiting_approval",
            Some(json!({"id": call.id, "name": call.name, "arguments": call.arguments})),
            pending_inbox_item_id,
            Vec::new(),
            None,
        )?;
        if self.config.unattended {
            self.ledger_append(
                "approval.inbox",
                "runtime",
                json!({"tool_call_id": call.id, "tool": call.name}),
            )?;
            self.record_approval(call, "approval_parked", "inbox", None, reason, level)?;
        }

        let receiver = self.approvals.begin(&call.id)?;
        self.set_runtime_state(if self.config.unattended {
            RuntimeState::WaitingUser
        } else {
            RuntimeState::WaitingApproval
        });
        loop {
            if self.cancel.load(Ordering::Acquire) {
                self.approvals.cancel(&call.id);
                self.record_approval(
                    call,
                    "approval_resolved",
                    "cancelled",
                    None,
                    "run cancelled while approval was pending",
                    level,
                )?;
                self.ledger_append(
                    "approval.cancelled",
                    "user",
                    json!({"tool_call_id": call.id, "tool": call.name}),
                )?;
                return Err("run cancelled while approval was pending".to_string());
            }
            match receiver.recv_timeout(Duration::from_millis(25)) {
                Ok(decision) => {
                    self.set_runtime_state(RuntimeState::Running);
                    let status = if decision.is_approved() {
                        "approved"
                    } else {
                        "denied"
                    };
                    self.record_approval(
                        call,
                        "approval_resolved",
                        status,
                        Some(decision.as_str()),
                        reason,
                        level,
                    )?;
                    self.ledger_append(
                        if decision.is_approved() {
                            "approval.approved"
                        } else {
                            "approval.denied"
                        },
                        "user",
                        json!({"tool_call_id": call.id, "tool": call.name, "decision": decision.as_str()}),
                    )?;
                    if decision.is_approved() {
                        self.approvals
                            .remember(decision, &call.name, &call.arguments);
                    }
                    return Ok(decision);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("approval request was abandoned".to_string());
                }
            }
        }
    }

    fn formalize_artifacts(&self, staged: &[StagedArtifact]) -> Result<Vec<Value>, String> {
        if staged.is_empty() {
            return Ok(Vec::new());
        }
        let authorities = self
            .authorities
            .as_ref()
            .ok_or_else(|| "artifact authority is unavailable".to_string())?;
        let workspace_text = self
            .config
            .workspace
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "workspace is required for artifacts".to_string())?;
        let workspace = PathBuf::from(workspace_text)
            .canonicalize()
            .map_err(|error| format!("workspace is unavailable: {error}"))?;
        let run_id = self.run_id.clone().unwrap_or_else(uuid_v4);
        let staging_root = workspace.join(".delta").join("staging").join(&run_id);
        let mut formalized = Vec::new();

        for candidate in staged {
            let source = candidate
                .staging_path
                .canonicalize()
                .map_err(|error| format!("staged artifact is unavailable: {error}"))?;
            let canonical_staging = staging_root
                .canonicalize()
                .map_err(|error| format!("staging root is unavailable: {error}"))?;
            if !source.starts_with(&canonical_staging) || !source.is_file() {
                return Err("worker artifact escaped its staging root".to_string());
            }
            let relative = Path::new(&candidate.relative_path);
            if relative.is_absolute()
                || relative.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                return Err("artifact destination must be workspace-relative".to_string());
            }
            let target = workspace.join(relative);
            if !target.starts_with(&workspace) || target.starts_with(workspace.join(".delta")) {
                return Err("artifact destination is reserved or outside workspace".to_string());
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            if target.exists() {
                fs::remove_file(&target).map_err(|error| error.to_string())?;
            }
            if fs::rename(&source, &target).is_err() {
                fs::copy(&source, &target).map_err(|error| error.to_string())?;
                fs::remove_file(&source).map_err(|error| error.to_string())?;
            }
            let bytes = fs::read(&target).map_err(|error| error.to_string())?;
            let sha256 = format!("{:x}", Sha256::digest(&bytes));
            let relative_path = candidate.relative_path.replace('\\', "/");
            let artifact = ArtifactInput {
                path: relative_path.clone(),
                name: target
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or(&relative_path)
                    .to_string(),
                kind: candidate.kind.clone(),
                size: bytes.len() as i64,
                modified_at: now_ts(),
                run_id: run_id.clone(),
                sha256: sha256.clone(),
                incomplete: candidate.incomplete,
                registered_at: now_ts(),
            };
            ArtifactRegistryWriter::new(&authorities.ledger.lock().unwrap())
                .register(&artifact, now_ts(), workspace_text)
                .map_err(|error| error.to_string())?;
            formalized.push(artifact.registered_payload());
        }
        Ok(formalized)
    }

    fn validate_tool_artifacts(
        &self,
        artifacts: &[Value],
        criteria: Option<&Value>,
    ) -> Result<Value, String> {
        let Some(authorities) = &self.authorities else {
            if artifacts.is_empty() {
                return Ok(json!({"ok": true, "checks": []}));
            }
            return Err("validation authority is unavailable".to_string());
        };
        let run_id = self.run_id.clone().unwrap_or_else(uuid_v4);
        let workspace = self.config.workspace.clone().unwrap_or_default();
        let default_criteria = json!({
            "min_artifacts": 0,
            "max_artifacts": 50,
            "require_complete": true
        });
        let ledger = authorities.ledger.lock().unwrap();
        let (_, result) = ValidationWriter::new(&ledger)
            .evaluate_and_register(
                &run_id,
                criteria.unwrap_or(&default_criteria),
                artifacts,
                &workspace,
                None,
                now_ts(),
            )
            .map_err(|error| error.to_string())?;
        serde_json::to_value(result).map_err(|error| error.to_string())
    }

    pub(super) fn execute_tool_call(&self, call: &ToolCall) -> ToolResult {
        let (_schema, metadata) = match self.tool_contract(call) {
            Ok(contract) => contract,
            Err(error) => {
                return self.fail_after_ledger(
                call,
                "tool.failed",
                "runtime",
                json!({"tool_call_id": call.id, "tool": call.name, "stage": "schema", "error": &error}),
                error,
            );
            }
        };
        let (level, decision) = match self.policy_for(call, metadata) {
            Ok(outcome) => outcome,
            Err(error) => return ToolResult::failure(&call.id, error),
        };
        self.emit_event(RuntimeEvent::ToolProposed {
            tool_call_id: call.id.clone(),
            name: call.name.clone(),
            arguments: call.arguments.clone(),
            risk_level: Some(format!("{level:?}")),
        });
        if let Err(error) = self.ensure_event_delivery() {
            return ToolResult::failure(&call.id, error);
        }
        if let Err(error) = self.ledger_append(
        "tool.proposed",
        "model",
        json!({"tool_call_id": call.id, "tool": call.name, "arguments": call.arguments, "level": format!("{level:?}")}),
    ) {
        return ToolResult::failure(&call.id, error);
    }

        if !decision.allowed
            && self
                .approvals
                .has_standing_grant(&call.name, &call.arguments)
        {
            if let Err(error) = self.record_approval(
                call,
                "standing_policy_resolved",
                "auto_approved",
                Some("standing"),
                "approved by a session-scoped standing grant",
                level,
            ) {
                return ToolResult::failure(&call.id, error);
            }
        } else if !decision.allowed {
            if !decision.needs_user {
                if let Err(error) = self.record_approval(
                    call,
                    "policy_denied",
                    "denied",
                    None,
                    &decision.reason,
                    level,
                ) {
                    return ToolResult::failure(
                        &call.id,
                        format!(
                            "{}; failed to persist policy denial: {error}",
                            decision.reason
                        ),
                    );
                }
                return self.fail_after_ledger(
                call,
                "tool.failed",
                "runtime",
                json!({"tool_call_id": call.id, "tool": call.name, "stage": "policy", "error": &decision.reason}),
                decision.reason,
            );
            }
            match self.require_approval(call, &decision.reason, level) {
                Ok(approval) if approval.is_approved() => {}
                Ok(_) => {
                    return self.fail_after_ledger(
                        call,
                        "tool.cancelled",
                        "user",
                        json!({"tool_call_id": call.id, "tool": call.name, "reason": "denied"}),
                        "tool call denied by user",
                    );
                }
                Err(error) => {
                    return self.fail_after_ledger(
                        call,
                        "tool.cancelled",
                        "runtime",
                        json!({"tool_call_id": call.id, "tool": call.name, "reason": &error}),
                        error,
                    );
                }
            }
        } else if let Err(error) = self.record_approval(
            call,
            "policy_resolved",
            "auto_approved",
            Some("policy"),
            &decision.reason,
            level,
        ) {
            return ToolResult::failure(&call.id, error);
        }
        if let Err(error) = self.ledger_append(
            "tool.approved",
            "runtime",
            json!({"tool_call_id": call.id, "tool": call.name, "level": format!("{level:?}")}),
        ) {
            return ToolResult::failure(&call.id, error);
        }

        if let Some(result) = self.execute_interaction(call) {
            return result;
        }

        let Some(authorities) = &self.authorities else {
            return ToolResult::failure(&call.id, "runtime authorities are unavailable");
        };
        let run_id = self.run_id.clone().unwrap_or_else(uuid_v4);
        let plan_input = ToolLifecyclePlanInput {
            db: "side_effects.db".to_string(),
            run_id: run_id.clone(),
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            args: call.arguments.clone(),
        };
        let plan = match tool_lifecycle::plan(&authorities.idempotency.lock().unwrap(), &plan_input)
        {
            Ok(plan) => plan,
            Err(error) => return ToolResult::failure(&call.id, error.to_string()),
        };
        match plan.action {
            PlanAction::Replay => {
                let output = plan.result.unwrap_or_else(|| json!({"ok": true}));
                if let Err(error) = self.ledger_append(
                    "tool.replayed",
                    "runtime",
                    json!({"tool_call_id": call.id, "tool": call.name}),
                ) {
                    return ToolResult::failure(
                    &call.id,
                    format!("committed result exists but tool.replayed could not be persisted: {error}"),
                );
                }
                return ToolResult::success(&call.id, output);
            }
            PlanAction::Uncertain => {
                let uncertainty = plan
                    .error
                    .as_ref()
                    .and_then(|value| value.get("error"))
                    .and_then(Value::as_str)
                    .unwrap_or("previous tool result is uncertain")
                    .to_string();
                if let Err(error) = self.ledger_append(
                "tool.uncertain",
                "runtime",
                json!({"tool_call_id": call.id, "tool": call.name, "operation_id": plan.operation_id}),
            ) {
                return ToolResult::failure(
                    &call.id,
                    format!("{uncertainty}; failed to persist tool.uncertain: {error}"),
                );
            }
                return ToolResult::failure(&call.id, uncertainty);
            }
            PlanAction::Execute => {}
        }

        self.emit_event(RuntimeEvent::ToolStarted {
            tool_call_id: call.id.clone(),
            name: call.name.clone(),
        });
        if let Err(error) = self.ensure_event_delivery() {
            return ToolResult::failure(&call.id, error);
        }
        if let Err(error) = self.ledger_append(
            "tool.started",
            "runtime",
            json!({"tool_call_id": call.id, "tool": call.name}),
        ) {
            return ToolResult::failure(&call.id, error);
        }

        let progress_error = Arc::new(Mutex::new(None::<String>));
        let progress_error_sink = progress_error.clone();
        let progress_authorities = authorities.clone();
        let progress_run_id = run_id.clone();
        let progress_workspace = self.config.workspace.clone().unwrap_or_default();
        let progress_tool_call_id = call.id.clone();
        let progress_tool = call.name.clone();
        let progress = Arc::new(move |frame: CapabilityProgress| {
            let write_result = progress_authorities.ledger.lock().unwrap().append(
                &progress_run_id,
                "tool.progress",
                "worker",
                now_ts(),
                &json!({
                    "tool_call_id": progress_tool_call_id,
                    "tool": progress_tool,
                    "job_id": frame.job_id,
                    "fraction": frame.fraction,
                    "stage": frame.stage,
                    "message": frame.message,
                }),
                &progress_workspace,
            );
            if let Err(error) = write_result {
                let mut current = progress_error_sink.lock().unwrap();
                if current.is_none() {
                    *current = Some(error.to_string());
                }
            }
        });
        let context = ToolExecutionContext {
            session_id: self.session_id.clone(),
            run_id: run_id.clone(),
            workspace: self.config.workspace.clone(),
            timeout: Duration::from_secs_f64(self.config.tool_timeout.unwrap_or(120.0).max(0.1)),
            cancel: self.cancel.clone(),
            progress,
            secrets: BTreeMap::new(),
        };
        let mut result = self.tool_executor.execute(call, &context);

        if let Some(progress_error) = progress_error.lock().unwrap().clone() {
            let primary = format!(
            "tool progress ledger persistence failed after execution started; outcome is uncertain: {progress_error}"
        );
            if let Err(error) = self.mark_tool_uncertain(
                authorities,
                &run_id,
                call,
                "tool.uncertain",
                "runtime",
                json!({
                    "tool_call_id": call.id,
                    "tool": call.name,
                    "reason": "progress_persistence_failed",
                    "error": &progress_error
                }),
            ) {
                return ToolResult::failure(&call.id, format!("{primary}; {error}"));
            }
            return ToolResult::failure(&call.id, primary);
        }

        if self.cancel.load(Ordering::Acquire) {
            let primary = "tool cancelled; side effect is uncertain";
            if let Err(error) = self.mark_tool_uncertain(
                authorities,
                &run_id,
                call,
                "tool.cancelled",
                "user",
                json!({"tool_call_id": call.id, "tool": call.name, "state": "uncertain"}),
            ) {
                return ToolResult::failure(&call.id, format!("{primary}; {error}"));
            }
            return ToolResult::failure(&call.id, primary);
        }

        if let Some(error) = result.error.clone() {
            let event_type = match result.state {
                ToolExitState::TimedOut => "tool.timed_out",
                ToolExitState::Cancelled => "tool.cancelled",
                ToolExitState::Completed | ToolExitState::Failed => "tool.failed",
            };
            let payload = json!({
                "tool_call_id": call.id,
                "tool": call.name,
                "error": &error,
                "state": format!("{:?}", result.state).to_lowercase()
            });
            let persisted = if matches!(
                result.state,
                ToolExitState::TimedOut | ToolExitState::Cancelled
            ) {
                self.mark_tool_uncertain(authorities, &run_id, call, event_type, "runtime", payload)
            } else {
                self.mark_tool_failed(
                    authorities,
                    &run_id,
                    call,
                    &error,
                    (event_type, "runtime", payload),
                )
            };
            if let Err(persistence_error) = persisted {
                return ToolResult::failure(&call.id, format!("{error}; {persistence_error}"));
            }
            return result;
        }

        let artifacts = match self.formalize_artifacts(&result.staged_artifacts) {
            Ok(artifacts) => artifacts,
            Err(error) => {
                let primary = format!(
                "{error}; capability execution completed but artifact finalization did not reach a trusted terminal state"
            );
                if let Err(persistence_error) = self.mark_tool_uncertain(
                authorities,
                &run_id,
                call,
                "tool.uncertain",
                "runtime",
                json!({"tool_call_id": call.id, "tool": call.name, "stage": "artifact", "error": &error}),
            ) {
                return ToolResult::failure(
                    &call.id,
                    format!("{primary}; {persistence_error}"),
                );
            }
                return ToolResult::failure(&call.id, primary);
            }
        };
        let validation = match self
            .validate_tool_artifacts(&artifacts, result.validation_criteria.as_ref())
        {
            Ok(validation) if validation.get("ok").and_then(Value::as_bool) != Some(false) => {
                validation
            }
            Ok(validation) => {
                let error = "tool artifact validation failed".to_string();
                let primary = format!(
                "{error}; capability execution completed but validation did not reach a trusted terminal state"
            );
                if let Err(persistence_error) = self.mark_tool_uncertain(
                authorities,
                &run_id,
                call,
                "tool.uncertain",
                "runtime",
                json!({"tool_call_id": call.id, "tool": call.name, "stage": "validation", "error": &error, "validation": validation}),
            ) {
                return ToolResult::failure(
                    &call.id,
                    format!("{primary}; {persistence_error}"),
                );
            }
                return ToolResult::failure(&call.id, primary);
            }
            Err(error) => {
                let primary = format!(
                "{error}; capability execution completed but validation persistence did not reach a trusted terminal state"
            );
                if let Err(persistence_error) = self.mark_tool_uncertain(
                authorities,
                &run_id,
                call,
                "tool.uncertain",
                "runtime",
                json!({"tool_call_id": call.id, "tool": call.name, "stage": "validation", "error": &error}),
            ) {
                return ToolResult::failure(
                    &call.id,
                    format!("{primary}; {persistence_error}"),
                );
            }
                return ToolResult::failure(&call.id, primary);
            }
        };
        if let Some(output) = result.output.as_object_mut() {
            if !artifacts.is_empty() {
                output.insert("artifacts".to_string(), Value::Array(artifacts.clone()));
            }
            output.insert("validation".to_string(), validation);
        }
        if let Err(error) = authorities.idempotency.lock().unwrap().commit(
            &run_id,
            &call.id,
            &call.name,
            &call.arguments,
            &result.output,
        ) {
            let primary = format!(
            "tool result commit failed after capability execution; outcome is uncertain: {error}"
        );
            if let Err(persistence_error) = self.mark_tool_uncertain(
            authorities,
            &run_id,
            call,
            "tool.uncertain",
            "runtime",
            json!({"tool_call_id": call.id, "tool": call.name, "reason": "idempotency_commit_failed", "error": error.to_string()}),
        ) {
            return ToolResult::failure(
                &call.id,
                format!("{primary}; {persistence_error}"),
            );
        }
            return ToolResult::failure(&call.id, primary);
        }
        if let Err(error) = self.ledger_append(
            "tool.completed",
            "runtime",
            json!({"tool_call_id": call.id, "tool": call.name, "result": result.output}),
        ) {
            return ToolResult::failure(&call.id, error);
        }
        if let Err(error) = self.register_checkpoint("tool_completed", None, None, artifacts, None)
        {
            return ToolResult::failure(&call.id, error);
        }
        result
    }
}
