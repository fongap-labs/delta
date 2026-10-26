//! Application Control Plane — Rust authority for Session/Workspace state.
//!
//! Ownership: the Rust core is the sole authority for session metadata and
//! workspace trust state. Writes go through this module.

use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use base64::Engine;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::{durability::atomic_write, ShadowReadError};

const SESSIONS_TABLE: &str = "CREATE TABLE IF NOT EXISTS sessions (
    session_id TEXT PRIMARY KEY, workspace TEXT, model TEXT, mode TEXT,
    title TEXT, agent TEXT DEFAULT 'delta', n_msgs INTEGER DEFAULT 0, messages TEXT,
    extra_roots TEXT, pinned INTEGER DEFAULT 0, archived INTEGER DEFAULT 0,
    origin TEXT, origin_label TEXT,
    auto_title TEXT, renamed INTEGER DEFAULT 0,
    grants TEXT, compaction TEXT, reasoning_effort TEXT DEFAULT 'auto',
    recovery TEXT,
    unattended INTEGER DEFAULT 0,
    updated_at TEXT DEFAULT CURRENT_TIMESTAMP
);";

const WORKSPACES_TABLE: &str = "CREATE TABLE IF NOT EXISTS workspaces (
    path TEXT PRIMARY KEY, last_used TEXT DEFAULT CURRENT_TIMESTAMP
);";

/// Start a read-write SQLite connection to `core.db`, ensuring the schema.
fn open_conn(db_path: &Path) -> Result<Connection, ShadowReadError> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(db_path)?;
    // Owner-only hardening like the other authority stores (private_fs.rs).
    let _ = crate::private_fs::harden_sqlite_files(db_path);
    conn.prepare(SESSIONS_TABLE)?.execute(params![])?;
    conn.prepare(WORKSPACES_TABLE)?.execute(params![])?;
    for (name, ddl) in [
        ("grants", "ALTER TABLE sessions ADD COLUMN grants TEXT"),
        (
            "compaction",
            "ALTER TABLE sessions ADD COLUMN compaction TEXT",
        ),
        (
            "reasoning_effort",
            "ALTER TABLE sessions ADD COLUMN reasoning_effort TEXT DEFAULT 'auto'",
        ),
        (
            "unattended",
            "ALTER TABLE sessions ADD COLUMN unattended INTEGER DEFAULT 0",
        ),
    ] {
        let present = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = ?1",
            params![name],
            |row| row.get::<_, i64>(0),
        )? > 0;
        if !present {
            conn.execute(ddl, params![])?;
        }
    }
    Ok(conn)
}

/// The conversations dir where `<session_id>.jsonl` lives.
fn conversations_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("conversations")
}

/// Load messages from `<state_dir>/conversations/<id>.jsonl` (append-only log).
fn load_messages(state_dir: &Path, session_id: &str) -> Vec<Value> {
    if !safe_session_id(session_id) {
        return Vec::new();
    }
    let path = conversations_dir(state_dir).join(format!("{session_id}.jsonl"));
    let Ok(file) = fs::File::open(&path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
        if let Ok(v) = serde_json::from_str::<Value>(&line) {
            out.push(v);
        }
    }
    out
}

/// Longest auto-derived session title, in characters.
const AUTO_TITLE_MAX_CHARS: usize = 30;

/// Human-readable task name from the user's first message: the first non-empty line with
/// whitespace collapsed, clipped to [`AUTO_TITLE_MAX_CHARS`] characters. `None` when the
/// text carries nothing nameable (the UI then shows a generic placeholder, never the id).
fn derive_title(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = collapsed.chars();
    let head: String = chars.by_ref().take(AUTO_TITLE_MAX_CHARS).collect();
    if head.is_empty() {
        return None;
    }
    Some(if chars.next().is_some() {
        format!("{}…", head.trim_end())
    } else {
        head
    })
}

/// Name a session from its first user message unless it already has a title or the user
/// renamed it. Best-effort: a missing row or a no-op update is not an error.
fn set_auto_title(state_dir: &Path, session_id: &str, text: &str) -> Result<(), ShadowReadError> {
    let Some(title) = derive_title(text) else {
        return Ok(());
    };
    let conn = open_conn(&state_dir.join("core.db"))?;
    conn.execute(
        "UPDATE sessions SET title = ?1, auto_title = ?1
         WHERE session_id = ?2 AND renamed = 0 AND (title IS NULL OR title = '')",
        params![title, session_id],
    )?;
    Ok(())
}

/// Serialize one session row into the frontend `SessionDto` shape.
fn row_to_dto(row: &rusqlite::Row) -> rusqlite::Result<Value> {
    let updated_at: Option<String> = row.get("updated_at")?;
    let origin: Option<String> = row.get("origin")?;
    Ok(json!({
        "session_id": row.get::<_, String>("session_id")?,
        "title": row.get::<_, Option<String>>("title")?,
        "workspace": row.get::<_, Option<String>>("workspace")?.unwrap_or_default(),
        "agent": row.get::<_, Option<String>>("agent")?.unwrap_or_else(|| "delta".to_string()),
        "model": row.get::<_, Option<String>>("model")?.unwrap_or_default(),
        "mode": row.get::<_, Option<String>>("mode")?.unwrap_or_else(|| "interactive".to_string()),
        "updated_at": updated_at,
        "messages": row.get::<_, i64>("n_msgs")?,
        "pinned": row.get::<_, i64>("pinned")? != 0,
        "archived": row.get::<_, i64>("archived")? != 0,
        "origin": origin,
        "reasoning_effort": row.get::<_, Option<String>>("reasoning_effort")?.unwrap_or_else(|| "auto".to_string()),
    }))
}

fn safe_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= 128
        && session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// Ensure the Rust-owned session row exists before an active Runtime is
/// registered. Existing transcript/title fields are preserved.
pub fn ensure_session(
    state_dir: &Path,
    session_id: &str,
    workspace: Option<&str>,
    model: &str,
) -> Result<Value, ShadowReadError> {
    if !safe_session_id(session_id) {
        return Err(ShadowReadError::Parse("unsafe session id".to_string()));
    }
    let workspace = workspace.unwrap_or_default();
    let conn = open_conn(&state_dir.join("core.db"))?;
    conn.execute(
        "INSERT INTO sessions
         (session_id, workspace, model, mode, title, agent, n_msgs, reasoning_effort, updated_at)
         VALUES (?1, ?2, ?3, 'interactive', NULL, 'delta', 0, 'auto', CURRENT_TIMESTAMP)
         ON CONFLICT(session_id) DO UPDATE SET
           workspace = CASE WHEN excluded.workspace = '' THEN sessions.workspace ELSE excluded.workspace END,
           model = excluded.model, agent = 'delta',
           updated_at = CURRENT_TIMESTAMP",
        params![session_id, workspace, model],
    )?;
    if !workspace.is_empty() {
        conn.execute(
            "INSERT INTO workspaces (path, last_used) VALUES (?1, CURRENT_TIMESTAMP)
             ON CONFLICT(path) DO UPDATE SET last_used = CURRENT_TIMESTAMP",
            params![workspace],
        )?;
    }
    fs::create_dir_all(conversations_dir(state_dir))?;
    Ok(json!({"ok": true, "session_id": session_id}))
}

/// Append one canonical transcript message and keep the SQLite index count in
/// lockstep. Runtime events call this; React never writes transcript state.
pub fn append_session_message(
    state_dir: &Path,
    session_id: &str,
    message: &Value,
) -> Result<(), ShadowReadError> {
    if !safe_session_id(session_id) {
        return Err(ShadowReadError::Parse("unsafe session id".to_string()));
    }
    fs::create_dir_all(conversations_dir(state_dir))?;
    let path = conversations_dir(state_dir).join(format!("{session_id}.jsonl"));
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    serde_json::to_writer(&mut file, message)?;
    file.write_all(b"\n")?;
    file.flush()?;
    let conn = open_conn(&state_dir.join("core.db"))?;
    conn.execute(
        "UPDATE sessions SET n_msgs = n_msgs + 1, updated_at = CURRENT_TIMESTAMP WHERE session_id = ?1",
        params![session_id],
    )?;
    Ok(())
}

/// Persist transcript and recovery state from the single RuntimeEvent protocol.
pub fn record_runtime_event(state_dir: &Path, frame: &Value) -> Result<(), ShadowReadError> {
    let session_id = frame
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| ShadowReadError::Parse("runtime event missing session id".to_string()))?;
    let event_type = frame
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let payload = frame.get("payload").cloned().unwrap_or_else(|| json!({}));
    match event_type {
        "turn_start" => {
            let input = payload.get("input").cloned().unwrap_or(Value::Null);
            let content = input.as_str().unwrap_or_default();
            if !content.is_empty() && content != "(resumed)" {
                let mut message = json!({
                    "role": "user",
                    "content": input,
                    "run_id": payload.get("run_id").cloned().unwrap_or(Value::Null),
                });
                if let Some(source) = payload.get("source").filter(|value| !value.is_null()) {
                    message["source"] = source.clone();
                }
                if let Some(attachments) = payload
                    .get("attachments")
                    .filter(|value| value.as_array().is_some_and(|items| !items.is_empty()))
                {
                    message["attachments"] = attachments.clone();
                }
                append_session_message(state_dir, session_id, &message)?;
                set_auto_title(state_dir, session_id, content)?;
            }
        }
        "assistant_message" => {
            let message = payload.get("message").cloned().unwrap_or_else(|| {
                json!({
                    "role": "assistant",
                    "content": payload.get("text").cloned().unwrap_or(Value::Null),
                    "reasoning": payload.get("reasoning").cloned().unwrap_or(Value::Null),
                    "tool_calls": payload.get("tool_calls").cloned().unwrap_or_else(|| json!([])),
                    "usage": payload.get("usage").cloned().unwrap_or(Value::Null),
                })
            });
            append_session_message(state_dir, session_id, &message)?;
        }
        "tool_finished" => {
            append_session_message(
                state_dir,
                session_id,
                &json!({
                    "role": "tool",
                    "tool_call_id": payload.get("tool_call_id").cloned().unwrap_or(Value::Null),
                    "name": payload.get("name").cloned().unwrap_or(Value::Null),
                    "content": payload.get("result").cloned().unwrap_or(Value::Null),
                    "error": payload.get("error").cloned().unwrap_or(Value::Null),
                }),
            )?;
        }
        "turn_end" | "interrupted" | "error" => {
            // A failure that ends the turn belongs in the transcript, not only in the live view:
            // otherwise a conversation reopened later shows the question with no answer and no
            // reason. Retry progress (non-terminal) is not written.
            if event_type == "error"
                && payload.get("terminal").and_then(Value::as_bool) == Some(true)
            {
                let text = payload
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                append_session_message(
                    state_dir,
                    session_id,
                    &json!({"role": "notice", "kind": "error", "text": text, "ts": crate::runtime::now_ts()}),
                )?;
            }
            let conn = open_conn(&state_dir.join("core.db"))?;
            conn.execute(
                "UPDATE sessions SET recovery = ?1, updated_at = CURRENT_TIMESTAMP WHERE session_id = ?2",
                params![serde_json::to_string(&json!({"event": event_type, "payload": payload}))?, session_id],
            )?;
        }
        _ => {}
    }
    Ok(())
}

/// Sessions created before auto-titling have no title and would show their raw id. Name
/// them once from their first user message; later listings find nothing left to do.
fn backfill_titles(state_dir: &Path, conn: &Connection) -> Result<(), ShadowReadError> {
    let mut stmt = conn.prepare(
        "SELECT session_id FROM sessions
         WHERE renamed = 0 AND (title IS NULL OR title = '') AND n_msgs > 0",
    )?;
    let ids = stmt
        .query_map(params![], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in ids {
        let first_user = load_messages(state_dir, &id)
            .into_iter()
            .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
            .and_then(|message| {
                message
                    .get("content")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            });
        if let Some(text) = first_user {
            set_auto_title(state_dir, &id, &text)?;
        }
    }
    Ok(())
}

/// Consensus: list sessions, optionally filtered by workspace.
///
/// Returns `{"sessions": [...]}` in the SAME shape the FastAPI
/// `GET /v1/sessions` returned, so the frontend `getSessions` parses it
/// unchanged when switched to direct IPC.
pub fn list_sessions(state_dir: &Path, workspace: Option<&str>) -> Result<Value, ShadowReadError> {
    let db_path = state_dir.join("core.db");
    if !db_path.exists() {
        return Ok(json!({"sessions": []}));
    }
    let conn = open_conn(&db_path)?;
    backfill_titles(state_dir, &conn)?;
    let mut sessions = Vec::new();
    match workspace {
        Some(ws) => {
            let mut stmt = conn
                .prepare("SELECT * FROM sessions WHERE workspace = ?1 ORDER BY updated_at DESC")?;
            for r in stmt.query_map(params![ws], row_to_dto)? {
                sessions.push(r?);
            }
        }
        None => {
            let mut stmt = conn.prepare("SELECT * FROM sessions ORDER BY updated_at DESC")?;
            for r in stmt.query_map(params![], row_to_dto)? {
                sessions.push(r?);
            }
        }
    }
    Ok(json!({"sessions": sessions}))
}

/// Load a session's transcript: metadata + messages from its `.jsonl`.
pub fn get_session_messages(state_dir: &Path, session_id: &str) -> Result<Value, ShadowReadError> {
    Ok(json!({"messages": load_messages(state_dir, session_id)}))
}

/// Rename a session's title.
pub fn rename_session(
    state_dir: &Path,
    session_id: &str,
    title: &str,
) -> Result<Value, ShadowReadError> {
    let db_path = state_dir.join("core.db");
    if !db_path.exists() {
        return Ok(json!({"ok": false, "error": "state db missing"}));
    }
    let conn = open_conn(&db_path)?;
    let n = conn.execute(
        "UPDATE sessions SET title = ?1, renamed = 1 WHERE session_id = ?2",
        params![title, session_id],
    )?;
    Ok(json!({"ok": n > 0}))
}

/// Toggle pinned/archived flags on a session.
pub fn set_session_flags(
    state_dir: &Path,
    session_id: &str,
    is_pinned: Option<bool>,
    is_archived: Option<bool>,
) -> Result<Value, ShadowReadError> {
    let archived = is_archived;
    let pinned = is_pinned;
    let db_path = state_dir.join("core.db");
    if !db_path.exists() {
        return Ok(json!({"ok": false, "error": "state db missing"}));
    }
    let conn = open_conn(&db_path)?;
    if let Some(p) = pinned {
        conn.execute(
            "UPDATE sessions SET pinned = ?1 WHERE session_id = ?2",
            params![if p { 1 } else { 0 }, session_id],
        )?;
    }
    if let Some(a) = archived {
        conn.execute(
            "UPDATE sessions SET archived = ?1 WHERE session_id = ?2",
            params![if a { 1 } else { 0 }, session_id],
        )?;
    }
    Ok(json!({"ok": true}))
}

/// Delete a session: remove its row and its transcript file.
pub fn delete_session(state_dir: &Path, session_id: &str) -> Result<Value, ShadowReadError> {
    if !safe_session_id(session_id) {
        return Err(ShadowReadError::Parse("unsafe session id".to_string()));
    }
    let db_path = state_dir.join("core.db");
    let mut is_is_deleted_row = false;
    if db_path.exists() {
        let conn = open_conn(&db_path)?;
        let n = conn.execute(
            "DELETE FROM sessions WHERE session_id = ?1",
            params![session_id],
        )?;
        is_is_deleted_row = n > 0;
    }
    let jl = conversations_dir(state_dir).join(format!("{session_id}.jsonl"));
    let _ = fs::remove_file(&jl);
    Ok(json!({"ok": is_is_deleted_row, "session_id": session_id}))
}

/// List recently-used workspaces (distinct workspace column from sessions).
pub fn list_recent_workspaces(state_dir: &Path) -> Result<Value, ShadowReadError> {
    let db_path = state_dir.join("core.db");
    if !db_path.exists() {
        return Ok(json!({"workspaces": []}));
    }
    let conn = open_conn(&db_path)?;
    let mut stmt = conn.prepare(
        "SELECT workspace, MAX(updated_at) AS last FROM sessions \
         WHERE workspace != '' GROUP BY workspace ORDER BY last DESC",
    )?;
    let rows = stmt.query_map(params![], |r| {
        Ok(json!({
            "path": r.get::<_, String>("workspace")?,
            "name": Path::new(&r.get::<_, String>("workspace")?).file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            "exists": Path::new(&r.get::<_, String>("workspace")?).is_dir(),
        }))
    })?;
    let mut workspaces = Vec::new();
    for r in rows {
        workspaces.push(r?);
    }
    Ok(json!({"workspaces": workspaces}))
}

fn session_workspace(state_dir: &Path, session_id: &str) -> Result<String, ShadowReadError> {
    let conn = open_conn(&state_dir.join("core.db"))?;
    conn.query_row(
        "SELECT workspace FROM sessions WHERE session_id = ?1",
        params![session_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .map(|value| value.unwrap_or_default())
    .map_err(ShadowReadError::from)
}

/// The model and mode a session was last run with, so a resumed run continues under the same
/// settings instead of the caller supplying them again.
pub fn session_model_mode(
    state_dir: &Path,
    session_id: &str,
) -> Result<(String, String), ShadowReadError> {
    let conn = open_conn(&state_dir.join("core.db"))?;
    conn.query_row(
        "SELECT model, mode FROM sessions WHERE session_id = ?1",
        params![session_id],
        |row| {
            Ok((
                row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                row.get::<_, Option<String>>(1)?
                    .unwrap_or_else(|| "interactive".to_string()),
            ))
        },
    )
    .map_err(ShadowReadError::from)
}

pub fn get_session_workspace(
    state_dir: &Path,
    session_id: &str,
) -> Result<String, ShadowReadError> {
    session_workspace(state_dir, session_id)
}

/// How long a kept earlier version of a replaced file stays in the workspace.
pub const KEPT_VERSION_DAYS: u64 = 30;
/// How long an abandoned staging folder stays (staged files are promoted at once; what is left
/// belongs to a run that failed).
const STAGING_LEFTOVER_DAYS: u64 = 7;
/// How long screenshots a tool took stay in the workspace (an audit trail of what the agent saw).
const SCREENSHOT_DAYS: u64 = 7;

fn remove_old_children(
    dir: &Path,
    now: std::time::SystemTime,
    max_age: std::time::Duration,
) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > max_age);
        // Only folders the runtime made (one per run); anything else in there is left alone.
        if old && path.is_dir() && fs::remove_dir_all(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Delete what the runtime keeps in a workspace for safety once it is old: earlier versions of
/// replaced files after [`KEPT_VERSION_DAYS`], abandoned staging folders after a week. Best effort:
/// a folder that cannot be removed is left for next time. Returns how many folders went.
pub fn prune_workspace_leftovers(workspace: &Path, now: std::time::SystemTime) -> usize {
    let day = std::time::Duration::from_secs(24 * 60 * 60);
    let delta = workspace.join(".delta");
    remove_old_children(&delta.join("versions"), now, day * KEPT_VERSION_DAYS as u32)
        + remove_old_children(
            &delta.join("staging"),
            now,
            day * STAGING_LEFTOVER_DAYS as u32,
        )
        + remove_old_children(&delta.join("screens"), now, day * SCREENSHOT_DAYS as u32)
}

/// Put a kept earlier version of a file back (the undo for a file the agent replaced).
///
/// `backup` must be a file under the workspace's `.delta/versions/`, `target` a workspace-relative
/// path outside `.delta`. The version being replaced is itself kept under
/// `.delta/versions/restored-<time>/`, so a restore can be undone too.
pub fn restore_file_version(
    state_dir: &Path,
    session_id: &str,
    backup: &str,
    target: &str,
) -> Result<Value, ShadowReadError> {
    let workspace = get_session_workspace(state_dir, session_id)?;
    if workspace.is_empty() {
        return Ok(json!({"ok": false, "error": "this task has no workspace"}));
    }
    restore_version_in(Path::new(&workspace), backup, target)
}

fn restore_version_in(
    workspace: &Path,
    backup: &str,
    target: &str,
) -> Result<Value, ShadowReadError> {
    let fail = |message: &str| Ok(json!({"ok": false, "error": message}));
    let Ok(workspace) = workspace.canonicalize() else {
        return fail("the workspace is unavailable");
    };
    let relative_ok = |path: &str| {
        let path = Path::new(path);
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
    };
    if !relative_ok(backup) || !relative_ok(target) {
        return fail("paths must be relative to the workspace");
    }
    let versions = workspace.join(".delta").join("versions");
    let Ok(source) = workspace.join(backup).canonicalize() else {
        return fail("that earlier version is no longer available");
    };
    if !source.starts_with(&versions) || !source.is_file() {
        return fail("that is not a kept earlier version");
    }
    let destination = workspace.join(target);
    if destination.starts_with(workspace.join(".delta")) {
        return fail("that location is reserved");
    }
    if destination.exists() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let kept = versions.join(format!("restored-{stamp}")).join(target);
        if let Some(parent) = kept.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&destination, &kept)?;
    } else if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = fs::read(&source)?;
    atomic_write(&destination, &bytes)?;
    Ok(json!({"ok": true, "path": target}))
}

fn rewrite_messages(
    state_dir: &Path,
    session_id: &str,
    messages: &[Value],
) -> Result<(), ShadowReadError> {
    if !safe_session_id(session_id) {
        return Err(ShadowReadError::Parse("unsafe session id".to_string()));
    }
    fs::create_dir_all(conversations_dir(state_dir))?;
    let path = conversations_dir(state_dir).join(format!("{session_id}.jsonl"));
    let mut file = fs::File::create(path)?;
    for message in messages {
        serde_json::to_writer(&mut file, message)?;
        file.write_all(b"\n")?;
    }
    file.flush()?;
    let conn = open_conn(&state_dir.join("core.db"))?;
    conn.execute(
        "UPDATE sessions SET n_msgs = ?1, updated_at = CURRENT_TIMESTAMP WHERE session_id = ?2",
        params![messages.len() as i64, session_id],
    )?;
    Ok(())
}

/// Truncate a transcript from `index` onward and return the original user text
/// for composer prefill.
pub fn revert_session(
    state_dir: &Path,
    session_id: &str,
    index: usize,
) -> Result<Value, ShadowReadError> {
    let messages = load_messages(state_dir, session_id);
    let Some(target) = messages.get(index) else {
        return Ok(json!({"ok": false, "error": "message index is out of range"}));
    };
    let text = target
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    rewrite_messages(state_dir, session_id, &messages[..index])?;
    Ok(json!({"ok": true, "text": text}))
}

pub fn set_reasoning_effort(
    state_dir: &Path,
    session_id: &str,
    effort: &str,
) -> Result<Value, ShadowReadError> {
    if !matches!(effort, "auto" | "low" | "medium" | "high") {
        return Ok(json!({"ok": false, "error": "unsupported reasoning effort"}));
    }
    let conn = open_conn(&state_dir.join("core.db"))?;
    let changed = conn.execute(
        "UPDATE sessions SET reasoning_effort = ?1, updated_at = CURRENT_TIMESTAMP WHERE session_id = ?2",
        params![effort, session_id],
    )?;
    Ok(json!({"ok": changed > 0, "reasoning_effort": effort}))
}

pub fn get_reasoning_effort(state_dir: &Path, session_id: &str) -> Result<String, ShadowReadError> {
    let conn = open_conn(&state_dir.join("core.db"))?;
    conn.query_row(
        "SELECT reasoning_effort FROM sessions WHERE session_id = ?1",
        params![session_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .map(|value| value.unwrap_or_else(|| "auto".to_string()))
    .map_err(ShadowReadError::from)
}

pub fn set_session_mode(
    state_dir: &Path,
    session_id: &str,
    mode: &str,
) -> Result<Value, ShadowReadError> {
    if !matches!(mode, "interactive" | "plan" | "unattended") {
        return Ok(json!({"ok": false, "error": "unsupported session mode"}));
    }
    let conn = open_conn(&state_dir.join("core.db"))?;
    let changed = conn.execute(
        "UPDATE sessions SET mode = ?1, unattended = ?2, updated_at = CURRENT_TIMESTAMP WHERE session_id = ?3",
        params![mode, (mode == "unattended") as i64, session_id],
    )?;
    Ok(json!({"ok": changed > 0, "mode": mode}))
}

pub fn get_unattended(state_dir: &Path, session_id: &str) -> Result<bool, ShadowReadError> {
    let conn = open_conn(&state_dir.join("core.db"))?;
    conn.query_row(
        "SELECT unattended FROM sessions WHERE session_id = ?1",
        params![session_id],
        |row| Ok(row.get::<_, i64>(0)? != 0),
    )
    .map_err(ShadowReadError::from)
}

pub fn get_session_recovery(
    state_dir: &Path,
    session_id: &str,
) -> Result<Option<Value>, ShadowReadError> {
    if !safe_session_id(session_id) {
        return Err(ShadowReadError::Parse("unsafe session id".to_string()));
    }
    let db_path = state_dir.join("core.db");
    if !db_path.exists() {
        return Ok(None);
    }
    let conn = open_conn(&db_path)?;
    let mut stmt = conn.prepare("SELECT recovery FROM sessions WHERE session_id = ?1")?;
    let mut rows = stmt.query(params![session_id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let raw: Option<String> = row.get(0)?;
    raw.filter(|value| !value.trim().is_empty())
        .map(|value| serde_json::from_str::<Value>(&value).map_err(ShadowReadError::from))
        .transpose()
}

pub fn set_unattended(
    state_dir: &Path,
    session_id: &str,
    is_unattended: bool,
) -> Result<Value, ShadowReadError> {
    let unattended = is_unattended;
    let conn = open_conn(&state_dir.join("core.db"))?;
    let changed = conn.execute(
        "UPDATE sessions SET unattended = ?1, updated_at = CURRENT_TIMESTAMP WHERE session_id = ?2",
        params![unattended as i64, session_id],
    )?;
    Ok(json!({"ok": changed > 0, "unattended": unattended}))
}

/// Persist a session's context compaction state: the summary and the
/// transcript boundary it replaces (`messages[..boundary]` collapse into the
/// summary for outbound requests; the transcript itself is never rewritten).
pub fn set_session_compaction(
    state_dir: &Path,
    session_id: &str,
    compaction: &Value,
) -> Result<(), ShadowReadError> {
    if !safe_session_id(session_id) {
        return Err(ShadowReadError::Parse("unsafe session id".to_string()));
    }
    let payload = serde_json::to_string(compaction).map_err(ShadowReadError::from)?;
    let conn = open_conn(&state_dir.join("core.db"))?;
    conn.execute(
        "UPDATE sessions SET compaction = ?1, updated_at = CURRENT_TIMESTAMP WHERE session_id = ?2",
        params![payload, session_id],
    )?;
    Ok(())
}

pub fn get_session_compaction(
    state_dir: &Path,
    session_id: &str,
) -> Result<Option<Value>, ShadowReadError> {
    if !safe_session_id(session_id) {
        return Err(ShadowReadError::Parse("unsafe session id".to_string()));
    }
    let db_path = state_dir.join("core.db");
    if !db_path.exists() {
        return Ok(None);
    }
    let conn = open_conn(&db_path)?;
    let mut stmt = conn.prepare("SELECT compaction FROM sessions WHERE session_id = ?1")?;
    let mut rows = stmt.query(params![session_id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let raw: Option<String> = row.get(0)?;
    raw.filter(|value| !value.trim().is_empty())
        .map(|value| serde_json::from_str::<Value>(&value).map_err(ShadowReadError::from))
        .transpose()
}

fn parse_roots(raw: Option<String>) -> Vec<Value> {
    raw.and_then(|value| serde_json::from_str::<Vec<Value>>(&value).ok())
        .unwrap_or_default()
}

pub fn list_roots(state_dir: &Path, session_id: &str) -> Result<Value, ShadowReadError> {
    let conn = open_conn(&state_dir.join("core.db"))?;
    let (workspace, extra): (Option<String>, Option<String>) = conn.query_row(
        "SELECT workspace, extra_roots FROM sessions WHERE session_id = ?1",
        params![session_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let mut roots = Vec::new();
    if let Some(workspace) = workspace.filter(|value| !value.is_empty()) {
        let path = PathBuf::from(&workspace);
        roots.push(json!({
            "path": workspace,
            "writable": true,
            "label": path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default(),
            "primary": true,
            "exists": path.is_dir(),
        }));
    }
    roots.extend(parse_roots(extra).into_iter().map(|mut root| {
        if let Some(object) = root.as_object_mut() {
            let path = object
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            object.insert("primary".to_string(), Value::Bool(false));
            object.insert("exists".to_string(), Value::Bool(Path::new(&path).is_dir()));
            object.entry("label".to_string()).or_insert_with(|| {
                Value::String(
                    Path::new(&path)
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_default(),
                )
            });
        }
        root
    }));
    Ok(json!({"roots": roots}))
}

pub fn add_root(
    state_dir: &Path,
    session_id: &str,
    path: &str,
    is_writable: bool,
) -> Result<Value, ShadowReadError> {
    let writable = is_writable;
    let canonical = PathBuf::from(path)
        .canonicalize()
        .map_err(|error| ShadowReadError::Parse(format!("root is unavailable: {error}")))?;
    if !canonical.is_dir() {
        return Ok(json!({"ok": false, "error": "root must be a directory"}));
    }
    let conn = open_conn(&state_dir.join("core.db"))?;
    let extra = conn.query_row(
        "SELECT extra_roots FROM sessions WHERE session_id = ?1",
        params![session_id],
        |row| row.get::<_, Option<String>>(0),
    )?;
    let canonical = canonical.to_string_lossy().to_string();
    let mut roots = parse_roots(extra);
    if let Some(root) = roots
        .iter_mut()
        .find(|root| root.get("path").and_then(Value::as_str) == Some(canonical.as_str()))
    {
        root["writable"] = Value::Bool(writable);
    } else {
        roots.push(json!({"path": canonical, "writable": writable}));
    }
    conn.execute(
        "UPDATE sessions SET extra_roots = ?1, updated_at = CURRENT_TIMESTAMP WHERE session_id = ?2",
        params![serde_json::to_string(&roots)?, session_id],
    )?;
    let mut response = list_roots(state_dir, session_id)?;
    response["ok"] = Value::Bool(true);
    Ok(response)
}

pub fn remove_root(
    state_dir: &Path,
    session_id: &str,
    path: &str,
) -> Result<Value, ShadowReadError> {
    let conn = open_conn(&state_dir.join("core.db"))?;
    let extra = conn.query_row(
        "SELECT extra_roots FROM sessions WHERE session_id = ?1",
        params![session_id],
        |row| row.get::<_, Option<String>>(0),
    )?;
    let mut roots = parse_roots(extra);
    roots.retain(|root| root.get("path").and_then(Value::as_str) != Some(path));
    conn.execute(
        "UPDATE sessions SET extra_roots = ?1, updated_at = CURRENT_TIMESTAMP WHERE session_id = ?2",
        params![serde_json::to_string(&roots)?, session_id],
    )?;
    let mut response = list_roots(state_dir, session_id)?;
    response["ok"] = Value::Bool(true);
    Ok(response)
}

fn trust_path(state_dir: &Path) -> PathBuf {
    state_dir.join("workspace_trust.json")
}

fn trusted_workspace_paths(state_dir: &Path) -> Result<Vec<String>, ShadowReadError> {
    let path = trust_path(state_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&path).map_err(|error| {
        ShadowReadError::Parse(format!(
            "failed to read workspace trust authority {}: {error}",
            path.display()
        ))
    })?;
    let value = serde_json::from_str::<Value>(&text).map_err(|error| {
        ShadowReadError::Parse(format!(
            "invalid workspace trust authority {}: {error}",
            path.display()
        ))
    })?;
    let object = value.as_object().ok_or_else(|| {
        ShadowReadError::Parse(format!(
            "workspace trust authority {} must be a JSON object",
            path.display()
        ))
    })?;
    let entries = object
        .get("trusted_workspaces")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ShadowReadError::Parse(format!(
                "workspace trust authority {} must contain a trusted_workspaces array",
                path.display()
            ))
        })?;
    let mut paths = Vec::with_capacity(entries.len());
    for entry in entries {
        let value = entry.as_str().ok_or_else(|| {
            ShadowReadError::Parse(format!(
                "workspace trust authority {} contains a non-string workspace path",
                path.display()
            ))
        })?;
        paths.push(value.to_string());
    }
    Ok(paths)
}

fn requested_commands(workspace: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(workspace.join(".delta").join("config.toml")) else {
        return Vec::new();
    };
    let Some(start) = text.find("allowed_commands") else {
        return Vec::new();
    };
    let tail = &text[start..];
    let Some(open) = tail.find('[') else {
        return Vec::new();
    };
    let Some(close) = tail[open + 1..].find(']') else {
        return Vec::new();
    };
    tail[open + 1..open + 1 + close]
        .split(',')
        .map(|item| item.trim().trim_matches(['\'', '"']).to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn build_trust_dto(workspace: &Path, trusted_paths: &[String]) -> Value {
    let canonical = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf())
        .to_string_lossy()
        .to_string();
    let commands = requested_commands(workspace);
    let trusted = trusted_paths.contains(&canonical);
    json!({
        "workspace": canonical,
        "requested_commands": commands,
        "trusted": trusted,
        "required": !trusted && !commands.is_empty(),
        "exists": workspace.is_dir(),
    })
}

pub fn open_workspace(
    state_dir: &Path,
    path: &str,
    should_create: bool,
) -> Result<Value, ShadowReadError> {
    let create = should_create;
    let trusted_paths = trusted_workspace_paths(state_dir)?;
    let requested = PathBuf::from(path);
    if create && !requested.exists() {
        fs::create_dir_all(&requested)?;
    }
    let canonical = requested
        .canonicalize()
        .map_err(|error| ShadowReadError::Parse(format!("workspace is unavailable: {error}")))?;
    if !canonical.is_dir() {
        return Ok(json!({"ok": false, "path": path, "error": "workspace must be a directory"}));
    }
    let canonical_text = canonical.to_string_lossy().to_string();
    let command_trust = build_trust_dto(&canonical, &trusted_paths);
    let conn = open_conn(&state_dir.join("core.db"))?;
    conn.execute(
        "INSERT INTO workspaces (path, last_used) VALUES (?1, CURRENT_TIMESTAMP)
         ON CONFLICT(path) DO UPDATE SET last_used = CURRENT_TIMESTAMP",
        params![canonical_text],
    )?;
    let git_branch = std::process::Command::new("git")
        .args(["-C", &canonical_text, "branch", "--show-current"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|branch| branch.trim().to_string())
        .filter(|branch| !branch.is_empty());
    Ok(json!({
        "ok": true,
        "path": canonical_text,
        "git_branch": git_branch,
        "command_trust": command_trust,
    }))
}

pub fn list_trusted_workspaces(state_dir: &Path) -> Result<Value, ShadowReadError> {
    let paths = trusted_workspace_paths(state_dir)?;
    let workspaces = paths
        .iter()
        .map(|path| build_trust_dto(Path::new(path), &paths))
        .collect::<Vec<_>>();
    Ok(json!({"workspaces": workspaces}))
}

pub fn set_workspace_trusted(
    state_dir: &Path,
    path: &str,
    is_trusted: bool,
) -> Result<Value, ShadowReadError> {
    let trusted = is_trusted;
    let canonical = PathBuf::from(path)
        .canonicalize()
        .map_err(|error| ShadowReadError::Parse(format!("workspace is unavailable: {error}")))?;
    let canonical_text = canonical.to_string_lossy().to_string();
    let mut paths = trusted_workspace_paths(state_dir)?;
    paths.retain(|item| item != &canonical_text);
    if trusted {
        paths.push(canonical_text);
        paths.sort();
    }
    let bytes = serde_json::to_vec_pretty(&json!({"trusted_workspaces": paths}))?;
    atomic_write(trust_path(state_dir), &bytes)?;
    let mut response = build_trust_dto(&canonical, &paths);
    response["ok"] = Value::Bool(true);
    Ok(response)
}

fn session_run_ids(state_dir: &Path, session_id: &str) -> Vec<String> {
    let mut run_ids = Vec::new();
    for message in load_messages(state_dir, session_id) {
        if let Some(run_id) = message.get("run_id").and_then(Value::as_str) {
            if !run_ids.iter().any(|value| value == run_id) {
                run_ids.push(run_id.to_string());
            }
        }
    }
    run_ids
}

pub fn list_artifacts(state_dir: &Path, session_id: &str) -> Result<Value, ShadowReadError> {
    let ledger_path = state_dir.join("run_events.db");
    if !ledger_path.exists() {
        return Ok(json!({"artifacts": []}));
    }
    let workspace = session_workspace(state_dir, session_id)?;
    let reader = crate::artifact::ArtifactReader::open(ledger_path)?;
    let mut artifacts = Vec::new();
    for run_id in session_run_ids(state_dir, session_id) {
        for artifact in reader.list_for_run(&run_id)? {
            let absolute = PathBuf::from(&workspace).join(&artifact.path);
            let metadata = fs::metadata(&absolute).ok();
            let modified_at = metadata
                .as_ref()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs_f64())
                .unwrap_or_default();
            artifacts.push(json!({
                "path": artifact.path,
                "abs_path": absolute,
                "name": absolute.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default(),
                "kind": artifact.kind,
                "size": metadata.map(|metadata| metadata.len() as i64).unwrap_or(artifact.size),
                "modified_at": modified_at,
                "sha256": artifact.sha256,
                "run_id": run_id,
                "incomplete": artifact.incomplete,
            }));
        }
    }
    Ok(json!({"artifacts": artifacts}))
}

pub fn resolve_artifact_path(
    state_dir: &Path,
    session_id: &str,
    path: &str,
) -> Result<PathBuf, ShadowReadError> {
    let artifacts = list_artifacts(state_dir, session_id)?;
    let registered = artifacts["artifacts"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item["path"] == path));
    if !registered {
        return Err(ShadowReadError::Parse(
            "artifact is not registered for this session".to_string(),
        ));
    }
    let workspace = PathBuf::from(session_workspace(state_dir, session_id)?)
        .canonicalize()
        .map_err(|error| ShadowReadError::Parse(error.to_string()))?;
    let candidate = workspace.join(path);
    let canonical = candidate
        .canonicalize()
        .map_err(|error| ShadowReadError::Parse(error.to_string()))?;
    if !canonical.starts_with(&workspace) {
        return Err(ShadowReadError::Parse(
            "artifact escaped its workspace".to_string(),
        ));
    }
    Ok(canonical)
}

pub fn read_artifact(
    state_dir: &Path,
    session_id: &str,
    path: &str,
) -> Result<Value, ShadowReadError> {
    let absolute = resolve_artifact_path(state_dir, session_id, path)?;
    if absolute.is_dir() {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&absolute)?.flatten().take(500) {
            let metadata = entry.metadata().ok();
            entries.push(json!({
                "name": entry.file_name().to_string_lossy(),
                "dir": metadata.as_ref().is_some_and(fs::Metadata::is_dir),
                "size": metadata.map(|value| value.len()).unwrap_or_default(),
            }));
        }
        return Ok(json!({"ok": true, "path": path, "kind": "folder", "entries": entries}));
    }
    let bytes = fs::read(&absolute)?;
    let extension = absolute
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mime = match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "pdf" => Some("application/pdf"),
        _ => None,
    };
    if let Some(mime) = mime {
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        return Ok(json!({
            "ok": true, "path": path, "kind": if extension == "pdf" { "pdf" } else { "image" },
            "data_url": format!("data:{mime};base64,{encoded}"), "truncated": false,
        }));
    }
    let max = 2 * 1024 * 1024;
    let truncated = bytes.len() > max;
    let content = String::from_utf8_lossy(&bytes[..bytes.len().min(max)]).to_string();
    Ok(json!({
        "ok": true, "path": path, "kind": "text", "content": content,
        "truncated": truncated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_title_uses_first_line_and_clips() {
        assert_eq!(
            derive_title("  今天X有什么热点\n第二行").as_deref(),
            Some("今天X有什么热点")
        );
        assert_eq!(derive_title("a   b\tc").as_deref(), Some("a b c"));
        assert_eq!(derive_title("\n  \n"), None);
        let long = "字".repeat(AUTO_TITLE_MAX_CHARS + 5);
        let title = derive_title(&long).unwrap();
        assert_eq!(title.chars().count(), AUTO_TITLE_MAX_CHARS + 1);
        assert!(title.ends_with('…'));
    }

    #[test]
    fn a_failure_that_ends_the_turn_stays_in_the_transcript_but_retry_progress_does_not() {
        let tmp = tempfile::tempdir().unwrap();
        ensure_session(tmp.path(), "failing", None, "m").unwrap();
        let frame = |kind: &str, payload: Value| json!({"sessionId": "failing", "type": kind, "payload": payload});
        record_runtime_event(
            tmp.path(),
            &frame(
                "turn_start",
                json!({"input": "Find the news", "run_id": "r"}),
            ),
        )
        .unwrap();
        record_runtime_event(
            tmp.path(),
            &frame(
                "error",
                json!({"error": "retrying", "terminal": false, "attempt": 2}),
            ),
        )
        .unwrap();
        record_runtime_event(
            tmp.path(),
            &frame(
                "error",
                json!({"error": "HTTP 504: all nodes failed", "terminal": true}),
            ),
        )
        .unwrap();
        let messages = get_session_messages(tmp.path(), "failing").unwrap()["messages"].clone();
        let notices: Vec<&Value> = messages
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "notice")
            .collect();
        assert_eq!(notices.len(), 1, "{messages}");
        assert_eq!(notices[0]["kind"], "error");
        assert_eq!(notices[0]["text"], "HTTP 504: all nodes failed");
    }

    #[test]
    fn first_turn_names_the_session_and_listing_backfills_old_ones() {
        let tmp = tempfile::tempdir().unwrap();
        ensure_session(tmp.path(), "fresh", None, "m").unwrap();
        let start = |text: &str| {
            json!({
                "sessionId": "fresh", "type": "turn_start", "payload": {"input": text, "run_id": "r"},
            })
        };
        record_runtime_event(tmp.path(), &start("Summarize today's news")).unwrap();
        record_runtime_event(tmp.path(), &start("Second message")).unwrap();
        let listed = list_sessions(tmp.path(), None).unwrap();
        assert_eq!(listed["sessions"][0]["title"], "Summarize today's news");

        // A user rename is never overwritten.
        rename_session(tmp.path(), "fresh", "Mine").unwrap();
        record_runtime_event(tmp.path(), &start("Third")).unwrap();
        assert_eq!(
            list_sessions(tmp.path(), None).unwrap()["sessions"][0]["title"],
            "Mine"
        );

        // A legacy session with messages but no title is named on the next listing.
        ensure_session(tmp.path(), "legacy", None, "m").unwrap();
        append_session_message(
            tmp.path(),
            "legacy",
            &json!({"role": "user", "content": "Old question"}),
        )
        .unwrap();
        let listed = list_sessions(tmp.path(), None).unwrap();
        let legacy = listed["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["session_id"] == "legacy")
            .unwrap();
        assert_eq!(legacy["title"], "Old question");
    }

    #[test]
    fn a_kept_version_can_be_restored_and_the_restore_can_be_undone() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path();
        fs::create_dir_all(ws.join(".delta/versions/run-1/reports")).unwrap();
        fs::create_dir_all(ws.join("reports")).unwrap();
        fs::write(ws.join(".delta/versions/run-1/reports/a.md"), "original").unwrap();
        fs::write(ws.join("reports/a.md"), "replaced").unwrap();
        let backup = ".delta/versions/run-1/reports/a.md";
        let done = restore_version_in(ws, backup, "reports/a.md").unwrap();
        assert_eq!(done["ok"], true, "{done}");
        assert_eq!(
            fs::read_to_string(ws.join("reports/a.md")).unwrap(),
            "original"
        );
        // What was replaced is kept, so the restore is undoable.
        let kept: Vec<_> = fs::read_dir(ws.join(".delta/versions"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("restored-"))
            .collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(
            fs::read_to_string(kept[0].path().join("reports/a.md")).unwrap(),
            "replaced"
        );
    }

    #[test]
    fn restoring_refuses_paths_outside_the_kept_versions_or_into_reserved_space() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path();
        fs::create_dir_all(ws.join(".delta/versions/run-1")).unwrap();
        fs::write(ws.join(".delta/versions/run-1/a.md"), "x").unwrap();
        fs::write(ws.join("secret.txt"), "s").unwrap();
        for (backup, target) in [
            ("secret.txt", "out.txt"),
            ("../secret.txt", "out.txt"),
            (".delta/versions/run-1/a.md", "../out.txt"),
            (".delta/versions/run-1/a.md", ".delta/state.db"),
            (".delta/versions/run-1/a.md", "/abs/out.txt"),
            (".delta/versions/missing.md", "out.txt"),
            ("", "out.txt"),
        ] {
            let result = restore_version_in(ws, backup, target).unwrap();
            assert_eq!(result["ok"], false, "{backup} -> {target}");
        }
        assert!(!ws.join("out.txt").exists());
    }

    #[test]
    fn old_kept_versions_and_staging_leftovers_are_pruned_and_recent_ones_stay() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path();
        for dir in [
            ".delta/versions/run-a/x",
            ".delta/versions/restored-1",
            ".delta/staging/run-b/job",
        ] {
            fs::create_dir_all(ws.join(dir)).unwrap();
        }
        fs::write(ws.join(".delta/versions/run-a/x/f.md"), "old").unwrap();
        fs::write(ws.join(".delta/versions/not-a-run.txt"), "keep").unwrap();
        let now = std::time::SystemTime::now();
        let day = std::time::Duration::from_secs(24 * 60 * 60);
        // Just made: nothing goes.
        assert_eq!(prune_workspace_leftovers(ws, now), 0);
        // 8 days on: only the staging leftover is old enough.
        assert_eq!(prune_workspace_leftovers(ws, now + day * 8), 1);
        assert!(ws.join(".delta/versions/run-a/x/f.md").exists());
        assert!(!ws.join(".delta/staging/run-b").exists());
        // 31 days on: the kept versions go too; a plain file in `versions` is never touched.
        assert_eq!(prune_workspace_leftovers(ws, now + day * 31), 2);
        assert!(!ws.join(".delta/versions/run-a").exists());
        assert!(ws.join(".delta/versions/not-a-run.txt").exists());
        // Screenshots a tool took are kept a week, like staging leftovers.
        fs::create_dir_all(ws.join(".delta/screens/run-c")).unwrap();
        assert_eq!(prune_workspace_leftovers(ws, now + day * 8), 1);
        assert!(!ws.join(".delta/screens/run-c").exists());
        // No `.delta` folder at all is fine.
        assert_eq!(prune_workspace_leftovers(&ws.join("missing"), now), 0);
    }

    #[test]
    fn list_sessions_empty_when_no_db() {
        let tmp = tempfile::tempdir().unwrap();
        let v = list_sessions(tmp.path(), None).unwrap();
        assert_eq!(v.get("sessions").unwrap().as_array().unwrap().len(), 0);
    }

    #[test]
    fn session_crud_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("core.db");
        let conn = open_conn(&db).unwrap();
        conn.execute(
            "INSERT INTO sessions (session_id, workspace, model, mode, title, agent, n_msgs) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params!["s1", "/ws", "gpt-5.5", "interactive", "Hello", "delta", 2],
        )
        .unwrap();

        let listed = list_sessions(tmp.path(), None).unwrap();
        let arr = listed.get("sessions").unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["session_id"], "s1");
        assert_eq!(arr[0]["title"], "Hello");
        assert_eq!(arr[0]["messages"], 2);

        // rename
        let r = rename_session(tmp.path(), "s1", "Renamed").unwrap();
        assert_eq!(r["ok"], true);
        let listed2 = list_sessions(tmp.path(), None).unwrap();
        assert_eq!(listed2["sessions"][0]["title"], "Renamed");

        // flags
        set_session_flags(tmp.path(), "s1", Some(true), None).unwrap();
        let listed3 = list_sessions(tmp.path(), None).unwrap();
        assert_eq!(listed3["sessions"][0]["pinned"], true);

        // delete
        let d = delete_session(tmp.path(), "s1").unwrap();
        assert_eq!(d["ok"], true);
        let listed4 = list_sessions(tmp.path(), None).unwrap();
        assert_eq!(listed4["sessions"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn delete_session_refuses_an_id_that_is_not_a_plain_name() {
        let tmp = tempfile::tempdir().unwrap();
        let state = tmp.path().join("state");
        fs::create_dir_all(conversations_dir(&state)).unwrap();
        let inside = state.join("victim.jsonl");
        let outside = tmp.path().join("outside.jsonl");
        fs::write(&inside, "keep").unwrap();
        fs::write(&outside, "keep").unwrap();
        for id in ["../victim", "../../outside", "a/b", "..", ""] {
            assert!(delete_session(&state, id).is_err(), "{id:?} was accepted");
        }
        assert!(
            inside.exists() && outside.exists(),
            "a file outside conversations/ was deleted"
        );
    }

    #[test]
    fn messages_from_jsonl() {
        let tmp = tempfile::tempdir().unwrap();
        let convo = conversations_dir(tmp.path());
        fs::create_dir_all(&convo).unwrap();
        fs::write(convo.join("s1.jsonl"), r#"{"role":"user","content":"hi"}"#).unwrap();
        let v = get_session_messages(tmp.path(), "s1").unwrap();
        let msgs = v.get("messages").unwrap().as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    #[test]
    fn workspace_trust_missing_file_is_valid_empty_state() {
        let state = tempfile::tempdir().unwrap();
        let listed = list_trusted_workspaces(state.path()).unwrap();
        assert_eq!(listed["workspaces"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn corrupt_workspace_trust_fails_closed_and_is_not_overwritten() {
        let state = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let authority_path = trust_path(state.path());
        fs::write(&authority_path, b"{broken").unwrap();
        let before = fs::read(&authority_path).unwrap();

        assert!(list_trusted_workspaces(state.path()).is_err());
        let uncreated = state.path().join("must-not-be-created");
        assert!(open_workspace(state.path(), uncreated.to_str().unwrap(), true).is_err());
        assert!(!uncreated.exists());
        assert!(open_workspace(state.path(), workspace.path().to_str().unwrap(), false).is_err());
        assert!(!state.path().join("core.db").exists());
        assert!(
            set_workspace_trusted(state.path(), workspace.path().to_str().unwrap(), true).is_err()
        );
        assert_eq!(fs::read(&authority_path).unwrap(), before);
    }

    #[test]
    fn invalid_workspace_trust_shapes_fail_closed() {
        let state = tempfile::tempdir().unwrap();
        let authority_path = trust_path(state.path());
        for invalid in [
            "[]",
            "{}",
            r#"{"trusted_workspaces":"not-an-array"}"#,
            r#"{"trusted_workspaces":[1]}"#,
        ] {
            fs::write(&authority_path, invalid).unwrap();
            assert!(list_trusted_workspaces(state.path()).is_err(), "{invalid}");
        }
    }

    #[test]
    fn workspace_trust_roundtrip_uses_single_rust_authority() {
        let state = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let workspace_path = workspace.path().to_str().unwrap();

        let trusted = set_workspace_trusted(state.path(), workspace_path, true).unwrap();
        assert_eq!(trusted["trusted"], true);
        let listed = list_trusted_workspaces(state.path()).unwrap();
        assert_eq!(listed["workspaces"].as_array().unwrap().len(), 1);

        let untrusted = set_workspace_trusted(state.path(), workspace_path, false).unwrap();
        assert_eq!(untrusted["trusted"], false);
        let listed = list_trusted_workspaces(state.path()).unwrap();
        assert_eq!(listed["workspaces"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn workspace_filter_and_recent() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("core.db");
        let conn = open_conn(&db).unwrap();
        conn.execute(
            "INSERT INTO sessions (session_id, workspace, model, mode, title) VALUES (?1,?2,?3,?4,?5)",
            params!["a", "/wsA", "m", "interactive", "A"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sessions (session_id, workspace, model, mode, title) VALUES (?1,?2,?3,?4,?5)",
            params!["b", "/wsB", "m", "interactive", "B"],
        )
        .unwrap();
        let filtered = list_sessions(tmp.path(), Some("/wsA")).unwrap();
        let arr = filtered["sessions"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["session_id"], "a");

        let recent = list_recent_workspaces(tmp.path()).unwrap();
        let ws = recent["workspaces"].as_array().unwrap();
        assert_eq!(ws.len(), 2);
    }
}
