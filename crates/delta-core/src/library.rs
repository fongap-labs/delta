//! Rust Application Control Plane authority for the source library (资料库).
//!
//! The library is a user-chosen folder of plain source files (Markdown and
//! text) that Delta keeps as its corpus — the model's own NotebookLM-style
//! "sources". The folder location is customizable and persisted in
//! `library.json` under the state directory; the sources themselves stay
//! ordinary files in that folder, so the user can edit them with any tool and
//! sync the folder like any other documents. Everything here is Rust-owned:
//! the frontend goes through the control plane and never touches the folder
//! directly.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};

use crate::capability::{
    CapabilityGrants, CapabilityJob, CapabilityRegistration, CapabilityResult,
    NativeCapabilityRunner,
};
use crate::{durability::atomic_write, ShadowReadError};

/// Source file extensions the library manages. Anything else in the folder is
/// left alone by `list` (and never deleted by `remove`).
const SOURCE_EXTENSIONS: [&str; 3] = ["md", "markdown", "txt"];
/// Longest source file name, in characters.
const NAME_MAX_CHARS: usize = 120;
/// Longest source body accepted by `write`, in characters.
const CONTENT_MAX_CHARS: usize = 2_000_000;
/// A source file larger than this is skipped by search (it would dominate the
/// scan; the user can still read it with `read_library`).
const SEARCH_FILE_MAX_BYTES: u64 = 2 * 1024 * 1024;
/// Longest one line fragment handed back per match, in characters.
const SEARCH_SNIPPET_MAX_CHARS: usize = 240;
/// Default and maximum number of matches one search returns.
const SEARCH_DEFAULT_LIMIT: usize = 20;
const SEARCH_MAX_LIMIT: usize = 50;
/// Default and maximum characters `read_library` returns from one source.
const READ_DEFAULT_CHARS: usize = 20_000;
const READ_MAX_CHARS: usize = 100_000;

pub struct LibraryStore {
    config_path: PathBuf,
    default_folder: PathBuf,
}

impl LibraryStore {
    pub fn open(state_dir: impl AsRef<Path>) -> Result<Self, ShadowReadError> {
        let state_dir = state_dir.as_ref();
        std::fs::create_dir_all(state_dir)?;
        let store = Self {
            config_path: state_dir.join("library.json"),
            default_folder: state_dir.join("library"),
        };
        // The default folder exists from the first run: an empty library is a
        // usable library, not a setup step.
        std::fs::create_dir_all(&store.default_folder)?;
        Ok(store)
    }

    /// The folder the corpus lives in: the persisted location or the default
    /// under the state directory. A stored location that no longer exists is
    /// recreated lazily here, so a moved or detached drive heals on next use
    /// instead of failing every call.
    pub fn folder(&self) -> PathBuf {
        match self.read_config() {
            Ok(folder) if !folder.as_os_str().is_empty() => folder,
            _ => self.default_folder.clone(),
        }
    }

    /// Point the library at a different folder. The folder is created when it
    /// does not exist; existing sources stay on disk and simply no longer show
    /// here until the location is switched back.
    pub fn set_folder(&self, path: &str) -> Result<Value, ShadowReadError> {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Ok(json!({"ok": false, "error": "a folder path is required"}));
        }
        let folder = PathBuf::from(trimmed);
        std::fs::create_dir_all(&folder)?;
        let payload = json!({ "folder": folder.to_string_lossy() });
        let bytes = serde_json::to_vec_pretty(&payload)?;
        atomic_write(&self.config_path, &bytes)?;
        Ok(json!({"ok": true, "folder": folder.to_string_lossy()}))
    }

    /// Every managed source in the folder: name, size and modification time.
    /// Subfolders and foreign files are ignored.
    pub fn list(&self) -> Result<Vec<Value>, ShadowReadError> {
        let folder = self.folder();
        let mut sources = Vec::new();
        let entries = match std::fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) => {
                return Err(ShadowReadError::Parse(format!(
                    "failed to read the library folder {}: {error}",
                    folder.display()
                )))
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_source = entry
                .file_name()
                .to_str()
                .map(|name| {
                    path.is_file()
                        && SOURCE_EXTENSIONS.contains(
                            &name
                                .rsplit('.')
                                .next()
                                .unwrap_or_default()
                                .to_ascii_lowercase()
                                .as_str(),
                        )
                        && name.contains('.')
                })
                .unwrap_or(false);
            if !is_source {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            let modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs_f64());
            sources.push(json!({
                "name": entry.file_name().to_string_lossy(),
                "size": metadata.len(),
                "modified": modified,
            }));
        }
        sources.sort_by(|a, b| {
            a["name"]
                .as_str()
                .unwrap_or_default()
                .to_lowercase()
                .cmp(&b["name"].as_str().unwrap_or_default().to_lowercase())
        });
        Ok(sources)
    }

    /// One source's body, addressed by its file name in the folder.
    pub fn read(&self, name: &str) -> Result<Value, ShadowReadError> {
        let name = canonical_source_name(name)?;
        let path = self.folder().join(&name);
        let content = std::fs::read_to_string(&path).map_err(|error| {
            ShadowReadError::Parse(format!("failed to read source {name}: {error}"))
        })?;
        Ok(json!({"ok": true, "name": name, "content": content}))
    }

    /// Create or update one source. A name without an extension becomes a
    /// Markdown file; the body is capped so one paste cannot grow unbounded.
    pub fn write(&self, name: &str, content: &str) -> Result<Value, ShadowReadError> {
        let name = canonical_source_name(name)?;
        let truncated: String = content.chars().take(CONTENT_MAX_CHARS).collect();
        let path = self.folder().join(&name);
        let bytes = truncated.into_bytes();
        atomic_write(&path, &bytes)?;
        Ok(json!({"ok": true, "name": name}))
    }

    /// Delete one managed source. A name that is not a managed source file
    /// (wrong extension, traversal, hidden) is refused silently as "nothing
    /// deleted" — a stray binary in the folder is never removed through this
    /// authority.
    pub fn remove(&self, name: &str) -> Result<Value, ShadowReadError> {
        let name = match canonical_source_name(name) {
            Ok(name) => name,
            Err(_) => return Ok(json!({"ok": true, "deleted": false})),
        };
        let path = self.folder().join(&name);
        if !path.exists() {
            return Ok(json!({"ok": true, "deleted": false}));
        }
        std::fs::remove_file(&path).map_err(|error| {
            ShadowReadError::Parse(format!("failed to delete source {name}: {error}"))
        })?;
        Ok(json!({"ok": true, "deleted": true}))
    }

    fn read_config(&self) -> Result<PathBuf, ShadowReadError> {
        if !self.config_path.exists() {
            return Ok(PathBuf::new());
        }
        let text = std::fs::read_to_string(&self.config_path)?;
        let saved: Value = serde_json::from_str(&text).map_err(|error| {
            ShadowReadError::Parse(format!(
                "invalid library authority {}: {error}",
                self.config_path.display()
            ))
        })?;
        let raw = saved
            .get("folder")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        Ok(PathBuf::from(raw))
    }
}

/// The model-facing search over the corpus: case-insensitive line matches
/// across the library's sources, capped per call. Reading the user's own
/// curated corpus is low risk and auto-approved; the corpus never leaves the
/// machine through this tool.
pub fn search_registration(store: Arc<LibraryStore>) -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "library.search".to_owned(),
        tool_name: "search_library".to_owned(),
        description: "Search the user's source library (资料库) — the curated corpus of notes and \
            documents kept for this product. Use it to ground answers in the user's own material: \
            it returns matching lines with their source file names."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["query"],
            "properties": {
                "query": {"type": "string", "description": "Text to look for (case-insensitive)."},
                "limit": {"type": "integer", "description": "Maximum matches to return (default 20, max 50)."}
            }
        }),
        metadata: json!({
            "risk_level": "low", "requires_approval": false,
            "category": "filesystem", "capabilities": ["library.search"]
        }),
        side_effect: crate::skills::SideEffectClass::Read,
        grants: CapabilityGrants::default(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(move |job, control| {
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            run_search(&store, job)
        })),
    }
}

/// The model-facing read of one corpus source, bounded per call so a single
/// file cannot flood the context.
pub fn read_registration(store: Arc<LibraryStore>) -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "library.read".to_owned(),
        tool_name: "read_library".to_owned(),
        description: "Read one source from the user's source library (资料库) by name, as listed \
            by search_library. Returns the file's text from the start, bounded."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["name"],
            "properties": {
                "name": {"type": "string", "description": "The source file name (as search_library reports it)."},
                "max_chars": {"type": "integer", "description": "Maximum characters to return (default 20000, max 100000)."}
            }
        }),
        metadata: json!({
            "risk_level": "low", "requires_approval": false,
            "category": "filesystem", "capabilities": ["library.read"]
        }),
        side_effect: crate::skills::SideEffectClass::Read,
        grants: CapabilityGrants::default(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(move |job, control| {
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            run_read(&store, job)
        })),
    }
}

/// Search every source for the query; stop collecting at the limit but keep
/// counting the rest so the model knows the list is not exhaustive.
fn run_search(store: &LibraryStore, job: &CapabilityJob) -> CapabilityResult {
    let Some(query) = job
        .arguments
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .map(str::to_string)
    else {
        return CapabilityResult::failed(&job.job_id, "query is required", Some("arguments"));
    };
    let limit = job
        .arguments
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value.clamp(1, SEARCH_MAX_LIMIT as u64) as usize)
        .unwrap_or(SEARCH_DEFAULT_LIMIT);
    let needle = query.to_lowercase();
    let sources = match store.list() {
        Ok(sources) => sources,
        Err(error) => {
            return CapabilityResult::failed(&job.job_id, &error.to_string(), Some("library_scan"))
        }
    };
    let mut matches: Vec<Value> = Vec::new();
    let mut remaining = limit;
    let mut files_searched = 0usize;
    let mut is_truncated = false;
    for source in &sources {
        let Some(name) = source.get("name").and_then(Value::as_str) else {
            continue;
        };
        let path = store.folder().join(name);
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if metadata.len() > SEARCH_FILE_MAX_BYTES {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        files_searched += 1;
        for (index, line) in content.lines().enumerate() {
            if !line.to_lowercase().contains(&needle) {
                continue;
            }
            if remaining == 0 {
                is_truncated = true;
                break;
            }
            let snippet: String = line.trim().chars().take(SEARCH_SNIPPET_MAX_CHARS).collect();
            matches.push(json!({
                "source": name,
                "line": index + 1,
                "text": snippet,
            }));
            remaining -= 1;
        }
        if is_truncated {
            break;
        }
    }
    CapabilityResult::completed(
        &job.job_id,
        json!({
            "ok": true,
            "query": query,
            "matches": matches,
            "files_searched": files_searched,
            "truncated": is_truncated,
        }),
    )
}

/// Read one source from the start, bounded, with a flag when characters were
/// cut so the model knows to ask again with an offset mindset.
fn run_read(store: &LibraryStore, job: &CapabilityJob) -> CapabilityResult {
    let Some(name) = job
        .arguments
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
    else {
        return CapabilityResult::failed(&job.job_id, "name is required", Some("arguments"));
    };
    let max_chars = job
        .arguments
        .get("max_chars")
        .and_then(Value::as_u64)
        .map(|value| value.clamp(1, READ_MAX_CHARS as u64) as usize)
        .unwrap_or(READ_DEFAULT_CHARS);
    match store.read(&name) {
        Ok(value) => {
            let content = value.get("content").and_then(Value::as_str).unwrap_or("");
            let truncated = content.chars().count() > max_chars;
            let bounded: String = content.chars().take(max_chars).collect();
            CapabilityResult::completed(
                &job.job_id,
                json!({
                    "ok": true,
                    "name": name,
                    "content": bounded,
                    "truncated": truncated,
                }),
            )
        }
        Err(error) => {
            CapabilityResult::failed(&job.job_id, &error.to_string(), Some("library_read"))
        }
    }
}

/// A source file name that is safe to join onto the library folder: plain
/// characters, a managed extension, no traversal and no hidden dotfiles. A
/// name without any extension becomes a Markdown file.
fn canonical_source_name(name: &str) -> Result<String, ShadowReadError> {
    let trimmed = name.trim();
    if !trimmed.contains('.') {
        return safe_source_name(&format!("{trimmed}.md"));
    }
    safe_source_name(trimmed)
}

fn safe_source_name(name: &str) -> Result<String, ShadowReadError> {
    let trimmed = name.trim();
    let plain = !trimmed.is_empty()
        && trimmed.chars().count() <= NAME_MAX_CHARS
        && trimmed.contains('.')
        && !trimmed.starts_with('.')
        && trimmed
            .chars()
            .all(|c| !c.is_control() && !matches!(c, '/' | '\\' | ':'));
    if !plain {
        return Err(ShadowReadError::Parse(format!(
            "{trimmed} is not an allowed source file name"
        )));
    }
    let extension = trimmed
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !SOURCE_EXTENSIONS.contains(&extension.as_str()) {
        return Err(ShadowReadError::Parse(format!(
            "{trimmed} must be a Markdown or text file (md, markdown, txt)"
        )));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilityControl;

    #[test]
    fn the_default_folder_is_created_and_sources_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let store = LibraryStore::open(temp.path()).unwrap();
        assert!(
            store.folder().exists(),
            "default folder exists from first run"
        );

        assert_eq!(
            store.write("notes", "# hello").unwrap()["name"],
            json!("notes.md"),
            "a name without an extension becomes Markdown"
        );
        let sources = store.list().unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0]["name"], "notes.md");
        assert_eq!(store.read("notes.md").unwrap()["content"], "# hello");

        assert_eq!(store.write("notes.md", "# updated").unwrap()["ok"], true);
        assert_eq!(store.read("notes.md").unwrap()["content"], "# updated");
        assert_eq!(store.remove("notes.md").unwrap()["deleted"], true);
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn unsafe_source_names_are_refused() {
        let temp = tempfile::tempdir().unwrap();
        let store = LibraryStore::open(temp.path()).unwrap();
        for bad in [
            "../escape.md",
            "a/b.md",
            "a\\b.md",
            "a:b.md",
            ".hidden.md",
            "wrong.tar",
            "",
        ] {
            assert!(store.write(bad, "x").is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn the_folder_location_is_customizable_and_persisted() {
        let temp = tempfile::tempdir().unwrap();
        let store = LibraryStore::open(temp.path()).unwrap();
        let custom = temp.path().join("corpus").join("delta");
        assert_eq!(
            store.set_folder(custom.to_str().unwrap()).unwrap()["ok"],
            true
        );
        assert!(custom.exists(), "a new location is created");
        assert_eq!(store.folder(), custom);

        store.write("doc.md", "content").unwrap();
        let sources = store.list().unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0]["name"], "doc.md");
        assert_eq!(store.read("doc.md").unwrap()["content"], "content");

        // The location survives a fresh open of the authority.
        let reopened = LibraryStore::open(temp.path()).unwrap();
        assert_eq!(reopened.folder(), custom);
        assert_eq!(reopened.list().unwrap().len(), 1);
    }

    #[test]
    fn a_blank_folder_location_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let store = LibraryStore::open(temp.path()).unwrap();
        assert_eq!(store.set_folder("   ").unwrap()["ok"], false);
    }

    #[test]
    fn list_ignores_subfolders_and_foreign_files() {
        let temp = tempfile::tempdir().unwrap();
        let store = LibraryStore::open(temp.path()).unwrap();
        let folder = store.folder();
        std::fs::write(folder.join("note.md"), b"text").unwrap();
        std::fs::write(folder.join("image.png"), b"binary").unwrap();
        std::fs::create_dir_all(folder.join("nested")).unwrap();
        std::fs::write(folder.join("nested/inner.md"), b"deep").unwrap();

        let sources = store.list().unwrap();
        assert_eq!(sources.len(), 1, "only the top-level managed file shows");
        assert_eq!(sources[0]["name"], "note.md");
        assert_eq!(store.remove("image.png").unwrap()["deleted"], false);
        assert!(
            folder.join("image.png").exists(),
            "foreign files are never deleted"
        );
    }

    fn tool_control() -> CapabilityControl {
        use std::sync::atomic::AtomicBool;
        let progress: Arc<dyn Fn(crate::capability::CapabilityProgress) + Send + Sync> =
            Arc::new(|_| {});
        CapabilityControl::new(Arc::new(AtomicBool::new(false)), progress)
    }

    fn tool_job(arguments: Value) -> CapabilityJob {
        CapabilityJob::new("library.search", "job-library-test", arguments)
    }

    fn library_with_sources() -> (tempfile::TempDir, Arc<LibraryStore>) {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(LibraryStore::open(temp.path()).unwrap());
        store
            .write(
                "rust-notes.md",
                "# Rust\nRust ships fearless concurrency.\nRust compiles fast.\n",
            )
            .unwrap();
        store
            .write("design.md", "The design favors plain text files.\n")
            .unwrap();
        (temp, store)
    }

    #[test]
    fn search_returns_matching_lines_with_their_sources() {
        let (_temp, store) = library_with_sources();
        let runner = search_registration(store);
        let result = runner
            .runner
            .run(&tool_job(json!({"query": "RUST"})), &tool_control());
        assert_eq!(
            result.state,
            crate::capability::CapabilityExitState::Completed
        );
        let output = result.result.expect("completed output");
        assert_eq!(output["ok"], true);
        let matches = output["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 3, "case-insensitive across both lines");
        assert_eq!(matches[0]["source"], "rust-notes.md");
        assert_eq!(matches[0]["line"], json!(1));
        assert!(matches[0]["text"].as_str().unwrap().contains("Rust"));
        assert_eq!(output["files_searched"], json!(2));
    }

    #[test]
    fn search_respects_the_limit_and_reports_truncation() {
        let (_temp, store) = library_with_sources();
        let runner = search_registration(store);
        let result = runner.runner.run(
            &tool_job(json!({"query": "rust", "limit": 1})),
            &tool_control(),
        );
        let output = result.result.expect("completed output");
        assert_eq!(output["matches"].as_array().unwrap().len(), 1);
        assert_eq!(output["truncated"], json!(true), "more matches existed");
    }

    #[test]
    fn search_refuses_an_empty_query() {
        let (_temp, store) = library_with_sources();
        let runner = search_registration(store);
        let result = runner
            .runner
            .run(&tool_job(json!({"query": "   "})), &tool_control());
        assert_eq!(result.state, crate::capability::CapabilityExitState::Failed);
        let diagnostics = result.diagnostics.expect("failure carries diagnostics");
        assert_eq!(diagnostics.error_code.as_deref(), Some("arguments"));
    }

    #[test]
    fn read_returns_a_bounded_source_body() {
        let (_temp, store) = library_with_sources();
        let runner = read_registration(store);
        let result = runner.runner.run(
            &tool_job(json!({"name": "rust-notes.md", "max_chars": 10})),
            &tool_control(),
        );
        let output = result.result.expect("completed output");
        assert_eq!(output["ok"], true);
        assert_eq!(output["truncated"], json!(true));
        assert!(output["content"].as_str().unwrap().chars().count() <= 10);
    }

    #[test]
    fn read_reports_a_missing_source_as_a_failure() {
        let (_temp, store) = library_with_sources();
        let runner = read_registration(store);
        let result = runner
            .runner
            .run(&tool_job(json!({"name": "missing.md"})), &tool_control());
        assert_eq!(result.state, crate::capability::CapabilityExitState::Failed);
        let diagnostics = result.diagnostics.expect("failure carries diagnostics");
        assert_eq!(diagnostics.error_code.as_deref(), Some("library_read"));
    }

    #[test]
    fn the_library_tools_are_low_risk_and_auto_approved() {
        let (_temp, store) = library_with_sources();
        for registration in [search_registration(store.clone()), read_registration(store)] {
            assert_eq!(registration.metadata["risk_level"], json!("low"));
            assert_eq!(registration.metadata["requires_approval"], json!(false));
            assert!(!registration.grants.exec);
            assert!(!registration.workspace_write);
        }
    }
}
