//! Verification of a packaged Python runtime against its manifest (ADR-0080, ADR-0085).
//!
//! A packaged runtime carries `manifest.json`, which lists every file it ships with its SHA-256.
//! Each runtime decides for itself what a missing manifest means: this module only answers what a
//! present manifest says about the files beside it.

use std::path::{Component, Path};

use serde_json::Value;

pub const MANIFEST_NAME: &str = "manifest.json";

/// The problems that stop a runtime from being used, checked against a manifest of `format`.
/// A missing or unreadable manifest is a problem; the caller decides whether that is allowed.
pub fn pack_problems(root: &Path, format: &str) -> Vec<String> {
    let manifest: Value = match std::fs::read(root.join(MANIFEST_NAME)) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(manifest) => manifest,
            Err(_) => return vec![format!("{MANIFEST_NAME} is unreadable")],
        },
        Err(_) => return vec![format!("{MANIFEST_NAME} is missing")],
    };
    if manifest["format"] != format || manifest["version"] != 1 {
        return vec!["manifest format or version is not supported".to_string()];
    }
    let Some(files) = manifest["files"].as_array() else {
        return vec!["manifest lists no files".to_string()];
    };
    let mut problems = Vec::new();
    for entry in files {
        let (Some(relative), Some(expected)) = (entry["path"].as_str(), entry["sha256"].as_str())
        else {
            problems.push("manifest entry is malformed".to_string());
            continue;
        };
        let stays_inside = Path::new(relative)
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
        if !stays_inside {
            problems.push(format!("path escapes the pack: {relative}"));
            continue;
        }
        match std::fs::read(root.join(relative)) {
            Ok(bytes) if sha256_hex(&bytes) == expected => {}
            Ok(_) => problems.push(format!("hash mismatch: {relative}")),
            Err(_) => problems.push(format!("missing file: {relative}")),
        }
    }
    problems
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest_for(root: &Path, format: &str, files: &[&str]) {
        let entries: Vec<Value> = files
            .iter()
            .map(|path| {
                let bytes = std::fs::read(root.join(path)).unwrap();
                json!({"path": path, "sha256": sha256_hex(&bytes), "size": bytes.len()})
            })
            .collect();
        let manifest = json!({"format": format, "version": 1, "files": entries});
        std::fs::write(root.join(MANIFEST_NAME), manifest.to_string()).unwrap();
    }

    #[test]
    fn a_matching_manifest_has_no_problems() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("a.py"), "x = 1\n").unwrap();
        manifest_for(temp.path(), "delta.office-runtime", &["a.py"]);

        assert!(pack_problems(temp.path(), "delta.office-runtime").is_empty());
    }

    #[test]
    fn a_missing_manifest_is_a_problem() {
        let temp = tempfile::tempdir().unwrap();

        assert_eq!(
            pack_problems(temp.path(), "delta.office-runtime"),
            ["manifest.json is missing"]
        );
    }

    #[test]
    fn a_manifest_of_another_format_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("a.py"), "x = 1\n").unwrap();
        manifest_for(temp.path(), "delta.explore-runtime", &["a.py"]);

        assert_eq!(
            pack_problems(temp.path(), "delta.office-runtime"),
            ["manifest format or version is not supported"]
        );
    }

    #[test]
    fn a_changed_file_is_a_hash_mismatch() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("a.py"), "x = 1\n").unwrap();
        manifest_for(temp.path(), "delta.office-runtime", &["a.py"]);
        std::fs::write(temp.path().join("a.py"), "import os\n").unwrap();

        assert_eq!(
            pack_problems(temp.path(), "delta.office-runtime"),
            ["hash mismatch: a.py"]
        );
    }

    #[test]
    fn a_removed_file_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("a.py"), "x = 1\n").unwrap();
        manifest_for(temp.path(), "delta.office-runtime", &["a.py"]);
        std::fs::remove_file(temp.path().join("a.py")).unwrap();

        assert_eq!(
            pack_problems(temp.path(), "delta.office-runtime"),
            ["missing file: a.py"]
        );
    }

    #[test]
    fn a_path_that_escapes_the_pack_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = json!({
            "format": "delta.office-runtime",
            "version": 1,
            "files": [{"path": "../outside.py", "sha256": "00", "size": 0}],
        });
        std::fs::write(temp.path().join(MANIFEST_NAME), manifest.to_string()).unwrap();

        assert_eq!(
            pack_problems(temp.path(), "delta.office-runtime"),
            ["path escapes the pack: ../outside.py"]
        );
    }
}
