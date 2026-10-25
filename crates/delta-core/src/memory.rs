//! Rust Application Control Plane authority for durable memory and user rules.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::{durability::atomic_write, ShadowReadError};

/// Name and version of the memory export document.
const EXPORT_FORMAT: &str = "delta.memory";
const EXPORT_VERSION: u64 = 1;
/// Limits on what one import may add, so a bad file cannot flood the store.
const MAX_IMPORT_ENTRIES: usize = 2000;
const MAX_IMPORT_CONTENT_CHARS: usize = 4000;
const MAX_IMPORT_SCOPE_CHARS: usize = 64;

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
            );",
        )?;
        let has_summary = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('memories') WHERE name = 'summary'",
            [],
            |row| row.get::<_, i64>(0),
        )? > 0;
        if !has_summary {
            conn.execute("ALTER TABLE memories ADD COLUMN summary TEXT", [])?;
        }
        Ok(conn)
    }

    pub fn list(&self) -> Result<Vec<Value>, ShadowReadError> {
        let conn = self.connection()?;
        let mut stmt = conn
            .prepare("SELECT id, scope, content, summary, created_at FROM memories ORDER BY id")?;
        let rows = stmt.query_map([], |row| {
            let content: String = row.get(2)?;
            let summary: Option<String> = row.get(3)?;
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "scope": row.get::<_, String>(1)?,
                "content": content,
                "summary": summary.filter(|value| !value.is_empty()).unwrap_or_else(|| {
                    content.lines().next().unwrap_or_default().chars().take(120).collect()
                }),
                "created_at": row.get::<_, Option<String>>(4)?.unwrap_or_default(),
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

    pub fn update(&self, id: i64, content: &str) -> Result<Value, ShadowReadError> {
        let changed = self.connection()?.execute(
            "UPDATE memories SET content = ?1 WHERE id = ?2",
            params![content, id],
        )?;
        Ok(json!({"ok": changed > 0}))
    }

    pub fn delete(&self, id: i64) -> Result<Value, ShadowReadError> {
        let changed = self
            .connection()?
            .execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        Ok(json!({"ok": changed > 0}))
    }

    pub fn delete_all(&self) -> Result<Value, ShadowReadError> {
        let deleted = self.connection()?.execute("DELETE FROM memories", [])?;
        Ok(json!({"ok": true, "deleted": deleted}))
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
                "SELECT EXISTS(SELECT 1 FROM memories WHERE scope = ?1 AND content = ?2)",
                params![scope, content],
                |row| row.get::<_, bool>(0),
            )?;
            if exists {
                skipped += 1;
                continue;
            }
            transaction.execute(
                "INSERT INTO memories (scope, content, created_at)
                 VALUES (?1, ?2, COALESCE(?3, CURRENT_TIMESTAMP))",
                params![scope, content, created_at],
            )?;
            imported += 1;
        }
        transaction.commit()?;
        Ok(json!({"ok": true, "imported": imported, "skipped": skipped}))
    }

    pub fn settings(&self) -> Result<Value, ShadowReadError> {
        if !self.settings_path.exists() {
            return Ok(json!({"enabled": true, "user_rules": ""}));
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
            None => true,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_memory_settings_file_is_valid_default_state() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(temp.path()).unwrap();
        let settings = store.settings().unwrap();
        assert_eq!(settings["enabled"], true);
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
}
