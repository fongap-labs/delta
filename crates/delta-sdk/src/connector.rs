//! Connector API — the contract through which connectors register their
//! capabilities and interact with the Delta Capability Host.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorDescriptor {
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    pub detail_ui: String,
    #[serde(default)]
    pub managed: bool,
}

pub trait ConnectorApi {
    fn register(&self, descriptor: ConnectorDescriptor) -> Result<(), crate::Error>;
    fn list(&self) -> Vec<ConnectorDescriptor>;
    fn disconnect(&self, name: &str) -> Result<(), crate::Error>;
}
