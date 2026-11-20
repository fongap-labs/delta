//! The egress check before an outbound tool call (ADR-0061).
//!
//! A tool that sends content off the machine declares where it sends it. The workspace's egress
//! level decides whether the call may go ahead. *Ask each time* and *Approved endpoints* make a
//! call that is allowed only after the user approves that call; standing grants and automatic
//! approval never apply to it. A refused call is not run. Every outbound call records its
//! destination in the ledger, on the `tool.proposed` event or on the refusal.

use super::*;

use crate::egress::{self, EgressLevel, OutboundDecision};
use crate::policy::ToolMetadata;

/// The text of the refusal when no default level has been chosen yet.
const NOT_CHOSEN: &str = "Choose where this workspace's content may be sent (Settings ▸ Privacy) before a tool sends anything. Nothing was sent.";

/// The outcome of the outbound check for one call.
pub(super) struct OutboundGate {
    pub decision: OutboundDecision,
    /// The text shown on the approval card when the call must be approved.
    pub ask_reason: String,
    /// The ledger object: destination class, host, level and the decision.
    pub egress: Value,
}

impl OutboundGate {
    /// The refusal for a call that must not run; `None` when it may run, possibly after approval.
    pub(super) fn refusal(&self) -> Option<String> {
        match &self.decision {
            OutboundDecision::Deny(reason) => Some(reason.clone()),
            OutboundDecision::NotChosen => Some(NOT_CHOSEN.to_string()),
            OutboundDecision::Allow | OutboundDecision::Ask => None,
        }
    }

    /// Whether the user must approve this call, whatever the policy and the standing grants say.
    pub(super) fn requires_call_approval(&self) -> bool {
        self.decision == OutboundDecision::Ask
    }
}

impl RuntimeHost {
    /// Decide an outbound tool call. `Ok(None)` for a tool that sends nothing off the machine.
    /// `Err` is a failure to read the settings or to check the destination; nothing is run then.
    pub(super) fn gate_outbound_tool(
        &self,
        call: &ToolCall,
        metadata: Option<&ToolMetadata>,
    ) -> Result<Option<OutboundGate>, String> {
        let Some(metadata) = metadata else {
            return Ok(None);
        };
        // Hosts built without authorities are test hosts; the product always has them.
        let Some(authorities) = self.authorities.clone() else {
            return Ok(None);
        };
        let state_dir = authorities.state_dir().to_path_buf();
        let Some(target) =
            egress::outbound_target(&state_dir, metadata, &call.name, Some(&call.arguments))?
        else {
            return Ok(None);
        };
        let workspace = self.config.workspace.clone().unwrap_or_default();
        let level = egress::level_for(&state_dir, &workspace).map_err(|error| {
            format!("The egress settings could not be read, so nothing was sent: {error}")
        })?;
        let rule = egress::path_rule(&state_dir, &workspace, &target.path).map_err(|error| {
            format!("The per-path rules could not be read, so nothing was sent: {error}")
        })?;
        let decision = egress::decide_path(level, &target, rule);
        let effective = egress::effective_class(&target);
        let destination = if target.host.is_empty() {
            effective.as_str().to_owned()
        } else {
            target.host.clone()
        };
        let egress_object = json!({
            "class": effective.as_str(),
            "host": target.host,
            "level": level.map(EgressLevel::as_str),
            "decision": decision_label(&decision),
            "path": target.path,
            "override": rule.map(egress::PathRule::as_str),
        });
        Ok(Some(OutboundGate {
            decision,
            ask_reason: format!(
                "This workspace asks each time: {} will send content to {destination}. Nothing is sent until you approve this call.",
                call.name
            ),
            egress: egress_object,
        }))
    }
}

impl RuntimeHost {
    /// The destination class and egress level a declared side effect is judged against. The
    /// destination is `None` when the call sends nothing off the machine.
    pub(super) fn side_effect_context(
        &self,
        call: &ToolCall,
        metadata: Option<&ToolMetadata>,
    ) -> Result<(Option<egress::DestinationClass>, Option<EgressLevel>), String> {
        // Hosts built without authorities are test hosts; the product always has them.
        let Some(authorities) = self.authorities.clone() else {
            return Ok((None, None));
        };
        let state_dir = authorities.state_dir().to_path_buf();
        let destination = match metadata {
            Some(metadata) => {
                egress::outbound_target(&state_dir, metadata, &call.name, Some(&call.arguments))?
                    .map(|target| egress::effective_class(&target))
            }
            None => None,
        };
        let workspace = self.config.workspace.clone().unwrap_or_default();
        let level = egress::level_for(&state_dir, &workspace).map_err(|error| {
            format!("The egress settings could not be read, so nothing was sent: {error}")
        })?;
        Ok((destination, level))
    }
}

fn decision_label(decision: &OutboundDecision) -> &'static str {
    match decision {
        OutboundDecision::Allow => "allowed",
        OutboundDecision::Ask => "ask",
        OutboundDecision::Deny(_) => "blocked",
        OutboundDecision::NotChosen => "not_chosen",
    }
}
