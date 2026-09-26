//! Capability runner contract and Rust-authoritative registration catalogue.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;

use super::abi::{CapabilityGrants, CapabilityJob, CapabilityProgress, CapabilityResult};

pub trait CapabilityRunner: Send + Sync {
    fn run(&self, job: &CapabilityJob, control: &CapabilityControl) -> CapabilityResult;
}

/// Runtime-owned controls visible to a runner. Workers may observe cancellation
/// and emit progress; they cannot mutate Runtime state or grant themselves more
/// access.
#[derive(Clone)]
pub struct CapabilityControl {
    cancel: Arc<AtomicBool>,
    progress: Arc<dyn Fn(CapabilityProgress) + Send + Sync>,
}

impl CapabilityControl {
    pub fn new(
        cancel: Arc<AtomicBool>,
        progress: Arc<dyn Fn(CapabilityProgress) + Send + Sync>,
    ) -> Self {
        Self { cancel, progress }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    pub fn emit_progress(&self, progress: CapabilityProgress) {
        (self.progress)(progress);
    }
}

#[derive(Clone)]
pub struct CapabilityRegistration {
    pub capability_id: String,
    pub tool_name: String,
    pub description: String,
    pub parameters: Value,
    pub metadata: Value,
    pub grants: CapabilityGrants,
    pub workspace_write: bool,
    pub runner: Arc<dyn CapabilityRunner>,
}

/// Rust-authoritative capability catalogue. Registration owns both the model
/// schema and the runner, preventing a model-visible tool from bypassing its
/// controlled execution implementation.
#[derive(Default)]
pub struct CapabilityRegistry {
    by_tool: HashMap<String, CapabilityRegistration>,
}

impl CapabilityRegistry {
    pub fn register(&mut self, registration: CapabilityRegistration) -> Result<(), String> {
        if registration.capability_id.trim().is_empty() || registration.tool_name.trim().is_empty()
        {
            return Err("capability id and tool name are required".to_string());
        }
        if self.by_tool.contains_key(&registration.tool_name) {
            return Err(format!(
                "capability tool is already registered: {}",
                registration.tool_name
            ));
        }
        self.by_tool
            .insert(registration.tool_name.clone(), registration);
        Ok(())
    }

    pub fn get(&self, tool_name: &str) -> Option<CapabilityRegistration> {
        self.by_tool.get(tool_name).cloned()
    }

    pub fn unregister_prefix(&mut self, prefix: &str) -> usize {
        let before = self.by_tool.len();
        self.by_tool.retain(|name, _| !name.starts_with(prefix));
        before.saturating_sub(self.by_tool.len())
    }

    pub fn tool_schemas(&self) -> Value {
        self.tool_schemas_filtered(|_| true)
    }

    pub fn tool_schemas_filtered<F>(&self, mut include: F) -> Value
    where
        F: FnMut(&CapabilityRegistration) -> bool,
    {
        let mut registrations = self.by_tool.values().collect::<Vec<_>>();
        registrations.sort_by(|left, right| left.tool_name.cmp(&right.tool_name));
        Value::Array(
            registrations
                .into_iter()
                .filter(|registration| {
                    include(registration)
                        && registration
                            .metadata
                            .get("model_visible")
                            .and_then(Value::as_bool)
                            != Some(false)
                })
                .map(|registration| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": registration.tool_name,
                            "description": registration.description,
                            "parameters": registration.parameters,
                            "metadata": registration.metadata,
                        }
                    })
                })
                .collect(),
        )
    }
}
