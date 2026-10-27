//! Installing a packaged runtime on first use (ADR-0085, distribution of ADR-0080).
//!
//! A runtime pack is a ZIP archive whose files are listed in its `manifest.json` (see
//! [`runtime_pack`]). The installer fetches the archive from a fixed HTTPS address, checks it
//! against the digest the application was built with, extracts it into a staging directory,
//! checks the extracted files against the manifest, and only then moves the directory into place.
//! A failure at any step leaves nothing behind, and nothing is registered from a runtime that did
//! not pass every check.
//!
//! The installer runs only on an outbound decision of [`OutboundDecision::Allow`]. Deciding that
//! decision is the caller's job; this module never widens what the user's egress level allows.

use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::json;

use crate::approval::{ApprovalRecordInput, ApprovalWriter};
use crate::egress::OutboundDecision;
use crate::runtime_pack;

/// The largest archive the installer accepts, in bytes.
pub const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;

/// The most bytes the archive may expand to. A ZIP that declares more is refused before extraction.
pub const MAX_EXTRACTED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// The fixed description of one runtime pack, compiled into the application.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PackSource {
    /// The pack's name, for example `office`. Lowercase letters, digits and hyphens.
    pub name: String,
    /// The pack's version, `major.minor.patch`.
    pub version: String,
    /// The fixed HTTPS address of the archive.
    pub url: String,
    /// The lowercase hex SHA-256 of the archive.
    pub sha256: String,
    /// The exact size of the archive in bytes.
    pub size: u64,
    /// The format the pack's manifest declares, for example `delta.python-runtime`.
    pub format: String,
    /// The interpreter's path inside the pack, relative and without `..`, for example
    /// `python/bin/python3`.
    pub interpreter: String,
}

/// The shared runtime pack that Explore and Office run in: CPython, locked dependencies and the
/// worker packages. It is installed once, on first use.
pub const PYTHON_PACK_NAME: &str = "delta-python";
/// The manifest format of the shared runtime pack.
pub const PYTHON_PACK_FORMAT: &str = "delta.python-runtime";

/// The fixed description of the published shared runtime pack, compiled into the application.
/// `None` until a pack is published; the runtimes then stay unregistered.
pub fn published_python_pack() -> Option<PackSource> {
    #[derive(Deserialize)]
    struct Catalogue {
        packs: Vec<PackSource>,
    }
    let catalogue: Catalogue =
        serde_json::from_str(include_str!("../../../runtime/packs.json")).ok()?;
    catalogue
        .packs
        .into_iter()
        .find(|pack| pack.name == PYTHON_PACK_NAME)
}

/// How the installer fetches bytes. The production implementation speaks HTTPS through the
/// application's proxy settings and lands with the wiring of the runtimes; tests supply bytes
/// directly.
pub trait Fetch {
    /// The archive at `url`, which must not be larger than `max_bytes`.
    fn fetch(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, String>;
}

/// The result of an install that did not fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    /// The archive was fetched, checked and installed.
    Installed(PathBuf),
    /// A copy that still passes its manifest was already in place; nothing was fetched.
    AlreadyInstalled(PathBuf),
}

/// Where a pack of `name` and `version` lives under the state directory.
pub fn install_root(state_dir: &Path, name: &str, version: &str) -> PathBuf {
    state_dir.join("runtimes").join(format!("{name}-{version}"))
}

/// Install `source` under `state_dir` if `decision` allows the fetch.
pub fn install(
    state_dir: &Path,
    source: &PackSource,
    decision: &OutboundDecision,
    fetch: &dyn Fetch,
) -> Result<InstallOutcome, String> {
    if *decision != OutboundDecision::Allow {
        return Err("the runtime download is not allowed by the current egress level".to_string());
    }
    validate(source)?;
    let destination = install_root(state_dir, &source.name, &source.version);
    if destination.is_dir() && runtime_pack::pack_problems(&destination, &source.format).is_empty()
    {
        return Ok(InstallOutcome::AlreadyInstalled(destination));
    }

    let bytes = fetch.fetch(&source.url, source.size)?;
    if bytes.len() as u64 != source.size {
        return Err(format!(
            "the archive is {} bytes, expected {}",
            bytes.len(),
            source.size
        ));
    }
    if runtime_pack::sha256_hex(&bytes) != source.sha256 {
        return Err("the archive does not match its published digest".to_string());
    }

    let staging = staging_root(state_dir, source);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let staged = extract(&bytes, &staging).and_then(|()| {
        let problems = runtime_pack::pack_problems(&staging, &source.format);
        if problems.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "the runtime does not match its manifest: {}",
                problems.join("; ")
            ))
        }
    });
    if let Err(error) = staged {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }

    if destination.exists() {
        std::fs::remove_dir_all(&destination).map_err(|error| error.to_string())?;
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if let Err(error) = std::fs::rename(&staging, &destination) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error.to_string());
    }
    Ok(InstallOutcome::Installed(destination))
}

fn staging_root(state_dir: &Path, source: &PackSource) -> PathBuf {
    state_dir
        .join("runtimes")
        .join(format!(".staging-{}-{}", source.name, source.version))
}

fn validate(source: &PackSource) -> Result<(), String> {
    let interpreter_ok = !source.interpreter.is_empty()
        && Path::new(&source.interpreter)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)));
    if !interpreter_ok {
        return Err("the interpreter path must be relative and stay inside the pack".to_string());
    }
    let name_ok = !source.name.is_empty()
        && source.name.len() <= 40
        && source
            .name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && source.name.starts_with(|c: char| c.is_ascii_lowercase());
    if !name_ok {
        return Err("the pack name must be lowercase letters, digits and hyphens".to_string());
    }
    let version_ok = {
        let parts: Vec<&str> = source.version.split('.').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
    };
    if !version_ok {
        return Err("the pack version must be major.minor.patch".to_string());
    }
    if source.sha256.len() != 64
        || !source
            .sha256
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    {
        return Err("the pack digest must be 64 lowercase hex characters".to_string());
    }
    if source.size == 0 || source.size > MAX_ARCHIVE_BYTES {
        return Err("the pack size is outside the accepted range".to_string());
    }
    require_https(&source.url)
}

/// The address must be HTTPS with a host. Nothing else is fetched on a runtime's behalf.
pub fn require_https(url: &str) -> Result<(), String> {
    let parsed =
        url::Url::parse(url).map_err(|_| "the pack address is not a valid URL".to_string())?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err("the pack address must be an https address with a host".to_string());
    }
    Ok(())
}

fn extract(bytes: &[u8], destination: &Path) -> Result<(), String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let mut declared: u64 = 0;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| error.to_string())?;
        declared = declared.saturating_add(entry.size());
        if declared > MAX_EXTRACTED_BYTES {
            return Err("the archive expands beyond the accepted size".to_string());
        }
    }
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let Some(relative) = entry.enclosed_name() else {
            return Err("an archive entry would leave the pack".to_string());
        };
        let target = destination.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut file = File::create(&target).map_err(|error| error.to_string())?;
        std::io::copy(&mut entry, &mut file).map_err(|error| error.to_string())?;
        file.flush().map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// The record of one approval, written when the user confirms a download.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Approval {
    name: String,
    version: String,
    sha256: String,
    approved_at: String,
}

fn approvals_path(state_dir: &Path) -> PathBuf {
    state_dir.join("runtimes").join("approvals.json")
}

fn read_approvals(state_dir: &Path) -> Vec<Approval> {
    std::fs::read(approvals_path(state_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Whether the user approved exactly this pack: its name, version and archive digest.
pub fn is_approved(state_dir: &Path, source: &PackSource) -> bool {
    read_approvals(state_dir).iter().any(|approval| {
        approval.name == source.name
            && approval.version == source.version
            && approval.sha256 == source.sha256
    })
}

/// Record the user's approval of `source` and write it to the audit log.
pub fn record_approval(
    state_dir: &Path,
    source: &PackSource,
    approved_at: &str,
) -> Result<(), String> {
    let mut approvals = read_approvals(state_dir);
    approvals.retain(|approval| approval.name != source.name);
    approvals.push(Approval {
        name: source.name.clone(),
        version: source.version.clone(),
        sha256: source.sha256.clone(),
        approved_at: approved_at.to_string(),
    });
    std::fs::create_dir_all(state_dir.join("runtimes")).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec_pretty(&approvals).map_err(|error| error.to_string())?;
    std::fs::write(approvals_path(state_dir), bytes).map_err(|error| error.to_string())?;
    audit(state_dir, source, "approved");
    Ok(())
}

/// Record that the user declined `source`.
pub fn record_refusal(state_dir: &Path, source: &PackSource) {
    audit(state_dir, source, "declined");
}

fn audit(state_dir: &Path, source: &PackSource, stage: &str) {
    let path = state_dir.join("audit_events.db");
    let result = ApprovalWriter::open(path.to_string_lossy().as_ref()).and_then(|mut writer| {
        writer.record(
            ApprovalRecordInput {
                session_id: String::new(),
                agent: Some("delta".to_string()),
                workspace: None,
                connector: None,
                tool: "runtime.install".to_string(),
                stage: stage.to_string(),
                status: Some("ok".to_string()),
                approval: Some("user".to_string()),
                arguments: Some(json!({
                    "name": source.name,
                    "version": source.version,
                    "sha256": source.sha256,
                })),
                result_preview: None,
                reason: None,
                resource: Some(source.url.clone()),
                level: None,
                isolation: None,
                ts: None,
            },
            0.0,
            "",
        )
    });
    if let Err(error) = result {
        eprintln!("runtime: could not record the install decision in the audit log: {error}");
    }
}

/// Install `source` only when the user has approved this exact pack. Without an approval nothing
/// is fetched; the caller asks the user first.
pub fn install_if_approved(
    state_dir: &Path,
    source: &PackSource,
    fetch: &dyn Fetch,
) -> Result<InstallOutcome, String> {
    let decision = if is_approved(state_dir, source) {
        OutboundDecision::Allow
    } else {
        OutboundDecision::Ask
    };
    install(state_dir, source, &decision, fetch)
}

/// Where the installed shared runtime lives and which interpreter runs it, if a copy that passes
/// its manifest is in place.
pub fn installed_python_pack(state_dir: &Path, source: &PackSource) -> Option<(PathBuf, PathBuf)> {
    let root = install_root(state_dir, &source.name, &source.version);
    if !root.is_dir() || !runtime_pack::pack_problems(&root, &source.format).is_empty() {
        return None;
    }
    Some((root.clone(), root.join(&source.interpreter)))
}

/// The production fetcher: HTTPS through the application's proxy policy, with a size cap.
/// Redirects are not followed; a redirect is an error, so the fetched address is the one that was
/// checked.
pub struct HttpsFetch {
    state_dir: PathBuf,
}

impl HttpsFetch {
    /// A fetcher that reads the proxy policy from the preferences under `state_dir`.
    pub fn new(state_dir: &Path) -> Self {
        Self {
            state_dir: state_dir.to_path_buf(),
        }
    }

    /// The proxy the policy gives for `url`, or `None` for a direct connection.
    fn proxy_for(&self, url: &str) -> Option<ureq::Proxy> {
        let prefs = crate::model_authority::prefs_at(&self.state_dir)?;
        crate::net::proxy::ProxySettings::from_prefs(&prefs).ureq_proxy(url)
    }
}

impl Fetch for HttpsFetch {
    fn fetch(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
        require_https(url)?;
        let mut builder = ureq::AgentBuilder::new()
            .user_agent("Delta-runtime-install")
            .redirects(0)
            .timeout_connect(std::time::Duration::from_secs(15))
            .timeout_read(std::time::Duration::from_secs(120));
        if let Some(proxy) = self.proxy_for(url) {
            builder = builder.proxy(proxy);
        }
        let response = builder
            .build()
            .get(url)
            .call()
            .map_err(|error| format!("the runtime download failed: {error}"))?;
        read_capped(response.into_reader(), max_bytes)
    }
}

/// Read at most `max_bytes` from `reader`; a longer body is refused rather than truncated.
pub fn read_capped(reader: impl std::io::Read, max_bytes: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > max_bytes {
        return Err("the runtime download is larger than its published size".to_string());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const FORMAT: &str = "delta.office-runtime";
    const WORKER: &str = "packages/office_worker/worker.py";

    /// A fetcher that serves one fixed archive and counts how often it was asked.
    struct Served {
        bytes: Vec<u8>,
        calls: Cell<u32>,
    }

    impl Fetch for Served {
        fn fetch(&self, _url: &str, _max_bytes: u64) -> Result<Vec<u8>, String> {
            self.calls.set(self.calls.get() + 1);
            Ok(self.bytes.clone())
        }
    }

    /// A ZIP holding the worker and a manifest that lists it, as `runtime_pack` would write them.
    fn pack_archive(worker_body: &str, manifest_hash_of: &str) -> Vec<u8> {
        let manifest = serde_json::json!({
            "format": FORMAT,
            "version": 1,
            "pack": "office",
            "pack_version": "0.1.0",
            "files": [{
                "path": WORKER,
                "sha256": runtime_pack::sha256_hex(manifest_hash_of.as_bytes()),
                "size": manifest_hash_of.len(),
            }],
        });
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file(WORKER, options).unwrap();
            writer.write_all(worker_body.as_bytes()).unwrap();
            writer.start_file("manifest.json", options).unwrap();
            writer.write_all(manifest.to_string().as_bytes()).unwrap();
            writer.finish().unwrap();
        }
        buffer.into_inner()
    }

    fn source_for(bytes: &[u8]) -> PackSource {
        PackSource {
            name: "office".to_string(),
            version: "0.1.0".to_string(),
            url: "https://packs.example.com/office-0.1.0.zip".to_string(),
            sha256: runtime_pack::sha256_hex(bytes),
            size: bytes.len() as u64,
            format: FORMAT.to_string(),
            interpreter: "python/bin/python3".to_string(),
        }
    }

    fn state() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn a_matching_archive_is_installed_under_its_name_and_version() {
        let temp = state();
        let bytes = pack_archive("print('worker')\n", "print('worker')\n");
        let served = Served {
            bytes: bytes.clone(),
            calls: Cell::new(0),
        };

        let outcome = install(
            temp.path(),
            &source_for(&bytes),
            &OutboundDecision::Allow,
            &served,
        )
        .expect("installed");

        let expected = temp.path().join("runtimes").join("office-0.1.0");
        assert_eq!(outcome, InstallOutcome::Installed(expected.clone()));
        assert!(expected.join(WORKER).is_file());
        assert!(runtime_pack::pack_problems(&expected, FORMAT).is_empty());
        assert_eq!(served.calls.get(), 1);
    }

    #[test]
    fn a_second_install_of_an_intact_copy_fetches_nothing() {
        let temp = state();
        let bytes = pack_archive("x = 1\n", "x = 1\n");
        let source = source_for(&bytes);
        let served = Served {
            bytes,
            calls: Cell::new(0),
        };
        install(temp.path(), &source, &OutboundDecision::Allow, &served).unwrap();

        let again = install(temp.path(), &source, &OutboundDecision::Allow, &served).unwrap();

        assert!(matches!(again, InstallOutcome::AlreadyInstalled(_)));
        assert_eq!(served.calls.get(), 1);
    }

    #[test]
    fn a_damaged_copy_is_replaced_from_a_fresh_archive() {
        let temp = state();
        let bytes = pack_archive("x = 1\n", "x = 1\n");
        let source = source_for(&bytes);
        let served = Served {
            bytes,
            calls: Cell::new(0),
        };
        let first = install(temp.path(), &source, &OutboundDecision::Allow, &served).unwrap();
        let InstallOutcome::Installed(root) = first else {
            panic!("first install")
        };
        std::fs::write(root.join(WORKER), "import os\n").unwrap();

        let repaired = install(temp.path(), &source, &OutboundDecision::Allow, &served).unwrap();

        assert!(matches!(repaired, InstallOutcome::Installed(_)));
        assert_eq!(served.calls.get(), 2);
        assert!(runtime_pack::pack_problems(&root, FORMAT).is_empty());
    }

    #[test]
    fn a_decision_other_than_allow_fetches_nothing() {
        for decision in [
            OutboundDecision::Ask,
            OutboundDecision::NotChosen,
            OutboundDecision::Deny("refused".to_string()),
        ] {
            let temp = state();
            let bytes = pack_archive("x = 1\n", "x = 1\n");
            let served = Served {
                bytes: bytes.clone(),
                calls: Cell::new(0),
            };

            let result = install(temp.path(), &source_for(&bytes), &decision, &served);

            assert!(result.is_err(), "decision {decision:?}");
            assert_eq!(served.calls.get(), 0);
            assert!(!temp.path().join("runtimes").exists());
        }
    }

    #[test]
    fn a_digest_mismatch_leaves_nothing_behind() {
        let temp = state();
        let bytes = pack_archive("x = 1\n", "x = 1\n");
        let mut source = source_for(&bytes);
        source.sha256 = "0".repeat(64);
        let served = Served {
            bytes,
            calls: Cell::new(0),
        };

        let result = install(temp.path(), &source, &OutboundDecision::Allow, &served);

        assert!(result.unwrap_err().contains("digest"));
        assert!(!temp.path().join("runtimes").join("office-0.1.0").exists());
    }

    #[test]
    fn a_size_different_from_the_published_one_is_refused() {
        let temp = state();
        let bytes = pack_archive("x = 1\n", "x = 1\n");
        let mut source = source_for(&bytes);
        source.size += 1;
        let served = Served {
            bytes,
            calls: Cell::new(0),
        };

        assert!(install(temp.path(), &source, &OutboundDecision::Allow, &served).is_err());
    }

    #[test]
    fn a_file_that_does_not_match_the_manifest_is_refused_and_removed() {
        let temp = state();
        // The archive is intact, but the manifest describes different worker bytes.
        let bytes = pack_archive("print('changed')\n", "print('original')\n");
        let served = Served {
            bytes: bytes.clone(),
            calls: Cell::new(0),
        };

        let result = install(
            temp.path(),
            &source_for(&bytes),
            &OutboundDecision::Allow,
            &served,
        );

        assert!(result.unwrap_err().contains("hash mismatch"));
        let runtimes = temp.path().join("runtimes");
        assert!(!runtimes.join("office-0.1.0").exists());
        assert!(!runtimes.join(".staging-office-0.1.0").exists());
    }

    #[test]
    fn an_entry_that_would_leave_the_pack_is_refused() {
        let temp = state();
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("../escape.py", options).unwrap();
            writer.write_all(b"x = 1\n").unwrap();
            writer.finish().unwrap();
        }
        let bytes = buffer.into_inner();
        let served = Served {
            bytes: bytes.clone(),
            calls: Cell::new(0),
        };

        let result = install(
            temp.path(),
            &source_for(&bytes),
            &OutboundDecision::Allow,
            &served,
        );

        assert!(result.is_err());
        assert!(!temp.path().join("escape.py").exists());
        assert!(!temp.path().join("runtimes").join("escape.py").exists());
    }

    #[test]
    fn only_an_https_address_with_a_host_is_accepted() {
        assert!(require_https("https://packs.example.com/a.zip").is_ok());
        assert!(require_https("http://packs.example.com/a.zip").is_err());
        assert!(require_https("file:///etc/passwd").is_err());
        assert!(require_https("https://").is_err());
        assert!(require_https("not a url").is_err());
    }

    #[test]
    fn names_and_versions_cannot_escape_the_runtimes_folder() {
        let temp = state();
        let bytes = pack_archive("x = 1\n", "x = 1\n");
        let served = Served {
            bytes: bytes.clone(),
            calls: Cell::new(0),
        };

        for (name, version) in [
            ("../evil", "0.1.0"),
            ("office", "../0.1.0"),
            ("Office", "0.1.0"),
            ("office", "0.1"),
        ] {
            let mut source = source_for(&bytes);
            source.name = name.to_string();
            source.version = version.to_string();
            let result = install(temp.path(), &source, &OutboundDecision::Allow, &served);
            assert!(result.is_err(), "{name} {version}");
        }
        assert_eq!(served.calls.get(), 0);
    }

    /// A ZIP of `entries` with a manifest of `format` that lists each of them.
    fn archive_of(format: &str, entries: &[(&str, &str)]) -> Vec<u8> {
        let files: Vec<serde_json::Value> = entries
            .iter()
            .map(|(path, body)| {
                json!({"path": path, "sha256": runtime_pack::sha256_hex(body.as_bytes()), "size": body.len()})
            })
            .collect();
        let manifest = json!({"format": format, "version": 1, "files": files});
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default();
            for (path, body) in entries {
                writer.start_file(*path, options).unwrap();
                writer.write_all(body.as_bytes()).unwrap();
            }
            writer.start_file("manifest.json", options).unwrap();
            writer.write_all(manifest.to_string().as_bytes()).unwrap();
            writer.finish().unwrap();
        }
        buffer.into_inner()
    }

    fn shared_source_for(bytes: &[u8]) -> PackSource {
        PackSource {
            name: PYTHON_PACK_NAME.to_string(),
            version: "0.1.0".to_string(),
            url: "https://packs.example.com/delta-python-0.1.0.zip".to_string(),
            sha256: runtime_pack::sha256_hex(bytes),
            size: bytes.len() as u64,
            format: PYTHON_PACK_FORMAT.to_string(),
            interpreter: "python/bin/python3".to_string(),
        }
    }

    #[test]
    fn nothing_is_fetched_until_the_user_approves_that_exact_pack() {
        let temp = state();
        let bytes = pack_archive("x = 1\n", "x = 1\n");
        let source = source_for(&bytes);
        let served = Served {
            bytes,
            calls: Cell::new(0),
        };

        assert!(install_if_approved(temp.path(), &source, &served).is_err());
        assert_eq!(served.calls.get(), 0);

        record_approval(temp.path(), &source, "2026-10-27T13:30:00Z").unwrap();
        assert!(is_approved(temp.path(), &source));
        let outcome = install_if_approved(temp.path(), &source, &served).unwrap();
        assert!(matches!(outcome, InstallOutcome::Installed(_)));
        assert_eq!(served.calls.get(), 1);
    }

    #[test]
    fn an_approval_does_not_cover_a_different_digest() {
        let temp = state();
        let bytes = pack_archive("x = 1\n", "x = 1\n");
        let source = source_for(&bytes);
        record_approval(temp.path(), &source, "2026-10-27T13:30:00Z").unwrap();

        let mut changed = source.clone();
        changed.sha256 = "1".repeat(64);

        assert!(!is_approved(temp.path(), &changed));
    }

    #[test]
    fn an_approval_and_a_refusal_are_written_to_the_audit_log() {
        let temp = state();
        let bytes = pack_archive("x = 1\n", "x = 1\n");
        let source = source_for(&bytes);

        record_refusal(temp.path(), &source);
        record_approval(temp.path(), &source, "2026-10-27T13:30:00Z").unwrap();

        assert!(temp.path().join("audit_events.db").is_file());
    }

    #[test]
    fn a_body_at_the_cap_is_read_and_one_byte_more_is_refused() {
        let body = vec![7_u8; 10];

        assert_eq!(read_capped(&body[..], 10).unwrap(), body);
        assert!(read_capped(&body[..], 9).unwrap_err().contains("larger"));
    }

    #[test]
    fn the_shared_pack_is_found_only_when_an_intact_copy_is_in_place() {
        let temp = state();
        let worker = "print('explore')\n";
        let interpreter = "#!/bin/sh\n";
        let bytes = shared_archive(&[
            ("packages/explore_pack/worker.py", worker),
            ("python/bin/python3", interpreter),
        ]);
        let source = shared_source_for(&bytes);
        assert!(installed_python_pack(temp.path(), &source).is_none());

        let served = Served {
            bytes,
            calls: Cell::new(0),
        };
        install_if_approved(temp.path(), &source, &served).ok();
        record_approval(temp.path(), &source, "2026-10-27T13:30:00Z").unwrap();
        install_if_approved(temp.path(), &source, &served).unwrap();

        let (root, python) = installed_python_pack(temp.path(), &source).expect("installed");
        assert_eq!(python, root.join("python/bin/python3"));
        std::fs::write(root.join("packages/explore_pack/worker.py"), "import os\n").unwrap();
        assert!(installed_python_pack(temp.path(), &source).is_none());
    }

    /// A ZIP whose manifest is of the shared format and lists `entries`.
    fn shared_archive(entries: &[(&str, &str)]) -> Vec<u8> {
        archive_of(PYTHON_PACK_FORMAT, entries)
    }

    #[test]
    fn the_catalogue_publishes_nothing_until_a_pack_is_listed() {
        assert!(published_python_pack().is_none());
    }

    #[test]
    fn the_production_fetcher_refuses_a_plain_http_address_before_connecting() {
        let temp = state();
        let fetch = HttpsFetch::new(temp.path());

        assert!(fetch.fetch("http://packs.example.com/a.zip", 10).is_err());
    }
}
