//! Product edition marker for the About screen (ADR-0083).
//!
//! A display-only marker lives under `<state>/extensions/<source>/product-edition.json`. Foundation
//! reads the highest-ranked valid marker and otherwise shows Delta. This is not an entitlement.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::Manager;

const EDITION_FORMAT: &str = "delta.product-edition";
const EDITION_VERSION: u32 = 1;
const MAX_EDITION_BYTES: u64 = 4096;
const MAX_EDITION_CHARS: usize = 32;
const MAX_LABEL_CHARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edition {
    pub edition: String,
    pub label: String,
}

impl Default for Edition {
    fn default() -> Self {
        Self {
            edition: "base".to_owned(),
            label: "Delta".to_owned(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EditionFile {
    format: String,
    version: u32,
    edition: String,
    label: String,
    source: String,
    #[serde(default)]
    rank: i64,
}

#[derive(Debug, Serialize)]
pub struct ProductInfo {
    pub edition: String,
    pub label: String,
    pub version: String,
}

/// Read the highest-ranked valid edition marker from the installed extensions directory.
pub fn edition_from_state(state_dir: &Path) -> Edition {
    let root = state_dir.join("extensions");
    let Ok(entries) = fs::read_dir(&root) else {
        return Edition::default();
    };
    let mut directories: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    directories.sort();

    let mut best: Option<(i64, Edition)> = None;
    for directory in directories {
        let Some(source) = directory.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if source.is_empty() || !directory.join("capability-workers.json").is_file() {
            continue;
        }
        let path = directory.join("product-edition.json");
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > MAX_EDITION_BYTES {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(file) = serde_json::from_str::<EditionFile>(&text) else {
            continue;
        };
        if file.format != EDITION_FORMAT || file.version != EDITION_VERSION || file.source != source
        {
            continue;
        }
        let edition = file.edition.trim().to_owned();
        let label = file.label.trim().to_owned();
        if edition.is_empty()
            || label.is_empty()
            || edition.chars().count() > MAX_EDITION_CHARS
            || label.chars().count() > MAX_LABEL_CHARS
        {
            continue;
        }
        let candidate = Edition { edition, label };
        match &best {
            Some((rank, _)) if *rank >= file.rank => {}
            _ => best = Some((file.rank, candidate)),
        }
    }

    best.map(|(_, edition)| edition).unwrap_or_default()
}

#[tauri::command]
pub fn product_info(app: tauri::AppHandle) -> ProductInfo {
    let edition = edition_from_state(&crate::state_dir());
    ProductInfo {
        edition: edition.edition,
        label: edition.label,
        version: app.package_info().version.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    fn marker(source: &str, rank: i64) -> serde_json::Value {
        json!({
            "format": EDITION_FORMAT,
            "version": 1,
            "edition": "pro",
            "label": "Delta Pro",
            "source": source,
            "rank": rank,
        })
    }

    fn install(state: &Path, source: &str, rank: i64) {
        let directory = state.join("extensions").join(source);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("capability-workers.json"), b"{}").unwrap();
        fs::write(
            directory.join("product-edition.json"),
            serde_json::to_vec(&marker(source, rank)).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn no_marker_shows_delta() {
        let state = tempfile::tempdir().unwrap();
        assert_eq!(edition_from_state(state.path()), Edition::default());
    }

    #[test]
    fn a_valid_marker_shows_its_label() {
        let state = tempfile::tempdir().unwrap();
        install(state.path(), "example-extension", 100);
        let edition = edition_from_state(state.path());
        assert_eq!(edition.edition, "pro");
        assert_eq!(edition.label, "Delta Pro");
    }

    #[test]
    fn a_marker_without_an_installed_manifest_is_ignored() {
        let state = tempfile::tempdir().unwrap();
        let directory = state.path().join("extensions").join("example-extension");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("product-edition.json"),
            serde_json::to_vec(&marker("example-extension", 100)).unwrap(),
        )
        .unwrap();
        assert_eq!(edition_from_state(state.path()), Edition::default());
    }

    #[test]
    fn a_source_mismatch_is_ignored() {
        let state = tempfile::tempdir().unwrap();
        install(state.path(), "actual-extension", 100);
        let directory = state.path().join("extensions").join("actual-extension");
        fs::write(
            directory.join("product-edition.json"),
            serde_json::to_vec(&marker("other-extension", 100)).unwrap(),
        )
        .unwrap();
        assert_eq!(edition_from_state(state.path()), Edition::default());
    }

    #[test]
    fn the_highest_rank_wins() {
        let state = tempfile::tempdir().unwrap();
        install(state.path(), "first-extension", 10);
        install(state.path(), "second-extension", 200);
        let edition = edition_from_state(state.path());
        assert_eq!(edition.label, "Delta Pro");
        assert_eq!(edition.edition, "pro");
    }
}
