//! Rust Application Control Plane authority for durable memory and user rules.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::capability::{
    CapabilityGrants, CapabilityRegistration, CapabilityResult, NativeCapabilityRunner,
};
use crate::skills::SideEffectClass;
use crate::{durability::atomic_write, ShadowReadError};

/// Name and version of the memory export document.
const EXPORT_FORMAT: &str = "delta.memory";
const EXPORT_VERSION: u64 = 1;
/// Limits on what one import may add, so a bad file cannot flood the store.
const MAX_IMPORT_ENTRIES: usize = 2000;
const MAX_IMPORT_CONTENT_CHARS: usize = 4000;
const MAX_IMPORT_SCOPE_CHARS: usize = 64;
/// Limits on what the model may suggest, so the waiting list stays something a person can read.
const MAX_CANDIDATE_CHARS: usize = 500;
const MAX_CANDIDATE_QUOTE_CHARS: usize = 300;
const MAX_PENDING_CANDIDATES: i64 = 30;

fn ensure_column(conn: &Connection, name: &str, definition: &str) -> Result<(), ShadowReadError> {
    let exists = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('memories') WHERE name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    )? > 0;
    if !exists {
        conn.execute(
            &format!("ALTER TABLE memories ADD COLUMN {name} {definition}"),
            [],
        )?;
    }
    Ok(())
}

pub struct MemoryStore {
    db_path: PathBuf,
    settings_path: PathBuf,
}

impl MemoryStore {
    pub fn open(state_dir: impl AsRef<Path>) -> Result<Self, ShadowReadError> {
        let state_dir = state_dir.as_ref();
        std::fs::create_dir_all(state_dir)?;
        let store = Self {
            db_path: state_dir.join("memory.db"),
            settings_path: state_dir.join("memory-settings.json"),
        };
        store.connection()?;
        Ok(store)
    }

    fn connection(&self) -> Result<Connection, ShadowReadError> {
        let conn = Connection::open(&self.db_path)?;
        // Owner-only hardening like the other authority stores (private_fs.rs).
        let _ = crate::private_fs::harden_sqlite_files(&self.db_path);
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scope TEXT NOT NULL,
                key TEXT,
                content TEXT NOT NULL,
                summary TEXT,
                workspace TEXT,
                session_id TEXT,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS memory_candidates (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                source TEXT,
                session_id TEXT,
                workspace TEXT,
                state TEXT NOT NULL DEFAULT 'pending',
                memory_id INTEGER,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                resolved_at TEXT
            );",
        )?;
        // Columns added after the first release are added in place, like `summary` was.
        ensure_column(&conn, "summary", "TEXT")?;
        ensure_column(&conn, "source", "TEXT")?;
        ensure_column(&conn, "status", "TEXT NOT NULL DEFAULT 'active'")?;
        ensure_column(&conn, "superseded_by", "INTEGER")?;
        ensure_column(&conn, "saved_by", "TEXT NOT NULL DEFAULT 'user'")?;
        Ok(conn)
    }

    /// The memories in force. A memory that was corrected (`superseded`) stays in the store as
    /// history, but is not listed.
    pub fn list(&self) -> Result<Vec<Value>, ShadowReadError> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, scope, content, summary, created_at, source, saved_by
             FROM memories WHERE status = 'active' ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            let content: String = row.get(2)?;
            let summary: Option<String> = row.get(3)?;
            let source: Option<String> = row.get(5)?;
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "scope": row.get::<_, String>(1)?,
                "content": content,
                "summary": summary.filter(|value| !value.is_empty()).unwrap_or_else(|| {
                    content.lines().next().unwrap_or_default().chars().take(120).collect()
                }),
                "created_at": row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                "source": source.and_then(|text| serde_json::from_str::<Value>(&text).ok()),
                "saved_by": row.get::<_, String>(6)?,
            }))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(ShadowReadError::from)
    }

    pub fn add(
        &self,
        content: &str,
        scope: &str,
        workspace: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<Value, ShadowReadError> {
        let conn = self.connection()?;
        conn.execute(
            "INSERT INTO memories (scope, content, workspace, session_id) VALUES (?1, ?2, ?3, ?4)",
            params![scope, content, workspace, session_id],
        )?;
        Ok(json!({"ok": true, "id": conn.last_insert_rowid()}))
    }

    /// Correct a memory. The old text is kept as history and marked as replaced; the corrected
    /// text is a new memory that carries the old one's scope, workspace and source. The result
    /// names the new memory's id, which is not the id that was passed in.
    pub fn update(&self, id: i64, content: &str) -> Result<Value, ShadowReadError> {
        let mut conn = self.connection()?;
        let transaction = conn.transaction()?;
        let current = transaction
            .query_row(
                "SELECT content FROM memories WHERE id = ?1 AND status = 'active'",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .ok();
        let Some(current) = current else {
            return Ok(json!({"ok": false, "error": "that memory is not in force"}));
        };
        if current == content {
            return Ok(json!({"ok": true, "id": id}));
        }
        transaction.execute(
            "INSERT INTO memories (scope, content, workspace, session_id, source, saved_by)
             SELECT scope, ?2, workspace, session_id, source, saved_by FROM memories WHERE id = ?1",
            params![id, content],
        )?;
        let new_id = transaction.last_insert_rowid();
        transaction.execute(
            "UPDATE memories SET status = 'superseded', superseded_by = ?2 WHERE id = ?1",
            params![id, new_id],
        )?;
        transaction.commit()?;
        Ok(json!({"ok": true, "id": new_id, "previous_id": id}))
    }

    /// Erase a memory and the earlier versions it replaced.
    pub fn delete(&self, id: i64) -> Result<Value, ShadowReadError> {
        let mut conn = self.connection()?;
        let transaction = conn.transaction()?;
        let mut chain = vec![id];
        let mut next = id;
        while let Ok(earlier) = transaction.query_row(
            "SELECT id FROM memories WHERE superseded_by = ?1",
            params![next],
            |row| row.get::<_, i64>(0),
        ) {
            if chain.contains(&earlier) {
                break;
            }
            chain.push(earlier);
            next = earlier;
        }
        let mut deleted = 0;
        for member in chain {
            deleted +=
                transaction.execute("DELETE FROM memories WHERE id = ?1", params![member])?;
        }
        transaction.commit()?;
        Ok(json!({"ok": deleted > 0}))
    }

    pub fn delete_all(&self) -> Result<Value, ShadowReadError> {
        let conn = self.connection()?;
        let deleted = conn.execute("DELETE FROM memories", [])?;
        conn.execute("DELETE FROM memory_candidates", [])?;
        Ok(json!({"ok": true, "deleted": deleted}))
    }

    /// A suggestion from the model: a fact worth remembering, waiting for the user to decide.
    /// Nothing is remembered until [`MemoryStore::accept_candidate`]. A suggestion that repeats a
    /// memory in force, one already waiting, or one the user turned down is not filed again.
    pub fn propose(
        &self,
        content: &str,
        quote: Option<&str>,
        session_id: Option<&str>,
        run_id: Option<&str>,
        workspace: Option<&str>,
    ) -> Result<Value, ShadowReadError> {
        if !self.settings()?["enabled"].as_bool().unwrap_or(false) {
            return Ok(json!({"ok": false, "error": "memory is switched off"}));
        }
        let content = content.trim();
        if content.is_empty() {
            return Ok(json!({"ok": false, "error": "content is required"}));
        }
        if content.chars().count() > MAX_CANDIDATE_CHARS {
            return Ok(json!({
                "ok": false,
                "error": format!("a memory may hold at most {MAX_CANDIDATE_CHARS} characters; say it shorter")
            }));
        }
        let conn = self.connection()?;
        let in_force = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM memories WHERE status = 'active' AND content = ?1)",
            params![content],
            |row| row.get::<_, bool>(0),
        )?;
        if in_force {
            return Ok(json!({"ok": true, "duplicate": "saved"}));
        }
        let earlier = conn
            .query_row(
                "SELECT id, state FROM memory_candidates WHERE content = ?1
                 ORDER BY (state = 'pending') DESC, id DESC LIMIT 1",
                params![content],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .ok();
        match earlier {
            Some((id, state)) if state == "pending" => {
                return Ok(json!({"ok": true, "candidate_id": id, "duplicate": "pending"}))
            }
            Some((_, state)) if state == "rejected" => {
                return Ok(json!({"ok": true, "duplicate": "declined"}))
            }
            _ => {}
        }
        let waiting = conn.query_row(
            "SELECT COUNT(*) FROM memory_candidates WHERE state = 'pending'",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        if waiting >= MAX_PENDING_CANDIDATES {
            return Ok(json!({
                "ok": false,
                "error": "too many suggestions are waiting for the user to decide; do not suggest more now"
            }));
        }
        let source = json!({
            "kind": "session",
            "session_id": session_id,
            "run_id": run_id,
            "quote": quote
                .map(|text| text.trim().chars().take(MAX_CANDIDATE_QUOTE_CHARS).collect::<String>())
                .filter(|text| !text.is_empty()),
        });
        conn.execute(
            "INSERT INTO memory_candidates (content, source, session_id, workspace)
             VALUES (?1, ?2, ?3, ?4)",
            params![content, source.to_string(), session_id, workspace],
        )?;
        Ok(json!({"ok": true, "candidate_id": conn.last_insert_rowid()}))
    }

    /// Suggestions still waiting for the user, oldest first.
    pub fn candidates(&self) -> Result<Vec<Value>, ShadowReadError> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, content, source, created_at FROM memory_candidates
             WHERE state = 'pending' ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            let source: Option<String> = row.get(2)?;
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "content": row.get::<_, String>(1)?,
                "source": source.and_then(|text| serde_json::from_str::<Value>(&text).ok()),
                "created_at": row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            }))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(ShadowReadError::from)
    }

    /// The user accepts a suggestion, as it is or with their own wording. It becomes a memory that
    /// keeps the suggestion's source.
    pub fn accept_candidate(
        &self,
        id: i64,
        edited: Option<&str>,
    ) -> Result<Value, ShadowReadError> {
        let mut conn = self.connection()?;
        let transaction = conn.transaction()?;
        let candidate = transaction
            .query_row(
                "SELECT content, source, session_id, workspace FROM memory_candidates
                 WHERE id = ?1 AND state = 'pending'",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .ok();
        let Some((content, source, session_id, workspace)) = candidate else {
            return Ok(json!({"ok": false, "error": "that suggestion is no longer waiting"}));
        };
        let content = match edited.map(str::trim).filter(|text| !text.is_empty()) {
            Some(text) if text.chars().count() > MAX_CANDIDATE_CHARS => {
                return Ok(json!({
                    "ok": false,
                    "error": format!("a memory may hold at most {MAX_CANDIDATE_CHARS} characters")
                }))
            }
            Some(text) => text.to_owned(),
            None => content,
        };
        transaction.execute(
            "INSERT INTO memories (scope, content, workspace, session_id, source, saved_by)
             VALUES ('global', ?1, ?2, ?3, ?4, 'candidate')",
            params![content, workspace, session_id, source],
        )?;
        let memory_id = transaction.last_insert_rowid();
        transaction.execute(
            "UPDATE memory_candidates SET state = 'accepted', memory_id = ?2,
             resolved_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![id, memory_id],
        )?;
        transaction.commit()?;
        Ok(json!({"ok": true, "memory_id": memory_id}))
    }

    /// The user turns a suggestion down. The model is not offered the same text again.
    pub fn reject_candidate(&self, id: i64) -> Result<Value, ShadowReadError> {
        let changed = self.connection()?.execute(
            "UPDATE memory_candidates SET state = 'rejected', resolved_at = CURRENT_TIMESTAMP
             WHERE id = ?1 AND state = 'pending'",
            params![id],
        )?;
        Ok(json!({"ok": changed > 0}))
    }

    /// Every memory as a portable document that [`MemoryStore::import`] reads back.
    pub fn export(&self) -> Result<Value, ShadowReadError> {
        let memories = self
            .list()?
            .into_iter()
            .map(|row| {
                json!({
                    "scope": row["scope"],
                    "content": row["content"],
                    "created_at": row["created_at"],
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({"format": EXPORT_FORMAT, "version": EXPORT_VERSION, "memories": memories}))
    }

    /// Add the memories of an exported document. The whole document is checked first, so one bad
    /// entry imports nothing. A memory already stored with the same scope and content is skipped.
    pub fn import(&self, document: &Value) -> Result<Value, ShadowReadError> {
        if document.get("format").and_then(Value::as_str) != Some(EXPORT_FORMAT) {
            return Err(ShadowReadError::Parse(
                "this file is not a Delta memory export".to_string(),
            ));
        }
        if document.get("version").and_then(Value::as_u64) != Some(EXPORT_VERSION) {
            return Err(ShadowReadError::Parse(
                "this memory export has an unsupported version".to_string(),
            ));
        }
        let entries = document
            .get("memories")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                ShadowReadError::Parse("the memory export has no memories list".to_string())
            })?;
        if entries.len() > MAX_IMPORT_ENTRIES {
            return Err(ShadowReadError::Parse(format!(
                "a memory export may hold at most {MAX_IMPORT_ENTRIES} memories"
            )));
        }
        let mut checked = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let text = |key: &str| entry.get(key).and_then(Value::as_str);
            let scope = text("scope").filter(|scope| {
                !scope.trim().is_empty() && scope.chars().count() <= MAX_IMPORT_SCOPE_CHARS
            });
            let content = text("content").filter(|content| {
                !content.trim().is_empty() && content.chars().count() <= MAX_IMPORT_CONTENT_CHARS
            });
            let created_at = text("created_at").filter(|stamp| {
                !stamp.is_empty() && stamp.chars().count() <= MAX_IMPORT_SCOPE_CHARS
            });
            match (scope, content) {
                (Some(scope), Some(content)) => checked.push((scope, content, created_at)),
                _ => {
                    return Err(ShadowReadError::Parse(format!(
                        "memory entry {} is invalid, so nothing was imported",
                        index + 1
                    )))
                }
            }
        }
        let mut conn = self.connection()?;
        let transaction = conn.transaction()?;
        let (mut imported, mut skipped) = (0usize, 0usize);
        for (scope, content, created_at) in checked {
            let exists = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM memories WHERE status = 'active' AND scope = ?1 AND content = ?2)",
                params![scope, content],
                |row| row.get::<_, bool>(0),
            )?;
            if exists {
                skipped += 1;
                continue;
            }
            transaction.execute(
                "INSERT INTO memories (scope, content, created_at, saved_by)
                 VALUES (?1, ?2, COALESCE(?3, CURRENT_TIMESTAMP), 'import')",
                params![scope, content, created_at],
            )?;
            imported += 1;
        }
        transaction.commit()?;
        Ok(json!({"ok": true, "imported": imported, "skipped": skipped}))
    }

    pub fn settings(&self) -> Result<Value, ShadowReadError> {
        if !self.settings_path.exists() {
            return Ok(json!({"enabled": false, "user_rules": ""}));
        }
        let text = std::fs::read_to_string(&self.settings_path).map_err(|error| {
            ShadowReadError::Parse(format!(
                "failed to read memory settings authority {}: {error}",
                self.settings_path.display()
            ))
        })?;
        let saved = serde_json::from_str::<Value>(&text).map_err(|error| {
            ShadowReadError::Parse(format!(
                "invalid memory settings authority {}: {error}",
                self.settings_path.display()
            ))
        })?;
        let object = saved.as_object().ok_or_else(|| {
            ShadowReadError::Parse(format!(
                "memory settings authority {} must be a JSON object",
                self.settings_path.display()
            ))
        })?;
        let enabled = match object.get("enabled") {
            None => false,
            Some(Value::Bool(value)) => *value,
            Some(_) => {
                return Err(ShadowReadError::Parse(format!(
                    "memory settings authority {} has a non-boolean enabled field",
                    self.settings_path.display()
                )))
            }
        };
        let user_rules = match object.get("user_rules") {
            None => "",
            Some(Value::String(value)) => value.as_str(),
            Some(_) => {
                return Err(ShadowReadError::Parse(format!(
                    "memory settings authority {} has a non-string user_rules field",
                    self.settings_path.display()
                )))
            }
        };
        Ok(json!({
            "enabled": enabled,
            "user_rules": user_rules,
        }))
    }

    pub fn set_settings(&self, patch: &Value) -> Result<Value, ShadowReadError> {
        let mut settings = self.settings()?;
        if let Some(enabled) = patch.get("enabled").and_then(Value::as_bool) {
            settings["enabled"] = Value::Bool(enabled);
        }
        if let Some(rules) = patch.get("user_rules").and_then(Value::as_str) {
            settings["user_rules"] = Value::String(rules.chars().take(20_000).collect());
        }
        let bytes = serde_json::to_vec_pretty(&settings)?;
        atomic_write(&self.settings_path, &bytes)?;
        Ok(settings)
    }
}

/// The model's tool for suggesting a memory. It only files a suggestion for the user to accept,
/// edit or decline, so it needs no approval; nothing is remembered, and nothing reaches any model
/// as a memory, until the user accepts it.
pub fn propose_registration(state_dir: PathBuf) -> CapabilityRegistration {
    let state_dir = Arc::new(state_dir);
    CapabilityRegistration {
        capability_id: "memory.propose".to_owned(),
        tool_name: "memory_propose".to_owned(),
        side_effect: SideEffectClass::LocalWrite,
        description: "Suggest remembering one lasting fact about the user or how they like to \
            work, so later conversations can use it. The user sees the suggestion and decides; \
            nothing is remembered until they accept. Suggest only what the user said or showed \
            about themselves and what will still matter in a future conversation: a preference, \
            a standing instruction, a fact about their work. Never suggest secrets, passwords, \
            the contents of a file, or something that is only true of this task. Write it as one \
            short sentence."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["content"],
            "properties": {
                "content": {"type": "string", "description": "The fact, in one short sentence."},
                "evidence": {"type": "string", "description": "The user's own words that show it, quoted, so they can see why you suggest it."}
            }
        }),
        metadata: json!({
            "risk_level": "low", "requires_approval": false,
            "category": "memory", "capabilities": ["memory.propose"]
        }),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(move |job, control| {
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            let Some(content) = job.arguments.get("content").and_then(Value::as_str) else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "content is required",
                    Some("arguments"),
                );
            };
            let evidence = job.arguments.get("evidence").and_then(Value::as_str);
            let filed = MemoryStore::open(state_dir.as_path()).and_then(|store| {
                store.propose(
                    content,
                    evidence,
                    job.session_id.as_deref(),
                    job.run_id.as_deref(),
                    job.workspace.as_deref(),
                )
            });
            match filed {
                Ok(result) if result["ok"] == true => {
                    CapabilityResult::completed(&job.job_id, result)
                }
                Ok(result) => CapabilityResult::failed(
                    &job.job_id,
                    result["error"]
                        .as_str()
                        .unwrap_or("the suggestion was not filed"),
                    Some("not_filed"),
                ),
                Err(error) => {
                    CapabilityResult::failed(&job.job_id, &error.to_string(), Some("store"))
                }
            }
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_memory_settings_file_is_valid_default_state() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(temp.path()).unwrap();
        let settings = store.settings().unwrap();
        assert_eq!(
            settings["enabled"], false,
            "memory is off until the user switches it on"
        );
        assert_eq!(settings["user_rules"], "");
    }

    #[test]
    fn corrupt_memory_settings_fail_closed_and_are_not_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(temp.path()).unwrap();
        std::fs::write(&store.settings_path, b"{broken").unwrap();
        let before = std::fs::read(&store.settings_path).unwrap();

        assert!(store.settings().is_err());
        assert!(store.set_settings(&json!({"enabled": false})).is_err());
        assert_eq!(std::fs::read(&store.settings_path).unwrap(), before);
    }

    #[test]
    fn invalid_memory_settings_shapes_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(temp.path()).unwrap();
        for invalid in ["[]", r#"{"enabled":"yes"}"#, r#"{"user_rules":42}"#] {
            std::fs::write(&store.settings_path, invalid).unwrap();
            assert!(store.settings().is_err(), "{invalid}");
        }
    }

    #[test]
    fn partial_memory_settings_keep_compatible_defaults() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(temp.path()).unwrap();
        std::fs::write(&store.settings_path, r#"{"enabled":false}"#).unwrap();
        let settings = store.settings().unwrap();
        assert_eq!(settings["enabled"], false);
        assert_eq!(settings["user_rules"], "");
    }

    #[test]
    fn exported_memories_import_back_into_an_empty_store() {
        let source = tempfile::tempdir().unwrap();
        let from = MemoryStore::open(source.path()).unwrap();
        from.add("Prefers short answers", "global", None, None)
            .unwrap();
        from.add("Reports are in Chinese", "global", None, None)
            .unwrap();
        let document = from.export().unwrap();
        assert_eq!(document["format"], "delta.memory");
        assert_eq!(document["version"], 1);

        let target = tempfile::tempdir().unwrap();
        let to = MemoryStore::open(target.path()).unwrap();
        let result = to.import(&document).unwrap();
        assert_eq!(
            (result["imported"].as_u64(), result["skipped"].as_u64()),
            (Some(2), Some(0))
        );
        let rows = to.list().unwrap();
        let contents: Vec<&str> = rows
            .iter()
            .filter_map(|row| row["content"].as_str())
            .collect();
        assert_eq!(
            contents,
            ["Prefers short answers", "Reports are in Chinese"]
        );
        assert_eq!(rows[0]["created_at"], from.list().unwrap()[0]["created_at"]);
    }

    #[test]
    fn importing_the_same_export_twice_skips_what_is_already_stored() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(temp.path()).unwrap();
        store.add("Keep me", "global", None, None).unwrap();
        let document = json!({
            "format": "delta.memory", "version": 1,
            "memories": [
                {"scope": "global", "content": "Keep me"},
                {"scope": "global", "content": "New one"},
                {"scope": "global", "content": "New one"},
            ],
        });
        let result = store.import(&document).unwrap();
        assert_eq!(
            (result["imported"].as_u64(), result["skipped"].as_u64()),
            (Some(1), Some(2))
        );
        assert_eq!(store.list().unwrap().len(), 2);
    }

    #[test]
    fn a_bad_memory_export_imports_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(temp.path()).unwrap();
        let bad_documents = [
            json!({"format": "other", "version": 1, "memories": []}),
            json!({"format": "delta.memory", "version": 2, "memories": []}),
            json!({"format": "delta.memory", "version": 1}),
            json!({"format": "delta.memory", "version": 1, "memories": [
                {"scope": "global", "content": "Fine"},
                {"scope": "global", "content": "   "},
            ]}),
            json!({"format": "delta.memory", "version": 1, "memories": [
                {"scope": "global", "content": "Fine"},
                {"content": "No scope"},
            ]}),
            json!({"format": "delta.memory", "version": 1, "memories": [
                {"scope": "global", "content": "x".repeat(4001)},
            ]}),
        ];
        for document in &bad_documents {
            assert!(store.import(document).is_err(), "{document}");
        }
        assert!(
            store.list().unwrap().is_empty(),
            "a rejected import must add nothing"
        );
    }

    #[test]
    fn an_import_over_the_entry_limit_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(temp.path()).unwrap();
        let entries: Vec<Value> = (0..=MAX_IMPORT_ENTRIES)
            .map(|index| json!({"scope": "global", "content": format!("memory {index}")}))
            .collect();
        let document = json!({"format": "delta.memory", "version": 1, "memories": entries});
        assert!(store.import(&document).is_err());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn memory_crud_and_settings_are_rust_owned() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(temp.path()).unwrap();
        let added = store.add("Remember this", "global", None, None).unwrap();
        let id = added["id"].as_i64().unwrap();
        assert_eq!(store.list().unwrap()[0]["summary"], "Remember this");
        assert_eq!(store.update(id, "Updated").unwrap()["ok"], true);
        assert_eq!(
            store
                .set_settings(&json!({"enabled": false, "user_rules": "Be concise"}))
                .unwrap()["enabled"],
            false
        );
        assert_eq!(store.delete(id).unwrap()["ok"], true);
    }

    fn store_with_memory_on() -> (tempfile::TempDir, MemoryStore) {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(temp.path()).unwrap();
        store.set_settings(&json!({"enabled": true})).unwrap();
        (temp, store)
    }

    fn propose(store: &MemoryStore, content: &str) -> Value {
        store
            .propose(
                content,
                Some("I always write in Chinese"),
                Some("s1"),
                Some("r1"),
                None,
            )
            .unwrap()
    }

    #[test]
    fn a_suggestion_is_not_a_memory_until_the_user_accepts_it() {
        let (_temp, store) = store_with_memory_on();
        let filed = propose(&store, "Writes reports in Chinese");
        let id = filed["candidate_id"].as_i64().unwrap();

        assert!(
            store.list().unwrap().is_empty(),
            "nothing is remembered yet"
        );
        let waiting = store.candidates().unwrap();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0]["source"]["session_id"], "s1");
        assert_eq!(waiting[0]["source"]["quote"], "I always write in Chinese");

        let accepted = store.accept_candidate(id, None).unwrap();
        assert_eq!(accepted["ok"], true);
        assert!(store.candidates().unwrap().is_empty());
        let memories = store.list().unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0]["content"], "Writes reports in Chinese");
        assert_eq!(memories[0]["saved_by"], "candidate");
        assert_eq!(
            memories[0]["source"]["run_id"], "r1",
            "the memory keeps its source"
        );
    }

    #[test]
    fn a_suggestion_can_be_accepted_in_the_users_own_words() {
        let (_temp, store) = store_with_memory_on();
        let id = propose(&store, "Likes short answers")["candidate_id"]
            .as_i64()
            .unwrap();
        store
            .accept_candidate(id, Some("  Wants answers under 100 words  "))
            .unwrap();
        assert_eq!(
            store.list().unwrap()[0]["content"],
            "Wants answers under 100 words"
        );
        let too_long = "x".repeat(MAX_CANDIDATE_CHARS + 1);
        let other = propose(&store, "Second")["candidate_id"].as_i64().unwrap();
        assert_eq!(
            store.accept_candidate(other, Some(&too_long)).unwrap()["ok"],
            false
        );
        assert_eq!(
            store.candidates().unwrap().len(),
            1,
            "a refused edit leaves it waiting"
        );
    }

    #[test]
    fn a_suggestion_cannot_be_filed_while_memory_is_off() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(temp.path()).unwrap();
        let result = propose(&store, "Anything");
        assert_eq!(result["ok"], false);
        assert!(store.candidates().unwrap().is_empty());
    }

    #[test]
    fn a_turned_down_suggestion_is_not_offered_again() {
        let (_temp, store) = store_with_memory_on();
        let id = propose(&store, "Lives in Shanghai")["candidate_id"]
            .as_i64()
            .unwrap();
        assert_eq!(store.reject_candidate(id).unwrap()["ok"], true);
        assert_eq!(
            store.reject_candidate(id).unwrap()["ok"],
            false,
            "decided once"
        );
        let again = propose(&store, "Lives in Shanghai");
        assert_eq!(again["duplicate"], "declined");
        assert!(store.candidates().unwrap().is_empty());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn repeated_and_known_suggestions_are_not_filed_twice() {
        let (_temp, store) = store_with_memory_on();
        let first = propose(&store, "Uses metric units")["candidate_id"]
            .as_i64()
            .unwrap();
        let second = propose(&store, "  Uses metric units ");
        assert_eq!(second["candidate_id"].as_i64(), Some(first));
        assert_eq!(second["duplicate"], "pending");
        assert_eq!(store.candidates().unwrap().len(), 1);

        store.accept_candidate(first, None).unwrap();
        assert_eq!(propose(&store, "Uses metric units")["duplicate"], "saved");
        assert!(store.candidates().unwrap().is_empty());
    }

    #[test]
    fn suggestions_are_limited_in_length_and_in_number() {
        let (_temp, store) = store_with_memory_on();
        assert_eq!(propose(&store, "   ")["ok"], false);
        assert_eq!(
            propose(&store, &"x".repeat(MAX_CANDIDATE_CHARS + 1))["ok"],
            false
        );
        for index in 0..MAX_PENDING_CANDIDATES {
            assert_eq!(propose(&store, &format!("fact {index}"))["ok"], true);
        }
        assert_eq!(propose(&store, "one too many")["ok"], false);
        assert_eq!(
            store.candidates().unwrap().len() as i64,
            MAX_PENDING_CANDIDATES
        );
    }

    #[test]
    fn a_correction_replaces_the_text_and_keeps_the_old_one_as_history() {
        let (_temp, store) = store_with_memory_on();
        let id = propose(&store, "Works at Acme")["candidate_id"]
            .as_i64()
            .unwrap();
        let memory = store.accept_candidate(id, None).unwrap()["memory_id"]
            .as_i64()
            .unwrap();

        let corrected = store.update(memory, "Works at Globex").unwrap();
        let new_id = corrected["id"].as_i64().unwrap();
        assert_ne!(new_id, memory);
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["content"], "Works at Globex");
        assert_eq!(
            listed[0]["source"]["run_id"], "r1",
            "a correction keeps the source"
        );

        let history: (String, i64) = store
            .connection()
            .unwrap()
            .query_row(
                "SELECT status, superseded_by FROM memories WHERE id = ?1",
                params![memory],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(history, ("superseded".to_string(), new_id));
        assert_eq!(
            store.update(memory, "Stale edit").unwrap()["ok"],
            false,
            "only a memory in force"
        );
        assert_eq!(
            store.update(new_id, "Works at Globex").unwrap()["id"],
            new_id,
            "no change, no new version"
        );
    }

    #[test]
    fn deleting_a_memory_erases_its_earlier_versions_too() {
        let (_temp, store) = store_with_memory_on();
        let first = store.add("v1", "global", None, None).unwrap()["id"]
            .as_i64()
            .unwrap();
        let second = store.update(first, "v2").unwrap()["id"].as_i64().unwrap();
        let third = store.update(second, "v3").unwrap()["id"].as_i64().unwrap();
        assert_eq!(store.delete(third).unwrap()["ok"], true);
        let left: i64 = store
            .connection()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }

    #[test]
    fn forgetting_everything_clears_waiting_suggestions_as_well() {
        let (_temp, store) = store_with_memory_on();
        store.add("A fact", "global", None, None).unwrap();
        propose(&store, "A suggestion");
        store.delete_all().unwrap();
        assert!(store.list().unwrap().is_empty());
        assert!(store.candidates().unwrap().is_empty());
    }

    #[test]
    fn a_store_from_before_sources_and_history_opens_and_keeps_its_memories() {
        let temp = tempfile::tempdir().unwrap();
        {
            let conn = Connection::open(temp.path().join("memory.db")).unwrap();
            conn.execute_batch(
                "CREATE TABLE memories (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, scope TEXT NOT NULL, key TEXT,
                    content TEXT NOT NULL, summary TEXT, workspace TEXT, session_id TEXT,
                    created_at TEXT DEFAULT CURRENT_TIMESTAMP);
                 INSERT INTO memories (scope, content) VALUES ('global', 'Old note');",
            )
            .unwrap();
        }
        let store = MemoryStore::open(temp.path()).unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["content"], "Old note");
        assert_eq!(listed[0]["saved_by"], "user");
        assert!(listed[0]["source"].is_null());
    }

    fn run_propose_tool(state: &Path, arguments: Value) -> crate::capability::CapabilityResult {
        use std::sync::atomic::AtomicBool;
        let registration = propose_registration(state.to_path_buf());
        let mut job = crate::capability::CapabilityJob::new("memory.propose", "j1", arguments);
        job.session_id = Some("s9".to_owned());
        job.run_id = Some("r9".to_owned());
        let control = crate::capability::CapabilityControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(|_| {}),
        );
        registration.runner.run(&job, &control)
    }

    #[test]
    fn the_propose_tool_files_a_suggestion_with_its_run_as_the_source() {
        let (temp, store) = store_with_memory_on();
        let result = run_propose_tool(
            temp.path(),
            json!({"content": "Prefers dark mode", "evidence": "I like dark mode"}),
        );
        assert_eq!(
            result.state,
            crate::capability::CapabilityExitState::Completed
        );
        let waiting = store.candidates().unwrap();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0]["content"], "Prefers dark mode");
        assert_eq!(waiting[0]["source"]["session_id"], "s9");
        assert_eq!(waiting[0]["source"]["run_id"], "r9");
        assert!(
            store.list().unwrap().is_empty(),
            "a suggestion is not a memory"
        );
    }

    #[test]
    fn the_propose_tool_fails_when_memory_is_off_and_asks_for_no_approval() {
        let temp = tempfile::tempdir().unwrap();
        let result = run_propose_tool(temp.path(), json!({"content": "Anything"}));
        assert_eq!(result.state, crate::capability::CapabilityExitState::Failed);
        assert!(MemoryStore::open(temp.path())
            .unwrap()
            .candidates()
            .unwrap()
            .is_empty());

        let registration = propose_registration(temp.path().to_path_buf());
        assert_eq!(registration.metadata["requires_approval"], false);
        let missing = run_propose_tool(temp.path(), json!({}));
        assert_eq!(
            missing.state,
            crate::capability::CapabilityExitState::Failed
        );
    }
}
