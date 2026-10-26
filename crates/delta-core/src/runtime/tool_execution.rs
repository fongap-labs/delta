//! Trusted tool execution pipeline for the Rust Runtime.
//!
//! This module owns approval resolution, artifact finalization, validation and
//! side-effect execution sequencing. The parent runtime keeps turn orchestration
//! and delegates tool calls through this boundary.

use super::*;
use std::path::{Component, Path};

use sha2::{Digest, Sha256};

use crate::approval::ApprovalRecordInput;
use crate::artifact::{ArtifactInput, ArtifactRegistryWriter, ArtifactStatus};
use crate::source_citation::{SourceCitationWriter, SourceReadInput, SourceRegisterInput};
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
            .unwrap_or_else(PoisonError::into_inner)
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

    /// Whether a workspace rule (A3) already covers this call.
    fn workspace_rule_covers(&self, call: &ToolCall) -> bool {
        let Some(authorities) = &self.authorities else {
            return false;
        };
        let workspace = self.config.workspace.clone().unwrap_or_default();
        crate::workspace_rules::covers(
            authorities.state_dir(),
            &workspace,
            &call.name,
            &call.arguments,
        )
    }

    /// The user chose "always allow in this workspace" for this call. The call itself was approved;
    /// the rule is made only when the call is one a rule may cover. A rule that can not be made
    /// leaves the call allowed once, and says so in the ledger.
    fn remember_workspace_rule(&self, call: &ToolCall, can_make_rule: bool) {
        let Some(authorities) = &self.authorities else {
            return;
        };
        let workspace = self.config.workspace.clone().unwrap_or_default();
        let outcome = if can_make_rule {
            crate::workspace_rules::add(
                authorities.state_dir(),
                &workspace,
                &call.name,
                &call.arguments,
            )
            .map(|_| ())
            .map_err(|error| error.to_string())
        } else {
            Err("this kind of call is asked every time".to_owned())
        };
        let (kind, detail) = match outcome {
            Ok(()) => ("approval.rule_created", Value::Null),
            Err(error) => ("approval.rule_refused", json!(error)),
        };
        let _ = self.ledger_append(
            kind,
            "user",
            json!({"tool_call_id": call.id, "tool": call.name, "scope": "workspace", "detail": detail}),
        );
    }

    /// Tell the automation circuit breaker how the user answered an approval (A3). The answer is
    /// already given and recorded, so a failure here goes to the ledger and never changes it.
    fn note_automation_outcome(&self, call: &ToolCall, is_approved: bool) {
        let Some(authorities) = &self.authorities else {
            return;
        };
        let outcome =
            crate::automation::AutomationStore::open(authorities.state_dir()).and_then(|store| {
                store.note_approval_outcome(&self.session_id, &call.name, is_approved)
            });
        match outcome {
            Ok(Some(paused)) => {
                let title = paused["title"].as_str().unwrap_or("Automation");
                let body = format!(
                    "{title} was paused because {} was denied {} times in a row. Turn it back on in Automations after reviewing what it asks for.",
                    call.name, paused["count"]
                );
                let key = format!("breaker:{}", paused["task_id"].as_str().unwrap_or_default());
                let _ = authorities.inbox.add_notice(
                    &self.session_id,
                    &key,
                    &format!("Paused: {title}"),
                    &body,
                );
                let _ = self.ledger_append("automation.paused", "runtime", paused);
            }
            Ok(None) => {}
            Err(error) => {
                let _ = self.ledger_append(
                    "automation.breaker_failed",
                    "runtime",
                    json!({"tool": call.name, "error": error.to_string()}),
                );
            }
        }
    }

    /// Tell the product surface, the ledger, the audit log, the Inbox (for an unattended run) and
    /// the checkpoint that `call` is waiting for an answer.
    fn announce_approval(
        &self,
        call: &ToolCall,
        reason: &str,
        level: RiskLevel,
    ) -> Result<(), String> {
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

        Ok(())
    }

    pub(super) fn require_approval(
        &self,
        call: &ToolCall,
        reason: &str,
        level: RiskLevel,
    ) -> Result<ApprovalDecision, String> {
        // The request is registered before anyone is told about it: an answer that comes back
        // while the request is still being recorded (the app, the Inbox) must find it waiting.
        let receiver = self.approvals.begin(&call.id)?;
        if let Err(error) = self.announce_approval(call, reason, level) {
            self.approvals.cancel(&call.id);
            return Err(error);
        }
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
                    self.note_automation_outcome(call, decision.is_approved());
                    return Ok(decision);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("approval request was abandoned".to_string());
                }
            }
        }
    }

    fn formalize_artifacts(
        &self,
        tool_call_id: &str,
        staged: &[StagedArtifact],
    ) -> Result<Vec<Value>, String> {
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
            // Replacing a file must never destroy what was there: the old version moves to
            // `.delta/versions/<run>/<path>` (the first replacement in a run keeps the original),
            // and the result says where, so the user can get it back.
            let mut previous_version = None;
            if target.exists() {
                let kept = workspace
                    .join(".delta")
                    .join("versions")
                    .join(&run_id)
                    .join(relative);
                if !kept.exists() {
                    if let Some(parent) = kept.parent() {
                        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                    }
                    if fs::rename(&target, &kept).is_err() {
                        fs::copy(&target, &kept).map_err(|error| error.to_string())?;
                    }
                }
                previous_version = Some(
                    Path::new(".delta")
                        .join("versions")
                        .join(&run_id)
                        .join(relative)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
                if target.exists() {
                    fs::remove_file(&target).map_err(|error| error.to_string())?;
                }
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
                // Incomplete output is still being drafted. Finished output waits for review.
                status: if candidate.incomplete {
                    ArtifactStatus::Drafting
                } else {
                    ArtifactStatus::PendingReview
                },
                lineage: Vec::new(),
            };
            ArtifactRegistryWriter::new(&lock_recovering(&authorities.ledger))
                .register(&artifact, now_ts(), workspace_text)
                .map_err(|error| error.to_string())?;
            let mut payload = artifact.registered_payload();
            if let Some(previous) = previous_version {
                payload["previous_version"] = Value::String(previous);
            }
            self.emit_event(RuntimeEvent::ArtifactUpdated {
                tool_call_id: tool_call_id.to_string(),
                artifact: payload.clone(),
            });
            if !candidate.incomplete {
                let mut finished = lock_recovering(&self.turn_artifacts);
                finished.retain(|(path, _)| *path != relative_path);
                finished.push((relative_path.clone(), artifact.name.clone()));
            }
            formalized.push(payload);
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
        let ledger = lock_recovering(&authorities.ledger);
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

    /// Record the files a read tool read (`source.read`): register each as a source, then append
    /// the per-run record. Only inputs the capability host hashed before dispatch are recorded.
    fn record_sources_read(
        &self,
        authorities: &RuntimeAuthorities,
        run_id: &str,
        call: &ToolCall,
        inputs: &[VerifiedInput],
    ) -> Result<(), String> {
        if inputs.is_empty() {
            return Ok(());
        }
        let workspace = self.config.workspace.clone().unwrap_or_default();
        let workspace_root = Path::new(&workspace).canonicalize().ok();
        let ledger = lock_recovering(&authorities.ledger);
        let writer = SourceCitationWriter::new(&ledger);
        let captured_at = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|error| error.to_string())?;
        for input in inputs {
            // A workspace file is identified by its path inside the workspace, so the same file
            // keeps one identity if the workspace folder is moved; anything else by its full path.
            let location = workspace_root
                .as_ref()
                .and_then(|root| input.path.strip_prefix(root).ok())
                .unwrap_or(&input.path)
                .to_string_lossy()
                .replace('\\', "/");
            let mtime_ns = std::fs::metadata(&input.path)
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|elapsed| u64::try_from(elapsed.as_nanos()).ok());
            writer
                .record_read(
                    run_id,
                    SourceReadInput {
                        source: SourceRegisterInput {
                            origin: "file".to_string(),
                            location,
                            fingerprint: input.sha256.clone(),
                            captured_at: captured_at.clone(),
                            mtime_ns,
                            size_bytes: Some(input.size),
                            permissions: json!({}),
                        },
                        tool_call_id: call.id.clone(),
                        tool: call.name.clone(),
                    },
                    now_ts(),
                    &workspace,
                )
                .map_err(|error| format!("source.read could not be persisted: {error}"))?;
        }
        Ok(())
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
        let is_read_tool = metadata
            .as_ref()
            .and_then(|metadata| metadata.category.as_deref())
            == Some("read");
        let outbound = match self.gate_outbound_tool(call, metadata.as_ref()) {
            Ok(outbound) => outbound,
            Err(error) => return ToolResult::failure(&call.id, error),
        };
        // Whether this call may become a workspace rule: not one that sends content out, not one
        // that deletes or sends, and not a skill proposal (each of those is asked every time).
        let can_make_rule = outbound.is_none()
            && metadata
                .as_ref()
                .is_some_and(|metadata| workspace_rule_eligible(&call.name, metadata));
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
        let egress = outbound
            .as_ref()
            .map(|gate| gate.egress.clone())
            .unwrap_or(Value::Null);
        if let Err(error) = self.ledger_append(
        "tool.proposed",
        "model",
        json!({"tool_call_id": call.id, "tool": call.name, "arguments": call.arguments, "level": format!("{level:?}"), "egress": egress}),
    ) {
        return ToolResult::failure(&call.id, error);
    }
        if let Some(refusal) = outbound.as_ref().and_then(|gate| gate.refusal()) {
            return self.fail_after_ledger(
                call,
                "tool.failed",
                "runtime",
                json!({"tool_call_id": call.id, "tool": call.name, "stage": "egress", "error": &refusal, "egress": egress}),
                refusal,
            );
        }
        // An outbound call the level makes *Ask each time* or *Approved endpoints* must be approved
        // by the user for this call: no standing grant and no automatic approval applies to it.
        let asks_for_call = outbound
            .as_ref()
            .is_some_and(|gate| gate.requires_call_approval());
        let ask_reason = outbound.as_ref().map(|gate| gate.ask_reason.clone());

        // A grant answers a question. It can not overrule a refusal (`needs_user` is false for one:
        // Plan mode, the side-effect check), nor stand in for the approval an outbound call needs.
        let standing = if decision.allowed || !decision.needs_user || asks_for_call {
            None
        } else if self
            .approvals
            .has_standing_grant(&call.name, &call.arguments)
        {
            Some("approved by a session-scoped standing grant")
        } else if self.workspace_rule_covers(call) {
            Some("approved by a workspace rule")
        } else {
            None
        };
        if let Some(basis) = standing {
            if let Err(error) = self.record_approval(
                call,
                "standing_policy_resolved",
                "auto_approved",
                Some("standing"),
                basis,
                level,
            ) {
                return ToolResult::failure(&call.id, error);
            }
        } else if !decision.allowed || asks_for_call {
            if !decision.allowed && !decision.needs_user {
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
            let reason = match &ask_reason {
                Some(ask_reason) if asks_for_call => ask_reason.as_str(),
                _ => decision.reason.as_str(),
            };
            match self.require_approval(call, reason, level) {
                Ok(approval) if approval.is_approved() => {
                    if approval == ApprovalDecision::AlwaysWorkspace {
                        self.remember_workspace_rule(call, can_make_rule);
                    }
                }
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
        let plan =
            match tool_lifecycle::plan(&lock_recovering(&authorities.idempotency), &plan_input) {
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
            let write_result = lock_recovering(&progress_authorities.ledger).append(
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
                let mut current = lock_recovering(&progress_error_sink);
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
            // Same route as the model request that asked for this tool.
            proxy: self.turn_proxy.clone(),
        };
        let mut result = self.tool_executor.execute(call, &context);
        // Whatever a tool returned, only a bounded amount goes on: into the conversation, the
        // event stream, the transcript, the replay record and the next model request.
        result.output = limit_tool_output(std::mem::take(&mut result.output));

        if let Some(progress_error) = lock_recovering(&progress_error).clone() {
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

        let artifacts = match self.formalize_artifacts(&call.id, &result.staged_artifacts) {
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
        if is_read_tool {
            if let Err(error) =
                self.record_sources_read(authorities, &run_id, call, &result.verified_inputs)
            {
                let primary =
                    format!("{error}; the read is not recorded, so the call is treated as failed");
                if let Err(persistence_error) = self.mark_tool_failed(
                    authorities,
                    &run_id,
                    call,
                    &error,
                    (
                        "tool.failed",
                        "runtime",
                        json!({"tool_call_id": call.id, "tool": call.name, "stage": "source", "error": &error}),
                    ),
                ) {
                    return ToolResult::failure(&call.id, format!("{primary}; {persistence_error}"));
                }
                return ToolResult::failure(&call.id, primary);
            }
        }
        if let Err(error) = lock_recovering(&authorities.idempotency).commit(
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
            json!({"tool_call_id": call.id, "tool": call.name, "result": ledger_summary(&result.output)}),
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

/// Most bytes of one tool's output that go into the conversation, the stores and the model
/// request. A tool that can return more (a long document, a big table) is cut to a preview that
/// says so; a tool that pages by itself, like `read_file`, never reaches it.
pub(super) const MAX_TOOL_OUTPUT_BYTES: usize = 128 * 1024;
/// How much of an over-long output the preview keeps; the rest of the budget is the wrapper.
const OUTPUT_PREVIEW_BYTES: usize = 96 * 1024;
/// Characters of a result the ledger keeps as a preview.
const LEDGER_PREVIEW_CHARS: usize = 500;

/// `output` itself when it fits the limit, otherwise an object that says how long it was and shows
/// the start of it. The `ok` flag and the picture paths survive: the model and the screenshot
/// handling both read them.
pub(super) fn limit_tool_output(output: Value) -> Value {
    let text = output.to_string();
    if text.len() <= MAX_TOOL_OUTPUT_BYTES {
        return output;
    }
    let mut end = OUTPUT_PREVIEW_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut limited = json!({
        "truncated": true,
        "original_bytes": text.len(),
        "shown_bytes": end,
        "preview": &text[..end],
        "note": format!(
            "This result was {} bytes, over the {MAX_TOOL_OUTPUT_BYTES}-byte limit, so only the first {end} bytes of its JSON text are shown. Ask for a smaller part if you need the rest.",
            text.len()
        ),
    });
    if let Some(ok) = output.get("ok") {
        limited["ok"] = ok.clone();
    }
    if let Some(images) = output.get("images") {
        limited["images"] = images.clone();
    }
    limited
}

/// What the ledger keeps of a tool result: how big it was, its hash and the start of it. The
/// ledger records what happened; the content is not kept in it, and the replay record holds the
/// result when one is needed.
fn ledger_summary(output: &Value) -> Value {
    let text = output.to_string();
    let preview: String = text.chars().take(LEDGER_PREVIEW_CHARS).collect();
    let mut summary = json!({
        "bytes": text.len(),
        "sha256": format!("{:x}", Sha256::digest(text.as_bytes())),
        "preview": preview,
    });
    // A cut-down result says how long it was before the cut.
    if let Some(original) = output
        .get("original_bytes")
        .filter(|_| output["truncated"] == true)
    {
        summary["original_bytes"] = original.clone();
    }
    summary
}

/// Whether a call of `name` may be remembered as a workspace rule. Calls that send content off the
/// machine, delete, or propose a skill are asked every time.
fn workspace_rule_eligible(name: &str, metadata: &crate::policy::ToolMetadata) -> bool {
    if name == "save_skill" {
        return false;
    }
    let category = metadata
        .category
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(category.as_str(), "web" | "mcp" | "connector") {
        return false;
    }
    !matches!(
        metadata.side_effect.as_deref(),
        Some("external_write" | "destructive")
    )
}
