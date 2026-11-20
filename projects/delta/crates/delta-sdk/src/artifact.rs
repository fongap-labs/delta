//! Artifact API — canonical wire contracts for artifact staging and
//! registration (R8.2).
//!
//! Workers never own Artifact authority. They submit staged output via
//! `ArtifactSubmission`; the Runtime validates, hashes, and formally
//! registers the artifact as an `ArtifactRecord`.

use serde::{Deserialize, Serialize};

/// A staged artifact submitted by a worker for Runtime formalization.
/// Field names are aligned with `CapabilityArtifact` (wire).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactSubmission {
    pub run_id: String,
    /// Worker-owned candidate path under the staging directory.
    pub staging_path: String,
    /// Requested destination relative to the workspace.
    pub relative_path: String,
    /// Artifact kind (e.g. "file", "csv", "sheet", "image", "report").
    #[serde(default)]
    pub kind: Option<String>,
    /// SHA-256 hash, if the worker computed one. The Runtime recomputes
    /// and verifies this during formalization.
    #[serde(default)]
    pub sha256: Option<String>,
    /// File size in bytes, if known.
    #[serde(default)]
    pub size: Option<u64>,
    /// Whether the artifact is incomplete (Runtime may reject or defer).
    #[serde(default)]
    pub incomplete: bool,
}

/// A formally registered artifact record (post-validation, post-hash).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub artifact_id: String,
    pub run_id: String,
    pub relative_path: String,
    pub kind: String,
    pub sha256: String,
    pub size: u64,
    pub registered_at: String,
}

pub trait ArtifactApi {
    fn submit(&self, submission: ArtifactSubmission) -> Result<ArtifactRecord, crate::Error>;
    fn list_for_run(&self, run_id: &str) -> Vec<ArtifactRecord>;
}
