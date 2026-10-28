//! Delta SDK -- public extension contracts (R8.2 canonical).
//!
//! Suite and third-party integrations consume Delta Foundation exclusively
//! through the trait surfaces and wire types defined in this crate.
//! Foundation authority (Runtime, Trust, Work, Capability, Automation,
//! Learning) is never re-exported as a public type; only the stable API
//! boundary is exposed.
//!
//! These wire types are the single canonical contract for data shapes
//! that cross process or repository boundaries. The Rust Runtime
//! (`delta-core`) owns internal runtime models; explicit conversions
//! exist from runtime models to these wire DTOs.

pub mod artifact;
pub mod capability;
pub mod connector;
pub mod event;
pub mod worker;

pub use thiserror::Error;

/// Canonical SDK error types.
#[derive(Debug, Error)]
pub enum Error {
    #[error("capability unavailable: {0}")]
    Unavailable(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("grant expired: {0}")]
    GrantExpired(String),
    #[error("grant not active: {0}")]
    GrantNotActive(String),
    #[error("invalid grant window: {0}")]
    InvalidGrantWindow(String),
    #[error("grant invalid: {0}")]
    GrantInvalid(String),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}
