//! Search over the user's own conversations (ADR-0075).
//!
//! A conversation is an append-only `<state>/conversations/<session>.jsonl`. This module keeps a
//! full-text index of the user's and the assistant's text messages in the local library database,
//! in tables of their own, and brings it up to date line by line: a message is indexed when it is
//! appended, and a search first catches up any conversation the index has not seen (for example
//! one written before this index existed). Nothing leaves the machine, and a search is limited to
//! the workspace of the session that asks.

use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::capability::{
    CapabilityGrants, CapabilityRegistration, CapabilityResult, NativeCapabilityRunner,
};
use crate::skills::SideEffectClass;

const DEFAULT_LIMIT: usize = 8;
const MAX_LIMIT: usize = 20;
const MAX_TERMS: usize = 8;
const MAX_TERM_CHARS: usize = 64;
/// Characters of context kept on each side of the first match in a snippet.
const SNIPPET_CONTEXT: usize = 60;
/// The trigram tokenizer can match a term of three or more characters only.
const TRIGRAM_MIN_CHARS: usize = 3;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS conversation_sources (
    session_id TEXT PRIMARY KEY,
    workspace TEXT,
    indexed_bytes INTEGER NOT NULL DEFAULT 0,
    indexed_lines INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS conversation_messages (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    line INTEGER NOT NULL,
    role TEXT NOT NULL,
    text TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS conversation_messages_by_session ON conversation_messages(session_id);
CREATE VIRTUAL TABLE IF NOT EXISTS conversation_fts USING fts5(
    text, content = 'conversation_messages', content_rowid = 'id', tokenize = 'trigram'
);
CREATE TRIGGER IF NOT EXISTS conversation_messages_after_insert AFTER INSERT ON conversation_messages BEGIN
    INSERT INTO conversation_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER IF NOT EXISTS conversation_messages_after_delete AFTER DELETE ON conversation_messages BEGIN
    INSERT INTO conversation_fts(conversation_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;";

fn conversations_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("conversations")
}

fn connect(state_dir: &Path) -> Result<Connection, String> {
    fs::create_dir_all(state_dir).map_err(|error| error.to_string())?;
    let conn = Connection::open(state_dir.join("library.db")).map_err(|error| error.to_string())?;
    conn.busy_timeout(Duration::from_secs(30))
        .map_err(|error| error.to_string())?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
        .map_err(|error| error.to_string())?;
    conn.execute_batch(SCHEMA)
        .map_err(|error| error.to_string())?;
    Ok(conn)
}

/// The workspace a session belongs to, as the session table records it.
fn session_workspace(state_dir: &Path, session_id: &str) -> Option<String> {
    let db = state_dir.join("core.db");
    if !db.exists() {
        return None;
    }
    let conn = Connection::open(db).ok()?;
    let row: Option<Option<String>> = conn
        .query_row(
            "SELECT workspace FROM sessions WHERE session_id = ?1",
            params![session_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .ok()?;
    row.flatten()
        .filter(|workspace| !workspace.trim().is_empty())
}

/// Every session the session table knows, with its title.
fn known_sessions(state_dir: &Path) -> Vec<(String, Option<String>)> {
    let db = state_dir.join("core.db");
    if !db.exists() {
        return Vec::new();
    }
    let Ok(conn) = Connection::open(db) else {
        return Vec::new();
    };
    let Ok(mut statement) = conn.prepare("SELECT session_id, title FROM sessions") else {
        return Vec::new();
    };
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
    });
    match rows {
        Ok(rows) => rows.filter_map(Result::ok).collect(),
        Err(_) => Vec::new(),
    }
}

/// The text a message says, for the roles that are searched: the user's and the assistant's.
/// Content is a string or a list of blocks; only the text blocks are kept.
pub(crate) fn message_text(message: &Value) -> Option<(String, String)> {
    let role = message.get("role").and_then(Value::as_str)?;
    if role != "user" && role != "assistant" {
        return None;
    }
    let text = match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| match block {
                Value::String(text) => Some(text.clone()),
                Value::Object(_) if block.get("type").and_then(Value::as_str) == Some("text") => {
                    block.get("text").and_then(Value::as_str).map(str::to_owned)
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => message
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    };
    (!text.trim().is_empty()).then(|| (role.to_owned(), text))
}

/// Forget everything indexed for one session.
fn clear_session(conn: &Connection, session_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM conversation_messages WHERE session_id = ?1",
        params![session_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM conversation_sources WHERE session_id = ?1",
        params![session_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Bring one session's index up to date with its file. Only complete lines are read, so a line
/// still being written is picked up on the next call. A file that became shorter (it was rewritten)
/// is indexed again from the start.
pub fn sync_session(state_dir: &Path, session_id: &str) -> Result<(), String> {
    if !crate::control_plane::safe_session_id(session_id) {
        return Err("unsafe session id".to_owned());
    }
    let conn = connect(state_dir)?;
    let path = conversations_dir(state_dir).join(format!("{session_id}.jsonl"));
    let Ok(metadata) = fs::metadata(&path) else {
        return clear_session(&conn, session_id);
    };
    let size = metadata.len();
    let workspace = session_workspace(state_dir, session_id);
    let (mut bytes, mut lines): (u64, i64) = conn
        .query_row(
            "SELECT indexed_bytes, indexed_lines FROM conversation_sources WHERE session_id = ?1",
            params![session_id],
            |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or((0, 0));
    if bytes > size {
        clear_session(&conn, session_id)?;
        bytes = 0;
        lines = 0;
    }
    if size > bytes {
        let mut file = fs::File::open(&path).map_err(|error| error.to_string())?;
        file.seek(SeekFrom::Start(bytes))
            .map_err(|error| error.to_string())?;
        let mut tail = Vec::with_capacity((size - bytes) as usize);
        file.read_to_end(&mut tail)
            .map_err(|error| error.to_string())?;
        let transaction = conn
            .unchecked_transaction()
            .map_err(|error| error.to_string())?;
        let mut consumed = 0usize;
        for raw in tail.split_inclusive(|byte| *byte == b'\n') {
            if !raw.ends_with(b"\n") {
                break;
            }
            consumed += raw.len();
            lines += 1;
            let Ok(message) = serde_json::from_slice::<Value>(raw) else {
                continue;
            };
            if let Some((role, text)) = message_text(&message) {
                transaction
                    .execute(
                        "INSERT INTO conversation_messages (session_id, line, role, text) VALUES (?1, ?2, ?3, ?4)",
                        params![session_id, lines, role, text],
                    )
                    .map_err(|error| error.to_string())?;
            }
        }
        transaction
            .execute(
                "INSERT INTO conversation_sources (session_id, workspace, indexed_bytes, indexed_lines)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(session_id) DO UPDATE SET
                    workspace = excluded.workspace,
                    indexed_bytes = excluded.indexed_bytes,
                    indexed_lines = excluded.indexed_lines",
                params![session_id, workspace, (bytes + consumed as u64) as i64, lines],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Index every conversation the session table knows, and drop the index of any session that no
/// longer exists. The catch-up that a search runs first.
pub fn sync_all(state_dir: &Path) -> Result<(), String> {
    let sessions = known_sessions(state_dir);
    let known: BTreeSet<&str> = sessions.iter().map(|(id, _)| id.as_str()).collect();
    for (session_id, _) in &sessions {
        if crate::control_plane::safe_session_id(session_id) {
            sync_session(state_dir, session_id)?;
        }
    }
    let conn = connect(state_dir)?;
    let mut statement = conn
        .prepare("SELECT session_id FROM conversation_sources")
        .map_err(|error| error.to_string())?;
    let indexed: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .collect();
    drop(statement);
    for session_id in indexed {
        if !known.contains(session_id.as_str()) {
            clear_session(&conn, &session_id)?;
        }
    }
    Ok(())
}

/// Drop the index of a deleted session. Its conversation file has already gone.
pub fn remove_session(state_dir: &Path, session_id: &str) -> Result<(), String> {
    let conn = connect(state_dir)?;
    clear_session(&conn, session_id)
}

/// The text around the first match, with ellipses where it was cut.
fn snippet(text: &str, needle: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let target: Vec<char> = needle.chars().collect();
    let at = (0..chars.len().saturating_sub(target.len().saturating_sub(1)))
        .find(|&start| {
            target.iter().enumerate().all(|(offset, wanted)| {
                chars
                    .get(start + offset)
                    .is_some_and(|found| found.to_lowercase().eq(wanted.to_lowercase()))
            })
        })
        .unwrap_or(0);
    let start = at.saturating_sub(SNIPPET_CONTEXT);
    let end = (at + target.len() + SNIPPET_CONTEXT).min(chars.len());
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(chars[start..end].iter());
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// Search the conversations of one workspace for the words in `query`. Every word must appear in
/// the same message. Newer messages come first.
pub fn search(
    state_dir: &Path,
    workspace: &str,
    query: &str,
    limit: usize,
) -> Result<Value, String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| term.chars().take(MAX_TERM_CHARS).collect::<String>())
        .take(MAX_TERMS)
        .collect();
    if terms.is_empty() {
        return Err("query is required".to_owned());
    }
    sync_all(state_dir)?;
    let conn = connect(state_dir)?;
    let mut sql = String::from(
        "SELECT m.session_id, m.line, m.role, m.text
         FROM conversation_messages m
         JOIN conversation_sources s ON s.session_id = m.session_id
         WHERE s.workspace = ?1",
    );
    let mut values: Vec<String> = vec![workspace.to_owned()];
    for term in &terms {
        values.push(term.clone());
        let index = values.len();
        if term.chars().count() >= TRIGRAM_MIN_CHARS {
            sql.push_str(&format!(
                " AND m.id IN (SELECT rowid FROM conversation_fts WHERE conversation_fts MATCH ?{index})"
            ));
            values[index - 1] = format!("\"{}\"", term.replace('"', "\"\""));
        } else {
            sql.push_str(&format!(" AND instr(lower(m.text), lower(?{index})) > 0"));
        }
    }
    let limit = limit.clamp(1, MAX_LIMIT);
    sql.push_str(&format!(" ORDER BY m.id DESC LIMIT {limit}"));
    let mut statement = conn.prepare(&sql).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(values.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let sessions = known_sessions(state_dir);
    let mut hits = Vec::new();
    for row in rows {
        let (session_id, line, role, text) = row.map_err(|error| error.to_string())?;
        let title = sessions
            .iter()
            .find(|(id, _)| *id == session_id)
            .and_then(|(_, title)| title.clone());
        hits.push(json!({
            "session_id": session_id,
            "title": title,
            "line": line,
            "role": role,
            "snippet": snippet(&text, &terms[0]),
        }));
    }
    Ok(json!({"ok": true, "query": query, "workspace": workspace, "hits": hits}))
}

/// The model's tool for conversation search. It asks no approval: the conversations are the
/// user's own and stay on this machine.
pub fn tool_registration(state_dir: PathBuf) -> CapabilityRegistration {
    let state_dir = Arc::new(state_dir);
    CapabilityRegistration {
        capability_id: "conversation.search".to_owned(),
        tool_name: "conversation_search".to_owned(),
        side_effect: SideEffectClass::Read,
        description: "Search the messages of earlier conversations in this workspace: the user's \
            and the assistant's text. Every word must appear in the same message; words of three or \
            more characters match in any language. Each hit names the conversation, its title, the \
            line of the transcript and a short snippet, newest first. Nothing leaves the machine."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["query"],
            "properties": {
                "query": {"type": "string", "description": "Words that must all appear in the same message."},
                "limit": {"type": "integer", "description": "How many hits, 1 to 20. Default 8."}
            }
        }),
        metadata: json!({
            "risk_level": "low", "requires_approval": false,
            "category": "read", "capabilities": ["conversation.search"]
        }),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(move |job, control| {
            let Some(query) = job.arguments.get("query").and_then(Value::as_str) else {
                return CapabilityResult::failed(&job.job_id, "query is required", Some("arguments"));
            };
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            let limit = job
                .arguments
                .get("limit")
                .and_then(Value::as_u64)
                .map_or(DEFAULT_LIMIT, |limit| limit as usize);
            let Some(workspace) = job.workspace.as_deref().filter(|w| !w.trim().is_empty()) else {
                return CapabilityResult::completed(
                    &job.job_id,
                    json!({
                        "ok": true, "query": query, "hits": [],
                        "note": "this session has no workspace, so there is no workspace to search",
                    }),
                );
            };
            match search(&state_dir, workspace, query, limit) {
                Ok(result) => CapabilityResult::completed(&job.job_id, result),
                Err(error) => CapabilityResult::failed(&job.job_id, &error, Some("search_failed")),
            }
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with_sessions(sessions: &[(&str, &str, &str)]) -> tempfile::TempDir {
        let state = tempfile::tempdir().unwrap();
        let db = Connection::open(state.path().join("core.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE sessions (session_id TEXT PRIMARY KEY, workspace TEXT, title TEXT);",
        )
        .unwrap();
        for (id, workspace, title) in sessions {
            db.execute(
                "INSERT INTO sessions (session_id, workspace, title) VALUES (?1, ?2, ?3)",
                params![id, workspace, title],
            )
            .unwrap();
        }
        fs::create_dir_all(state.path().join("conversations")).unwrap();
        state
    }

    fn write_lines(state: &Path, session: &str, lines: &[&str]) {
        let path = conversations_dir(state).join(format!("{session}.jsonl"));
        let mut text = String::new();
        for line in lines {
            text.push_str(line);
            text.push('\n');
        }
        fs::write(path, text).unwrap();
    }

    fn hits(result: &Value) -> Vec<String> {
        result["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|hit| hit["session_id"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn only_user_and_assistant_text_is_taken() {
        assert_eq!(
            message_text(&json!({"role": "user", "content": "你好 world"})),
            Some(("user".to_owned(), "你好 world".to_owned()))
        );
        let blocks = json!({"role": "assistant", "content": [
            {"type": "text", "text": "first"},
            {"type": "tool_use", "name": "read_file", "input": {"path": "secret.txt"}}
        ]});
        assert_eq!(message_text(&blocks).unwrap().1, "first");
        assert!(message_text(&json!({"role": "tool", "content": "raw output"})).is_none());
        assert!(message_text(&json!({"role": "user", "content": "   "})).is_none());
    }

    #[test]
    fn a_search_finds_words_in_the_workspace_of_the_session_only() {
        let state = state_with_sessions(&[("s1", "/work/a", "Budget"), ("s2", "/work/b", "Other")]);
        write_lines(
            state.path(),
            "s1",
            &[
                r#"{"role":"user","content":"\u6211\u4eec\u8ba8\u8bba\u4e00\u4e0b quarterly budget"}"#,
                r#"{"role":"assistant","content":"Budget approved"}"#,
            ],
        );
        write_lines(
            state.path(),
            "s2",
            &[r#"{"role":"user","content":"quarterly budget too"}"#],
        );
        let found = search(state.path(), "/work/a", "quarterly budget", 8).unwrap();
        assert_eq!(hits(&found), ["s1"]);
        assert_eq!(found["hits"][0]["title"], "Budget");
        assert_eq!(found["hits"][0]["line"], 1);
        let short = search(state.path(), "/work/a", "budget", 8).unwrap();
        assert_eq!(hits(&short), ["s1", "s1"], "both messages mention it");
    }

    /// Budget (ADR-0077): however many hits match and whatever limit is asked for, one search
    /// returns at most `MAX_LIMIT` hits, so the output size stays bounded.
    #[test]
    fn the_hit_count_is_capped_whatever_limit_is_asked() {
        let state = state_with_sessions(&[("s1", "/w", "t")]);
        let lines: Vec<String> = (0..30)
            .map(|index| format!(r#"{{"role":"user","content":"budget line {index}"}}"#))
            .collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        write_lines(state.path(), "s1", &refs);
        let found = search(state.path(), "/w", "budget", 10_000).unwrap();
        assert_eq!(hits(&found).len(), MAX_LIMIT);
    }

    #[test]
    fn short_words_match_by_substring_and_snippets_keep_the_match() {
        let state = state_with_sessions(&[("s1", "/w", "t")]);
        write_lines(
            state.path(),
            "s1",
            &[r#"{"role":"user","content":"a note about \u6eb6\u51fa testing"}"#],
        );
        let found = search(state.path(), "/w", "溶出", 8).unwrap();
        assert_eq!(hits(&found), ["s1"]);
        assert!(found["hits"][0]["snippet"]
            .as_str()
            .unwrap()
            .contains("溶出"));
        let short = search(state.path(), "/w", "no", 8).unwrap();
        assert_eq!(hits(&short), ["s1"]);
    }

    #[test]
    fn new_lines_are_picked_up_and_a_partial_line_waits_for_its_newline() {
        let state = state_with_sessions(&[("s1", "/w", "t")]);
        write_lines(
            state.path(),
            "s1",
            &[r#"{"role":"user","content":"first alpha"}"#],
        );
        sync_session(state.path(), "s1").unwrap();
        let path = conversations_dir(state.path()).join("s1.jsonl");
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        use std::io::Write;
        file.write_all(br#"{"role":"user","content":"second beta"}"#)
            .unwrap();
        drop(file);
        assert!(hits(&search(state.path(), "/w", "beta", 8).unwrap()).is_empty());
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"\n").unwrap();
        drop(file);
        assert_eq!(
            hits(&search(state.path(), "/w", "beta", 8).unwrap()),
            ["s1"]
        );
        assert_eq!(
            hits(&search(state.path(), "/w", "alpha", 8).unwrap()),
            ["s1"]
        );
    }

    #[test]
    fn a_deleted_session_leaves_no_hits_behind() {
        let state = state_with_sessions(&[("s1", "/w", "t")]);
        write_lines(
            state.path(),
            "s1",
            &[r#"{"role":"user","content":"keep this phrase"}"#],
        );
        assert_eq!(
            hits(&search(state.path(), "/w", "phrase", 8).unwrap()),
            ["s1"]
        );
        fs::remove_file(conversations_dir(state.path()).join("s1.jsonl")).unwrap();
        remove_session(state.path(), "s1").unwrap();
        assert!(hits(&search(state.path(), "/w", "phrase", 8).unwrap()).is_empty());
    }

    #[test]
    fn an_unsafe_session_id_is_refused() {
        let state = state_with_sessions(&[]);
        assert!(sync_session(state.path(), "../escape").is_err());
    }
}
