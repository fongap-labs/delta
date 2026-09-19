//! Event API — the contract through which extensions receive Runtime events
//! (run lifecycle, tool progress, approval requests, ledger entries).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEvent {
    pub event_type: String,
    pub run_id: String,
    pub payload: serde_json::Value,
    pub timestamp: String,
}

pub trait EventApi {
    fn subscribe(&self, run_id: &str) -> Result<(), crate::Error>;
    fn poll(&self) -> Vec<RuntimeEvent>;
}
