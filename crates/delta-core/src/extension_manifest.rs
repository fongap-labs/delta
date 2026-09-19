//! Manifest-driven registration of out-of-process capabilities.
//!
//! Foundation owns discovery, grants and supervision. Each installed extension
//! owns its own manifest under `<state>/extensions/<source>/`. This prevents
//! one extension from overwriting another while keeping every Worker below the
//! Rust-owned CapabilityHost.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::capability::{
    CapabilityGrants, CapabilityHost, CapabilityRegistration, CapabilityRunner, WorkerProcessRunner,
};
use crate::persistent_worker::PersistentWorkerProcessRunner;

pub const EXTENSIONS_DIRNAME: &str = "extensions";
pub const CAPABILITY_WORKER_MANIFEST_FILENAME: &str = "capability-workers.json";
pub const CAPABILITY_WORKER_MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
struct WorkerManifest {
    version: u32,
    source: String,
    #[serde(default)]
    workers: Vec<WorkerEntry>,
}

#[derive(Debug, Deserialize)]
struct WorkerEntry {
    capability_id: String,
    tool_name: String,
    description: String,
    #[serde(default = "default_parameters")]
    parameters: Value,
    #[serde(default)]
    metadata: Value,
    #[serde(default)]
    grants: CapabilityGrants,
    #[serde(default)]
    workspace_write: bool,
    program: String,
    #[serde(default)]
    arguments: Vec<String>,
    #[serde(default)]
    environment: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    persistent: bool,
    #[serde(default)]
    process_key: Option<String>,
}

#[derive(Debug, Default)]
pub struct WorkerManifestLoadReport {
    pub registered: usize,
    pub errors: Vec<String>,
}

fn default_parameters() -> Value {
    json!({"type": "object", "properties": {}})
}

fn default_true() -> bool {
    true
}

fn manifest_paths(state_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let root = state_dir.join(EXTENSIONS_DIRNAME);
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&root)
        .map_err(|error| format!("read extension directory {}: {error}", root.display()))?
    {
        let entry = entry.map_err(|error| format!("read extension directory entry: {error}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest = path.join(CAPABILITY_WORKER_MANIFEST_FILENAME);
        if manifest.is_file() {
            paths.push(manifest);
        }
    }
    paths.sort();
    Ok(paths)
}

fn load_manifest_file(host: &CapabilityHost, path: &Path) -> Result<usize, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("read capability worker manifest: {error}"))?;
    let manifest: WorkerManifest = serde_json::from_str(&text)
        .map_err(|error| format!("parse capability worker manifest: {error}"))?;

    if manifest.version != CAPABILITY_WORKER_MANIFEST_VERSION {
        return Err(format!(
            "unsupported capability worker manifest version: {}",
            manifest.version
        ));
    }

    let source = manifest.source.trim();
    if source.is_empty() {
        return Err("capability worker manifest source is required".to_string());
    }
    let directory_source = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if source != directory_source {
        return Err(format!(
            "capability worker manifest source '{}' does not match extension directory '{}'",
            source, directory_source
        ));
    }

    let mut registered = 0usize;
    let mut persistent_runners: HashMap<String, Arc<dyn CapabilityRunner>> = HashMap::new();
    for entry in manifest.workers.into_iter().filter(|entry| entry.enabled) {
        if entry.capability_id.trim().is_empty()
            || entry.tool_name.trim().is_empty()
            || entry.description.trim().is_empty()
        {
            return Err("worker capability id, tool name and description are required".to_string());
        }

        let program = PathBuf::from(&entry.program);
        if !program.is_absolute() {
            return Err(format!(
                "worker program must be an absolute path: {}",
                entry.program
            ));
        }

        let mut metadata = entry.metadata;
        if !metadata.is_object() {
            metadata = json!({});
        }
        if let Some(object) = metadata.as_object_mut() {
            object
                .entry("extension_source".to_string())
                .or_insert_with(|| Value::String(source.to_string()));
            object
                .entry("execution".to_string())
                .or_insert_with(|| Value::String("worker_process".to_string()));
        }

        let runner: Arc<dyn CapabilityRunner> = if entry.persistent {
            let key = entry.process_key.clone().unwrap_or_else(|| {
                format!(
                    "{}|{:?}|{:?}",
                    program.display(),
                    entry.arguments,
                    entry.environment
                )
            });
            if let Some(existing) = persistent_runners.get(&key) {
                existing.clone()
            } else {
                let created: Arc<dyn CapabilityRunner> = Arc::new(
                    PersistentWorkerProcessRunner::new(program.clone(), entry.arguments.clone())
                        .with_environment(entry.environment.clone()),
                );
                persistent_runners.insert(key, created.clone());
                created
            }
        } else {
            Arc::new(
                WorkerProcessRunner::new(program, entry.arguments)
                    .with_environment(entry.environment),
            )
        };
        host.register(CapabilityRegistration {
            capability_id: entry.capability_id,
            tool_name: entry.tool_name,
            description: entry.description,
            parameters: entry.parameters,
            metadata,
            grants: entry.grants,
            workspace_write: entry.workspace_write,
            runner,
        })?;
        registered += 1;
    }

    Ok(registered)
}

/// Load every installed extension independently.
///
/// Missing extension directories are a normal Foundation-only state. A broken
/// extension is reported and skipped without preventing other extensions or
/// Foundation from starting.
pub fn load_worker_manifests(
    host: &CapabilityHost,
    state_dir: impl AsRef<Path>,
) -> WorkerManifestLoadReport {
    let mut report = WorkerManifestLoadReport::default();
    let paths = match manifest_paths(state_dir.as_ref()) {
        Ok(paths) => paths,
        Err(error) => {
            report.errors.push(error);
            return report;
        }
    };

    for path in paths {
        match load_manifest_file(host, &path) {
            Ok(count) => report.registered += count,
            Err(error) => report.errors.push(format!("{}: {error}", path.display())),
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extension_dir(root: &Path, source: &str) -> PathBuf {
        let dir = root.join(EXTENSIONS_DIRNAME).join(source);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_extension_directory_is_foundation_only_state() {
        let dir = std::env::temp_dir().join(format!("delta-manifest-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let host = CapabilityHost::new();
        let report = load_worker_manifests(&host, &dir);
        assert_eq!(report.registered, 0);
        assert!(report.errors.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_relative_worker_program_without_blocking_other_extensions() {
        let dir = std::env::temp_dir().join(format!("delta-manifest-{}", uuid::Uuid::new_v4()));
        let bad = extension_dir(&dir, "bad");
        std::fs::write(
            bad.join(CAPABILITY_WORKER_MANIFEST_FILENAME),
            r#"{
              "version": 1,
              "source": "bad",
              "workers": [{
                "capability_id": "test.bad",
                "tool_name": "test_bad",
                "description": "bad",
                "program": "python",
                "arguments": ["-m", "test"]
              }]
            }"#,
        )
        .unwrap();

        let good = extension_dir(&dir, "good");
        let program = std::env::current_exe().unwrap();
        std::fs::write(
            good.join(CAPABILITY_WORKER_MANIFEST_FILENAME),
            serde_json::to_vec_pretty(&json!({
                "version": 1,
                "source": "good",
                "workers": [{
                    "capability_id": "test.good",
                    "tool_name": "test_good",
                    "description": "good",
                    "program": program.to_string_lossy(),
                    "arguments": []
                }]
            }))
            .unwrap(),
        )
        .unwrap();

        let host = CapabilityHost::new();
        let report = load_worker_manifests(&host, &dir);
        assert_eq!(report.registered, 1);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("absolute path"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn source_must_match_extension_directory() {
        let dir = std::env::temp_dir().join(format!("delta-manifest-{}", uuid::Uuid::new_v4()));
        let ext = extension_dir(&dir, "actual");
        std::fs::write(
            ext.join(CAPABILITY_WORKER_MANIFEST_FILENAME),
            r#"{"version":1,"source":"other","workers":[]}"#,
        )
        .unwrap();

        let host = CapabilityHost::new();
        let report = load_worker_manifests(&host, &dir);
        assert_eq!(report.registered, 0);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("does not match extension directory"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
