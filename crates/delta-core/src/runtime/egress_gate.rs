//! The egress check before every model request (ADR-0057).
//!
//! The workspace's egress level decides whether the request may go to the configured endpoint.
//! A refused request is never sent and ends the run with an `egress` error. *Ask each time* asks
//! once per run through the normal approval path (parked in the Inbox for unattended runs). Every
//! request that is sent is recorded as a `model.request` ledger event, which names the endpoint,
//! the level and the sources the run had read, and never contains message content.

use super::*;

use std::net::SocketAddr;

use crate::egress::{self, EgressLevel, EndpointClass, ModelDecision};
use crate::policy::RiskLevel;
use crate::source_citation::SourceCitationReader;

/// The text of the refusal shown when no default level has been chosen yet.
const NOT_CHOSEN: &str = "Choose where this workspace's content may be sent (Settings ▸ Privacy) before Delta contacts a model. Nothing was sent.";

impl RuntimeHost {
    /// Decide the request about to be sent. `Ok` carries the addresses the connection must use
    /// (empty when the decision did not rely on them); `Err` is the refusal shown to the user.
    pub(super) fn gate_model_request(
        &mut self,
        req: &ProviderRequest,
        iteration: usize,
        is_confirmed: &mut bool,
    ) -> Result<Vec<SocketAddr>, String> {
        // Hosts built without authorities are test hosts; the product always has them.
        let Some(authorities) = self.authorities.clone() else {
            return Ok(Vec::new());
        };
        let state_dir = authorities.state_dir().to_path_buf();
        let workspace = self.config.workspace.clone().unwrap_or_default();
        let level = egress::level_for(&state_dir, &workspace).map_err(|error| {
            format!("The egress settings could not be read, so nothing was sent: {error}")
        })?;
        let Some(level) = level else {
            self.record_refusal(req, None, "not_chosen")?;
            return Err(NOT_CHOSEN.to_string());
        };
        let mut class_label = None;
        let endpoint = match egress::classify_endpoint(&state_dir, &req.base_url) {
            Ok(endpoint) => endpoint,
            // Open and Ask each time do not depend on the class; the others cannot decide without
            // it. The request is then recorded with an `unresolved` class.
            Err(_) if matches!(level, EgressLevel::Open | EgressLevel::AskEachTime) => {
                class_label = Some("unresolved");
                egress::EndpointInfo {
                    host: endpoint_host(&req.base_url),
                    class: EndpointClass::Public,
                    addrs: Vec::new(),
                }
            }
            Err(error) => {
                self.record_refusal(req, Some(level), "unresolved")?;
                return Err(format!(
                    "This workspace's egress level needs to know where {} is, and it could not be checked: {error}. Nothing was sent.",
                    endpoint_host(&req.base_url)
                ));
            }
        };
        match egress::decide_model_request(Some(level), &endpoint.class) {
            ModelDecision::Allow => {}
            ModelDecision::NotChosen => {
                self.record_refusal(req, Some(level), "not_chosen")?;
                return Err(NOT_CHOSEN.to_string());
            }
            ModelDecision::Deny(reason) => {
                self.record_refusal(req, Some(level), endpoint.class.as_str())?;
                return Err(format!(
                    "Not sent to {}: {reason}. Change the workspace's egress level or use another model.",
                    endpoint.host
                ));
            }
            ModelDecision::Ask => {
                if !*is_confirmed {
                    self.confirm_endpoint(req, level, &endpoint)?;
                    *is_confirmed = true;
                }
            }
        }
        let class_label = class_label.unwrap_or(endpoint.class.as_str());
        self.record_request(req, iteration, level, &endpoint, class_label, &state_dir)?;
        let relies_on_addresses = endpoint.class == EndpointClass::Private
            && matches!(
                level,
                EgressLevel::PrivateOnly | EgressLevel::ApprovedEndpoints
            );
        Ok(if relies_on_addresses {
            endpoint.addrs
        } else {
            Vec::new()
        })
    }

    /// Ask the user to confirm the endpoint for this run. A denial ends the run.
    fn confirm_endpoint(
        &mut self,
        req: &ProviderRequest,
        level: EgressLevel,
        endpoint: &egress::EndpointInfo,
    ) -> Result<(), String> {
        let call = ToolCall {
            id: format!(
                "egress-{}-{}",
                self.run_id.clone().unwrap_or_default(),
                uuid_v4()
            ),
            name: "model.request".to_string(),
            arguments: json!({
                "endpoint": endpoint.host,
                "model": req.model,
                "level": level.as_str(),
            }),
        };
        let reason = format!(
            "This workspace asks each time: the conversation and the files read in it will be sent to {} for model {}.",
            endpoint.host, req.model
        );
        let decision = self.require_approval(&call, &reason, RiskLevel::L3)?;
        if decision.is_approved() {
            Ok(())
        } else {
            Err(format!(
                "Not sent to {}: you did not confirm the endpoint for this run.",
                endpoint.host
            ))
        }
    }

    fn record_request(
        &self,
        req: &ProviderRequest,
        iteration: usize,
        level: EgressLevel,
        endpoint: &egress::EndpointInfo,
        class_label: &str,
        state_dir: &Path,
    ) -> Result<(), String> {
        let bytes = serde_json::to_vec(&req.messages)
            .map(|body| body.len())
            .unwrap_or(0);
        let sources: Vec<Value> = match &self.run_id {
            Some(run_id) => SourceCitationReader::open(state_dir.join("run_events.db"))
                .and_then(|reader| reader.sources_read(run_id))
                .map(|records| {
                    records
                        .into_iter()
                        .map(|record| json!({"source_id": record.source_id, "sha256": record.fingerprint}))
                        .collect()
                })
                .unwrap_or_default(),
            None => Vec::new(),
        };
        self.ledger_append(
            "model.request",
            "runtime",
            json!({
                "iteration": iteration,
                "endpoint": endpoint.host,
                "endpoint_class": class_label,
                "model": req.model,
                "protocol": req.protocol,
                "level": level.as_str(),
                "bytes": bytes,
                "sources": sources,
            }),
        )
    }

    fn record_refusal(
        &self,
        req: &ProviderRequest,
        level: Option<EgressLevel>,
        why: &str,
    ) -> Result<(), String> {
        // Nobody watches an unattended run: the refusal also becomes an Inbox notice.
        if self.config.unattended {
            if let Some(authorities) = &self.authorities {
                let title = if why == "not_chosen" {
                    "Choose where workspace content may be sent"
                } else {
                    "An automation could not contact its model"
                };
                let body = format!(
                    "Nothing was sent to {}. The workspace's egress level did not allow it ({why}).",
                    endpoint_host(&req.base_url)
                );
                authorities
                    .inbox()
                    .add_notice(&self.session_id, &format!("egress-{why}"), title, &body)
                    .map_err(|error| error.to_string())?;
            }
        }
        self.ledger_append(
            "model.request_refused",
            "runtime",
            json!({
                "endpoint": endpoint_host(&req.base_url),
                "model": req.model,
                "level": level.map(EgressLevel::as_str),
                "why": why,
            }),
        )
    }
}

fn endpoint_host(base_url: &str) -> String {
    url::Url::parse(base_url.trim())
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_else(|| "the configured endpoint".to_string())
}
