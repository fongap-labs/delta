//! Delta Sync — managed sync and relay contract ports.
//!
//! Foundation owns only the port (trait) and Null/default implementation.
//! Concrete managed sync, relay, and OAuth broker implementations belong in
//! Delta Suite.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub relay_url: Option<String>,
}

pub trait RelayTransport {
    fn send(&self, message: &str) -> Result<(), String>;
    fn receive(&self) -> Result<Vec<String>, String>;
}

pub struct NullRelayTransport;

impl RelayTransport for NullRelayTransport {
    fn send(&self, _message: &str) -> Result<(), String> {
        Err("managed relay is unavailable: no relay configured".into())
    }
    fn receive(&self) -> Result<Vec<String>, String> {
        Err("managed relay is unavailable: no relay configured".into())
    }
}

pub trait OAuthBroker {
    fn begin(&self, provider: &str) -> Result<String, String>;
    fn refresh(&self, provider: &str) -> Result<(), String>;
    fn disconnect(&self, provider: &str) -> Result<(), String>;
}

pub struct NullOAuthBroker;

impl OAuthBroker for NullOAuthBroker {
    fn begin(&self, _provider: &str) -> Result<String, String> {
        Err("managed OAuth is unavailable: no broker configured".into())
    }
    fn refresh(&self, _provider: &str) -> Result<(), String> {
        Err("managed OAuth is unavailable: no broker configured".into())
    }
    fn disconnect(&self, _provider: &str) -> Result<(), String> {
        Ok(())
    }
}
