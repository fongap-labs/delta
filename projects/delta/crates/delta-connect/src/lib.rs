//! Delta Connect — connector contract types.
//!
//! Defines the stable data model for connector descriptors, tool metadata,
//! and account primitives that live in Foundation. First-party connector
//! implementations belong in Delta Suite.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorEntry {
    pub name: String,
    pub display_name: String,
    pub detail_ui: String,
    pub managed: bool,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRef {
    pub connector: String,
    pub account_id: String,
    pub display_name: String,
    pub managed: bool,
}

pub trait ConnectorRegistry {
    fn register(&mut self, entry: ConnectorEntry) -> Result<(), String>;
    fn get(&self, name: &str) -> Option<&ConnectorEntry>;
    fn list(&self) -> Vec<&ConnectorEntry>;
    fn tool_schemas(&self) -> serde_json::Value;
}
