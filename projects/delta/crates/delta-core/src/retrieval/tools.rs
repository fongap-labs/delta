//! The model's two tools for the library: search it, and bring a folder's index up to date.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use super::{Library, DEFAULT_LIMIT};
use crate::capability::{
    CapabilityGrants, CapabilityProgress, CapabilityRegistration, CapabilityResult,
    NativeCapabilityRunner,
};
use crate::skills::SideEffectClass;

/// How long one `library_index` call may read before it stops and says to call it again.
const INDEX_BUDGET: Duration = Duration::from_secs(300);

pub fn registrations(library: Arc<Library>) -> Vec<CapabilityRegistration> {
    vec![
        search_registration(library.clone()),
        index_registration(library),
    ]
}

fn search_registration(library: Arc<Library>) -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "library.search".to_owned(),
        tool_name: "library_search".to_owned(),
        side_effect: SideEffectClass::Read,
        description: "Search the local library: the folders the user put in it and this session may \
            read. Matches words and phrases in any language (3 or more characters), in text, \
            Markdown, CSV, HTML, PDF, Word, Excel and PowerPoint files. Each hit has the file path, \
            where in the file (lines, page, sheet and rows, slide), a short snippet, the file's \
            SHA-256 as indexed, and `stale` when the file changed since. A snippet is a lead, not \
            the source: read the file with read_file or read_document before quoting it, and cite \
            the path and place. Folders not yet indexed are listed in `not_indexed`; nothing is \
            sent anywhere."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["query"],
            "properties": {
                "query": {"type": "string", "description": "Words or phrases that must all appear in the passage."},
                "limit": {"type": "integer", "description": "How many hits, 1 to 20. Default 8."}
            }
        }),
        metadata: json!({
            "risk_level": "low", "requires_approval": false,
            "category": "read", "capabilities": ["library.search"]
        }),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(move |job, control| {
            let Some(query) = job.arguments.get("query").and_then(Value::as_str) else {
                return CapabilityResult::failed(&job.job_id, "query is required", Some("arguments"));
            };
            let limit = job
                .arguments
                .get("limit")
                .and_then(Value::as_u64)
                .map_or(DEFAULT_LIMIT, |limit| limit as usize);
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            match library.search(&job.grants.read_roots, query, limit) {
                Ok(outcome) => CapabilityResult::completed(
                    &job.job_id,
                    json!({
                        "ok": true,
                        "query": query,
                        "hits": outcome.hits.iter().map(super::Hit::to_json).collect::<Vec<_>>(),
                        "not_indexed": outcome.not_indexed,
                    }),
                ),
                Err(error) => CapabilityResult::failed(&job.job_id, &error, Some("search_failed")),
            }
        })),
    }
}

fn index_registration(library: Arc<Library>) -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "library.index".to_owned(),
        tool_name: "library_index".to_owned(),
        side_effect: SideEffectClass::LocalWrite,
        description: "Bring the library's index of one folder up to date, so library_search can \
            find what is in it. Only files that changed are read again. The folder must be one this \
            session may read (default: the workspace). It stops after a few minutes and says so; \
            call it again to carry on. Nothing leaves the computer."
            .to_owned(),
        parameters: json!({
            "type": "object",
            "properties": {
                "folder": {"type": "string", "description": "The folder to index, relative to the workspace or absolute. Default: the workspace."}
            }
        }),
        metadata: json!({
            "risk_level": "medium", "requires_approval": true,
            "category": "filesystem", "capabilities": ["library.index"]
        }),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(move |job, control| {
            let requested = job
                .arguments
                .get("folder")
                .and_then(Value::as_str)
                .filter(|folder| !folder.trim().is_empty())
                .unwrap_or(".");
            let resolved = match crate::capability::resolve_read_path(job, requested) {
                Ok(resolved) => resolved,
                Err((code, message)) => return CapabilityResult::failed(&job.job_id, &message, Some(code)),
            };
            let job_id = job.job_id.clone();
            let report = library.index_root(
                &resolved.to_string_lossy(),
                INDEX_BUDGET,
                &|| control.is_cancelled(),
                &|done, total| {
                    control.emit_progress(CapabilityProgress {
                        job_id: job_id.clone(),
                        fraction: if total == 0 { 1.0 } else { done as f64 / total as f64 },
                        stage: Some("indexing".to_owned()),
                        message: Some(format!("{done} of {total} files")),
                    });
                },
            );
            match report {
                Ok(report) => {
                    let mut out = report.to_json();
                    out["ok"] = json!(true);
                    if !report.is_complete {
                        out["note"] = json!("Stopped before the end of the folder; call library_index again to carry on.");
                    }
                    CapabilityResult::completed(&job.job_id, out)
                }
                Err(error) => CapabilityResult::failed(&job.job_id, &error, Some("index_failed")),
            }
        })),
    }
}
