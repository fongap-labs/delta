//! Worker Manifest — canonical wire contract for worker declaration (R8.2).
//!
//! Aligned with `schemas/capability-manifest.schema.json`. A worker
//! declares its capabilities, permissions, and timeouts via this manifest.
//! The Runtime (Capability Host) reads the manifest to register the worker
//! and enforce its declared permission scope.

use serde::{Deserialize, Serialize};

/// A single capability declared by a worker manifest entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestCapability {
    /// Fully-qualified capability identifier (e.g. "research.statistics.anova").
    pub capability_id: String,
    /// Permission tokens required to execute this capability.
    pub permissions: Vec<String>,
    /// Maximum wall-clock seconds before the capability is force-cancelled.
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

/// Worker Manifest — declares a worker's capabilities and permission scope.
///
/// Wire-aligned with `schemas/capability-manifest.schema.json`:
/// ```json
/// {
///   "worker_id": "string",
///   "capabilities": [{ "capability_id": "...", "permissions": [...], "timeout_seconds": ... }]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerManifest {
    /// Unique identifier for the worker process.
    pub worker_id: String,
    /// Capabilities declared by this worker.
    pub capabilities: Vec<ManifestCapability>,
}

/// Worker API — the contract through which Delta Runtime supervises Worker
/// processes (Python, PowerShell, Shell) and external adapters.
pub trait WorkerApi {
    fn discover(&self) -> Vec<WorkerManifest>;
    fn execute(
        &self,
        manifest: &WorkerManifest,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, crate::Error>;
    fn cancel(&self, job_id: &str) -> Result<(), crate::Error>;
}
