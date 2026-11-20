//! The local library (roadmap B1): a full-text index of the folders the user chose to put in it.
//!
//! SQLite FTS5 does the search, so there are no extra dependencies, no network, and nothing that
//! `private_only` could object to. The index lives in `library.db` in the state directory and is
//! made only from files the user pointed Delta at; a search is limited to the folders the
//! session may read. Every hit names its file, where in it, the SHA-256 the file had when it was
//! indexed, and the index version, and says when the file has changed since.
//!
//! The text is indexed with the `trigram` tokenizer, which matches any run of three or more
//! characters in any language, Chinese included. A query term of fewer than three characters can
//! not use it and is answered by a plain substring scan instead.
//!
//! Not here: a vector (semantic) layer. It needs an embedding endpoint and the egress judgement
//! that goes with one, and is left for a decision of its own.

mod extract;
pub mod tools;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{params, params_from_iter, Connection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// Folders that are never indexed, wherever they are.
const SKIPPED_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "__pycache__",
    "dist",
    "build",
    "venv",
    "site-packages",
];
/// Files committed to the database in one transaction.
const BATCH_FILES: usize = 100;
const DEFAULT_LIMIT: usize = 8;
const MAX_LIMIT: usize = 20;
const MAX_TERMS: usize = 8;
const MAX_TERM_CHARS: usize = 64;
/// Hits from one file in one answer, so one long document does not crowd out the others.
const MAX_HITS_PER_FILE: usize = 3;
const SNIPPET_CHARS: usize = 240;

/// Called once a root has been indexed, with the report; the control plane uses it to write the
/// `source.indexed` ledger event.
pub type IndexedHook = Arc<dyn Fn(&IndexReport) + Send + Sync>;

pub struct Library {
    path: PathBuf,
    on_indexed: Option<IndexedHook>,
}

/// What one indexing run did.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexReport {
    pub root: String,
    /// The root's index version after the run; it rises only when something changed.
    pub version: i64,
    pub files_seen: usize,
    pub files_indexed: usize,
    pub files_unchanged: usize,
    pub files_skipped: usize,
    pub files_removed: usize,
    pub chunks: usize,
    /// False when the time budget ran out; run again to continue where it stopped.
    pub is_complete: bool,
    pub seconds: f64,
}

impl IndexReport {
    pub fn to_json(&self) -> Value {
        json!({
            "root": self.root, "version": self.version, "files_seen": self.files_seen,
            "files_indexed": self.files_indexed, "files_unchanged": self.files_unchanged,
            "files_skipped": self.files_skipped, "files_removed": self.files_removed,
            "chunks": self.chunks, "is_complete": self.is_complete, "seconds": self.seconds,
        })
    }
}

/// One search hit.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub root: String,
    pub path: String,
    pub locator: Value,
    pub snippet: String,
    pub sha256: String,
    pub version: i64,
    /// The file is gone or has a different size or time than when it was indexed.
    pub is_stale: bool,
}

impl Hit {
    pub fn to_json(&self) -> Value {
        json!({
            "root": self.root, "path": self.path, "locator": self.locator,
            "snippet": self.snippet, "sha256": self.sha256, "index_version": self.version,
            "stale": self.is_stale,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchOutcome {
    pub hits: Vec<Hit>,
    /// Folders the session may read that are not in the library.
    pub not_indexed: Vec<String>,
}

/// One folder of the library, as the settings page lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct RootInfo {
    pub root: String,
    pub version: i64,
    pub indexed_at: Option<String>,
    pub files: i64,
    pub chunks: i64,
    pub skipped: i64,
}

impl RootInfo {
    pub fn to_json(&self) -> Value {
        json!({
            "root": self.root, "version": self.version, "indexed_at": self.indexed_at,
            "files": self.files, "chunks": self.chunks, "skipped": self.skipped,
        })
    }
}

fn db_error(error: rusqlite::Error) -> String {
    format!("the library index failed: {error}")
}

impl Library {
    pub fn open(state_dir: impl AsRef<Path>) -> Result<Self, String> {
        let library = Self {
            path: state_dir.as_ref().join("library.db"),
            on_indexed: None,
        };
        library.connection()?;
        Ok(library)
    }

    pub fn with_hook(mut self, hook: IndexedHook) -> Self {
        self.on_indexed = Some(hook);
        self
    }

    fn connection(&self) -> Result<Connection, String> {
        let conn = Connection::open(&self.path).map_err(db_error)?;
        conn.busy_timeout(Duration::from_secs(30))
            .map_err(db_error)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS roots (
                 root TEXT PRIMARY KEY,
                 version INTEGER NOT NULL DEFAULT 0,
                 indexed_at TEXT
             );
             CREATE TABLE IF NOT EXISTS files (
                 id INTEGER PRIMARY KEY,
                 root TEXT NOT NULL,
                 path TEXT NOT NULL,
                 sha256 TEXT NOT NULL,
                 size INTEGER NOT NULL,
                 mtime_ns INTEGER NOT NULL,
                 kind TEXT NOT NULL,
                 version INTEGER NOT NULL,
                 status TEXT NOT NULL,
                 note TEXT,
                 UNIQUE (root, path)
             );
             CREATE TABLE IF NOT EXISTS chunks (
                 id INTEGER PRIMARY KEY,
                 file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                 ordinal INTEGER NOT NULL,
                 locator TEXT NOT NULL,
                 text TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS chunks_by_file ON chunks(file_id);
             CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
                 text, content = 'chunks', content_rowid = 'id', tokenize = 'trigram'
             );
             CREATE TRIGGER IF NOT EXISTS chunks_after_insert AFTER INSERT ON chunks BEGIN
                 INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
             END;
             CREATE TRIGGER IF NOT EXISTS chunks_after_delete AFTER DELETE ON chunks BEGIN
                 INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
             END;",
        )
        .map_err(db_error)?;
        Ok(conn)
    }

    /// The folders in the library.
    pub fn roots(&self) -> Result<Vec<RootInfo>, String> {
        let conn = self.connection()?;
        let mut statement = conn
            .prepare(
                "SELECT r.root, r.version, r.indexed_at,
                        (SELECT COUNT(*) FROM files f WHERE f.root = r.root AND f.status = 'indexed'),
                        (SELECT COUNT(*) FROM chunks c JOIN files f ON f.id = c.file_id WHERE f.root = r.root),
                        (SELECT COUNT(*) FROM files f WHERE f.root = r.root AND f.status = 'skipped')
                 FROM roots r ORDER BY r.root",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(RootInfo {
                    root: row.get(0)?,
                    version: row.get(1)?,
                    indexed_at: row.get(2)?,
                    files: row.get(3)?,
                    chunks: row.get(4)?,
                    skipped: row.get(5)?,
                })
            })
            .map_err(db_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
    }

    /// Take a folder out of the library. Returns whether it was there.
    pub fn remove_root(&self, root: &str) -> Result<bool, String> {
        let root = canonical(root)?;
        let mut conn = self.connection()?;
        let transaction = conn.transaction().map_err(db_error)?;
        transaction
            .execute(
                "DELETE FROM chunks WHERE file_id IN (SELECT id FROM files WHERE root = ?1)",
                params![root],
            )
            .map_err(db_error)?;
        transaction
            .execute("DELETE FROM files WHERE root = ?1", params![root])
            .map_err(db_error)?;
        let removed = transaction
            .execute("DELETE FROM roots WHERE root = ?1", params![root])
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)?;
        Ok(removed > 0)
    }

    /// Index `root`, reading only what changed since the last run. Stops cleanly at `budget` or
    /// when `is_cancelled` says so; the next run carries on. Files that vanished are dropped from
    /// the index only by a run that saw the whole folder.
    pub fn index_root(
        &self,
        root: &str,
        budget: Duration,
        is_cancelled: &dyn Fn() -> bool,
        on_progress: &dyn Fn(usize, usize),
    ) -> Result<IndexReport, String> {
        let started = Instant::now();
        let root = canonical(root)?;
        let root_path = PathBuf::from(&root);
        if !root_path.is_dir() {
            return Err(format!("{root} is not a folder"));
        }
        let mut conn = self.connection()?;
        conn.execute(
            "INSERT OR IGNORE INTO roots(root, version) VALUES (?1, 0)",
            params![root],
        )
        .map_err(db_error)?;
        let current_version: i64 = conn
            .query_row(
                "SELECT version FROM roots WHERE root = ?1",
                params![root],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        let next_version = current_version + 1;

        let candidates = walk(&root_path, is_cancelled);
        let mut report = IndexReport {
            root: root.clone(),
            version: current_version,
            files_seen: candidates.len(),
            files_indexed: 0,
            files_unchanged: 0,
            files_skipped: 0,
            files_removed: 0,
            chunks: 0,
            is_complete: true,
            seconds: 0.0,
        };
        let mut seen: HashSet<String> = HashSet::with_capacity(candidates.len());
        let mut transaction = conn.transaction().map_err(db_error)?;
        let mut in_batch = 0usize;
        for (position, (relative, kind)) in candidates.iter().enumerate() {
            if is_cancelled() || started.elapsed() >= budget {
                report.is_complete = false;
                break;
            }
            seen.insert(relative.clone());
            let absolute = root_path.join(relative);
            let outcome = index_file(&transaction, &root, relative, kind, &absolute, next_version)?;
            match outcome {
                FileOutcome::Unchanged => report.files_unchanged += 1,
                FileOutcome::Indexed { chunks } => {
                    report.files_indexed += 1;
                    report.chunks += chunks;
                }
                FileOutcome::Skipped => report.files_skipped += 1,
            }
            in_batch += 1;
            if in_batch >= BATCH_FILES {
                transaction.commit().map_err(db_error)?;
                transaction = conn.transaction().map_err(db_error)?;
                in_batch = 0;
                on_progress(position + 1, candidates.len());
            }
        }
        if report.is_complete {
            // The whole folder was seen: whatever is in the index and not on disk is gone.
            let known: Vec<(i64, String)> = {
                let mut statement = transaction
                    .prepare("SELECT id, path FROM files WHERE root = ?1")
                    .map_err(db_error)?;
                let rows = statement
                    .query_map(params![root], |row| Ok((row.get(0)?, row.get(1)?)))
                    .map_err(db_error)?;
                rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?
            };
            for (id, path) in known {
                if !seen.contains(&path) {
                    delete_file(&transaction, id)?;
                    report.files_removed += 1;
                }
            }
        }
        let changed = report.files_indexed + report.files_removed + report.files_skipped > 0;
        if changed {
            transaction
                .execute(
                    "UPDATE roots SET version = ?2, indexed_at = ?3 WHERE root = ?1",
                    params![root, next_version, now_rfc3339()],
                )
                .map_err(db_error)?;
            report.version = next_version;
        } else if report.is_complete {
            transaction
                .execute(
                    "UPDATE roots SET indexed_at = ?2 WHERE root = ?1",
                    params![root, now_rfc3339()],
                )
                .map_err(db_error)?;
        }
        transaction.commit().map_err(db_error)?;
        report.seconds = started.elapsed().as_secs_f64();
        if let Some(hook) = &self.on_indexed {
            hook(&report);
        }
        Ok(report)
    }

    /// Search the folders in `roots` (the ones the session may read) for `query`.
    pub fn search(
        &self,
        roots: &[String],
        query: &str,
        limit: usize,
    ) -> Result<SearchOutcome, String> {
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|term| term.chars().take(MAX_TERM_CHARS).collect::<String>())
            .take(MAX_TERMS)
            .collect();
        if terms.is_empty() {
            return Err("query is required".to_owned());
        }
        let limit = limit.clamp(1, MAX_LIMIT);
        let conn = self.connection()?;
        let mut indexed_roots = Vec::new();
        let mut not_indexed = Vec::new();
        for root in roots {
            let Ok(root) = canonical(root) else { continue };
            let known: bool = conn
                .query_row(
                    "SELECT 1 FROM roots WHERE root = ?1 AND version > 0",
                    params![root],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            if known {
                indexed_roots.push(root);
            } else {
                not_indexed.push(root);
            }
        }
        if indexed_roots.is_empty() {
            return Ok(SearchOutcome {
                hits: Vec::new(),
                not_indexed,
            });
        }
        // Fetch more than `limit` so that capping hits per file still leaves enough.
        let fetch = (limit * 5).max(20);
        let placeholders = vec!["?"; indexed_roots.len()].join(",");
        let uses_index = terms.iter().all(|term| term.chars().count() >= 3);
        let (sql, mut values): (String, Vec<String>) = if uses_index {
            let expression = terms
                .iter()
                .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(" AND ");
            (
                format!(
                    "SELECT c.locator, c.text, f.root, f.path, f.sha256, f.version, f.size, f.mtime_ns
                     FROM chunks_fts JOIN chunks c ON c.id = chunks_fts.rowid
                     JOIN files f ON f.id = c.file_id
                     WHERE chunks_fts MATCH ? AND f.root IN ({placeholders})
                     ORDER BY bm25(chunks_fts) LIMIT {fetch}"
                ),
                vec![expression],
            )
        } else {
            let conditions = terms
                .iter()
                .map(|_| "c.text LIKE ? ESCAPE '\\'")
                .collect::<Vec<_>>()
                .join(" AND ");
            (
                format!(
                    "SELECT c.locator, c.text, f.root, f.path, f.sha256, f.version, f.size, f.mtime_ns
                     FROM chunks c JOIN files f ON f.id = c.file_id
                     WHERE f.root IN ({placeholders}) AND {conditions}
                     ORDER BY f.path, c.ordinal LIMIT {fetch}"
                ),
                Vec::new(),
            )
        };
        // The LIKE form lists its roots first, then the terms; the FTS form the expression first.
        let mut bound: Vec<String> = Vec::new();
        if uses_index {
            bound.append(&mut values);
            bound.extend(indexed_roots.iter().cloned());
        } else {
            bound.extend(indexed_roots.iter().cloned());
            bound.extend(terms.iter().map(|term| {
                let escaped = term
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_");
                format!("%{escaped}%")
            }));
        }
        let mut statement = conn.prepare(&sql).map_err(db_error)?;
        let rows = statement
            .query_map(params_from_iter(bound.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            })
            .map_err(|error| format!("that query could not be searched: {error}"))?;
        let mut hits = Vec::new();
        let mut per_file: std::collections::HashMap<(String, String), usize> = Default::default();
        for row in rows {
            let (locator, text, root, path, sha256, version, size, mtime_ns) =
                row.map_err(|error| format!("that query could not be searched: {error}"))?;
            let count = per_file.entry((root.clone(), path.clone())).or_default();
            if *count >= MAX_HITS_PER_FILE {
                continue;
            }
            *count += 1;
            let stale = is_stale(&Path::new(&root).join(&path), size, mtime_ns);
            hits.push(Hit {
                snippet: snippet_of(&text, &terms),
                locator: serde_json::from_str(&locator).unwrap_or(Value::Null),
                root,
                path,
                sha256,
                version,
                is_stale: stale,
            });
            if hits.len() >= limit {
                break;
            }
        }
        Ok(SearchOutcome { hits, not_indexed })
    }
}

enum FileOutcome {
    Unchanged,
    Indexed { chunks: usize },
    Skipped,
}

fn canonical(path: &str) -> Result<String, String> {
    PathBuf::from(path)
        .canonicalize()
        .map(|path| path.to_string_lossy().to_string())
        .map_err(|error| format!("{path} is not available: {error}"))
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn mtime_ns_of(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .and_then(|elapsed| i64::try_from(elapsed.as_nanos()).ok())
        .unwrap_or(0)
}

fn is_stale(path: &Path, size: i64, mtime_ns: i64) -> bool {
    match std::fs::metadata(path) {
        Ok(metadata) => metadata.len() as i64 != size || mtime_ns_of(&metadata) != mtime_ns,
        Err(_) => true,
    }
}

/// Every file under `root` the index reads, as (path relative to the root with `/`, kind). Does not
/// follow symbolic links and skips hidden folders and the folders of build output and packages.
fn walk(root: &Path, is_cancelled: &dyn Fn() -> bool) -> Vec<(String, &'static str)> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(folder) = pending.pop() {
        if is_cancelled() {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().to_string();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() || name.starts_with('.') || name.starts_with("~$") {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                if !SKIPPED_DIRS.contains(&name.as_str()) {
                    pending.push(path);
                }
            } else if file_type.is_file() {
                let Some(kind) = path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .and_then(|extension| extract::kind_of(&extension.to_ascii_lowercase()))
                else {
                    continue;
                };
                if let Ok(relative) = path.strip_prefix(root) {
                    found.push((relative.to_string_lossy().replace('\\', "/"), kind));
                }
            }
        }
    }
    found.sort();
    found
}

/// Bring one file up to date in the open transaction.
fn index_file(
    transaction: &rusqlite::Transaction<'_>,
    root: &str,
    relative: &str,
    kind: &str,
    absolute: &Path,
    version: i64,
) -> Result<FileOutcome, String> {
    let Ok(metadata) = std::fs::metadata(absolute) else {
        // Gone between the walk and now; the next run will see it is missing.
        return Ok(FileOutcome::Unchanged);
    };
    let size = metadata.len() as i64;
    let mtime_ns = mtime_ns_of(&metadata);
    let existing: Option<(i64, String, i64, i64, String)> = transaction
        .query_row(
            "SELECT id, sha256, size, mtime_ns, status FROM files WHERE root = ?1 AND path = ?2",
            params![root, relative],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .ok();
    if let Some((_, _, old_size, old_mtime, _)) = &existing {
        if *old_size == size && *old_mtime == mtime_ns {
            return Ok(FileOutcome::Unchanged);
        }
    }
    let skip = |note: String, sha256: String| -> Result<FileOutcome, String> {
        write_file_row(
            transaction,
            existing.as_ref().map(|row| row.0),
            root,
            relative,
            &sha256,
            size,
            mtime_ns,
            kind,
            version,
            "skipped",
            Some(&note),
        )?;
        Ok(FileOutcome::Skipped)
    };
    if metadata.len() > extract::MAX_FILE_BYTES {
        return skip(
            format!("larger than {} MB", extract::MAX_FILE_BYTES / (1024 * 1024)),
            String::new(),
        );
    }
    let bytes = match std::fs::read(absolute) {
        Ok(bytes) => bytes,
        Err(error) => return skip(format!("could not be read: {error}"), String::new()),
    };
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    if let Some((id, old_sha, _, _, status)) = &existing {
        if *old_sha == sha256 && status == "indexed" {
            // Touched but not changed: remember the new time so it is not read again.
            transaction
                .execute(
                    "UPDATE files SET size = ?2, mtime_ns = ?3 WHERE id = ?1",
                    params![id, size, mtime_ns],
                )
                .map_err(db_error)?;
            return Ok(FileOutcome::Unchanged);
        }
    }
    let extracted = match extract::extract(kind, &bytes) {
        Ok(extracted) => extracted,
        Err(error) => return skip(format!("could not be read: {error}"), sha256),
    };
    if extracted.chunks.is_empty() {
        return skip("has no text to search".to_owned(), sha256);
    }
    let note = extracted.is_truncated.then(|| {
        format!(
            "only the first {} passages are searchable",
            extract::MAX_CHUNKS_PER_FILE
        )
    });
    let file_id = write_file_row(
        transaction,
        existing.as_ref().map(|row| row.0),
        root,
        relative,
        &sha256,
        size,
        mtime_ns,
        kind,
        version,
        "indexed",
        note.as_deref(),
    )?;
    let mut insert = transaction
        .prepare("INSERT INTO chunks(file_id, ordinal, locator, text) VALUES (?1, ?2, ?3, ?4)")
        .map_err(db_error)?;
    for (ordinal, chunk) in extracted.chunks.iter().enumerate() {
        insert
            .execute(params![
                file_id,
                ordinal as i64,
                chunk.locator.to_string(),
                chunk.text
            ])
            .map_err(db_error)?;
    }
    Ok(FileOutcome::Indexed {
        chunks: extracted.chunks.len(),
    })
}

/// Insert or replace a file's row, dropping the chunks it had. Returns the row's id.
#[allow(clippy::too_many_arguments)]
fn write_file_row(
    transaction: &rusqlite::Transaction<'_>,
    existing_id: Option<i64>,
    root: &str,
    relative: &str,
    sha256: &str,
    size: i64,
    mtime_ns: i64,
    kind: &str,
    version: i64,
    status: &str,
    note: Option<&str>,
) -> Result<i64, String> {
    if let Some(id) = existing_id {
        transaction
            .execute("DELETE FROM chunks WHERE file_id = ?1", params![id])
            .map_err(db_error)?;
        transaction
            .execute(
                "UPDATE files SET sha256 = ?2, size = ?3, mtime_ns = ?4, kind = ?5, version = ?6,
                                  status = ?7, note = ?8 WHERE id = ?1",
                params![id, sha256, size, mtime_ns, kind, version, status, note],
            )
            .map_err(db_error)?;
        return Ok(id);
    }
    transaction
        .execute(
            "INSERT INTO files(root, path, sha256, size, mtime_ns, kind, version, status, note)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![root, relative, sha256, size, mtime_ns, kind, version, status, note],
        )
        .map_err(db_error)?;
    Ok(transaction.last_insert_rowid())
}

fn delete_file(transaction: &rusqlite::Transaction<'_>, id: i64) -> Result<(), String> {
    transaction
        .execute("DELETE FROM chunks WHERE file_id = ?1", params![id])
        .map_err(db_error)?;
    transaction
        .execute("DELETE FROM files WHERE id = ?1", params![id])
        .map_err(db_error)?;
    Ok(())
}

/// About `SNIPPET_CHARS` characters of `text` around the first place a term occurs.
fn snippet_of(text: &str, terms: &[String]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = text.to_lowercase().chars().collect();
    // Lowercasing can change the length (a few letters expand); only trust it when it did not.
    let positions = if lower.len() == chars.len() {
        &lower
    } else {
        &chars
    };
    let at = terms
        .iter()
        .filter_map(|term| {
            let needle: Vec<char> = term.to_lowercase().chars().collect();
            find_chars(positions, &needle)
        })
        .min()
        .unwrap_or(0);
    let start = at.saturating_sub(SNIPPET_CHARS / 3);
    let end = (start + SNIPPET_CHARS).min(chars.len());
    let mut snippet: String = chars[start..end].iter().collect();
    snippet = snippet.split_whitespace().collect::<Vec<_>>().join(" ");
    format!(
        "{}{snippet}{}",
        if start > 0 { "…" } else { "" },
        if end < chars.len() { "…" } else { "" }
    )
}

fn find_chars(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&at| haystack[at..at + needle.len()] == *needle)
}

/// Seconds since the Unix epoch, for the ledger event.
pub fn now_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests;
