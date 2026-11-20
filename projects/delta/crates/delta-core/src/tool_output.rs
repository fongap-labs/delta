//! Complete tool results owned by a session; model-facing views remain bounded.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::capability::{
    CapabilityGrants, CapabilityRegistration, CapabilityResult, NativeCapabilityRunner,
};
use crate::skills::SideEffectClass;

pub(crate) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS tool_outputs (
    id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    workspace TEXT NOT NULL, run_id TEXT NOT NULL, tool_call_id TEXT NOT NULL,
    sha256 TEXT NOT NULL, content TEXT NOT NULL
); CREATE INDEX IF NOT EXISTS tool_outputs_session ON tool_outputs(session_id);";

const PAGE_BYTES: usize = 16 * 1024;

pub(crate) fn save(
    state_dir: &Path,
    session_id: &str,
    workspace: &str,
    run_id: &str,
    tool_call_id: &str,
    text: &str,
) -> Result<Value, String> {
    let conn = crate::control_plane::open_conn(&state_dir.join("core.db"))
        .map_err(|error| error.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let sha256 = format!("{:x}", Sha256::digest(text.as_bytes()));
    let changed = conn.execute(
        "INSERT INTO tool_outputs (id, session_id, workspace, run_id, tool_call_id, sha256, content)
         SELECT ?1, session_id, ?3, ?4, ?5, ?6, ?7 FROM sessions
         WHERE session_id = ?2 AND COALESCE(workspace, '') = ?3",
        params![id, session_id, workspace, run_id, tool_call_id, sha256, text],
    ).map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("tool output session or workspace does not match".to_string());
    }
    Ok(json!({"id": id, "sha256": sha256, "bytes": text.len(), "format": "json"}))
}

fn read_page(
    state_dir: &Path,
    session_id: &str,
    workspace: &str,
    id: &str,
    offset: usize,
) -> Result<Value, String> {
    let conn = crate::control_plane::open_conn(&state_dir.join("core.db"))
        .map_err(|error| error.to_string())?;
    let stored: Option<(String, String, String, String)> = conn
        .query_row(
            "SELECT content, sha256, run_id, tool_call_id FROM tool_outputs
         WHERE id = ?1 AND session_id = ?2 AND workspace = ?3",
            params![id, session_id, workspace],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (text, sha256, run_id, tool_call_id) = stored
        .ok_or_else(|| "tool output is unavailable in this session and workspace".to_string())?;
    if format!("{:x}", Sha256::digest(text.as_bytes())) != sha256 {
        return Err("stored tool output failed its integrity check".to_string());
    }
    if offset > text.len() || !text.is_char_boundary(offset) {
        return Err("offset must be a UTF-8 byte boundary within the stored output".to_string());
    }
    let mut end = offset.saturating_add(PAGE_BYTES).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok(json!({
        "id": id, "sha256": sha256, "run_id": run_id, "tool_call_id": tool_call_id,
        "format": "json", "text": &text[offset..end], "offset": offset,
        "next_offset": end, "total_bytes": text.len(), "truncated": end < text.len()
    }))
}

pub(crate) fn registration(state_dir: PathBuf) -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "tool_output.read".to_string(),
        tool_name: "read_tool_output".to_string(),
        description: "Read a page of a complete tool result using its output_ref.id. Offsets are UTF-8 bytes; pass next_offset to continue. Only this session's results are accessible.".to_string(),
        parameters: json!({"type": "object", "required": ["id"], "properties": {
            "id": {"type": "string"}, "offset": {"type": "integer", "minimum": 0}
        }, "additionalProperties": false}),
        metadata: json!({"risk_level": "low", "requires_approval": false, "category": "tool_output"}),
        side_effect: SideEffectClass::Read,
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(move |job, control| {
            let outcome = (|| {
                if control.is_cancelled() {
                    return Err("tool output read cancelled".to_string());
                }
                let session = job.session_id.as_deref().filter(|s| !s.is_empty())
                    .ok_or_else(|| "tool output read requires a session".to_string())?;
                let id = job.arguments.get("id").and_then(Value::as_str)
                    .ok_or_else(|| "id is required".to_string())?;
                let offset = match job.arguments.get("offset") {
                    None => 0,
                    Some(value) => value.as_u64().and_then(|n| usize::try_from(n).ok())
                        .ok_or_else(|| "offset must be a non-negative byte offset".to_string())?,
                };
                read_page(&state_dir, session, job.workspace.as_deref().unwrap_or_default(), id, offset)
            })();
            match outcome {
                Ok(output) => CapabilityResult::completed(&job.job_id, output),
                Err(error) => CapabilityResult::failed(&job.job_id, &error, Some("tool_output_read")),
            }
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_reconstruct_unicode_json_and_are_scoped_and_deleted_with_the_session() {
        let dir = tempfile::tempdir().unwrap();
        crate::control_plane::ensure_session(dir.path(), "s", Some("workspace"), "m").unwrap();
        let text = json!({"text": "证据🙂\n".repeat(40_000)}).to_string();
        let reference = save(dir.path(), "s", "workspace", "run", "call", &text).unwrap();
        let id = reference["id"].as_str().unwrap();
        assert!(read_page(dir.path(), "other", "workspace", id, 0).is_err());
        assert!(read_page(dir.path(), "s", "other", id, 0).is_err());
        let mut rebuilt = String::new();
        let mut offset = 0;
        loop {
            let page = read_page(dir.path(), "s", "workspace", id, offset).unwrap();
            assert!(page.to_string().len() < 128 * 1024);
            rebuilt.push_str(page["text"].as_str().unwrap());
            offset = page["next_offset"].as_u64().unwrap() as usize;
            if page["truncated"] == false {
                break;
            }
        }
        assert_eq!(rebuilt, text);
        assert!(read_page(dir.path(), "s", "workspace", id, text.len() + 1).is_err());
        let inside_unicode = text.find('证').unwrap() + 1;
        assert!(read_page(dir.path(), "s", "workspace", id, inside_unicode).is_err());
        crate::control_plane::delete_session(dir.path(), "s").unwrap();
        assert!(read_page(dir.path(), "s", "workspace", id, 0).is_err());
    }

    #[test]
    fn missing_owner_and_corrupted_content_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        assert!(save(dir.path(), "missing", "", "run", "call", "{}").is_err());
        crate::control_plane::ensure_session(dir.path(), "s", None, "m").unwrap();
        let reference = save(dir.path(), "s", "", "run", "call", "{}").unwrap();
        let conn = crate::control_plane::open_conn(&dir.path().join("core.db")).unwrap();
        conn.execute("UPDATE tool_outputs SET content = 'changed'", [])
            .unwrap();
        assert!(read_page(dir.path(), "s", "", reference["id"].as_str().unwrap(), 0).is_err());
    }
}
