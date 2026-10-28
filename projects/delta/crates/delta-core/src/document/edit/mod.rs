//! Editing a Word, Excel or PowerPoint file that already exists (A1).
//!
//! `edit_document` makes small, located changes and leaves everything else alone:
//! - `.docx`: replace text inside paragraphs (`find` / `replace`), keeping the formatting of the
//!   run where the text starts;
//! - `.pptx`: the same, on one slide;
//! - `.xlsx`: set a cell or a range of cells, keeping each cell's style and every other cell.
//!
//! The edited package is built by copying every part it did not change byte for byte. Before it is
//! offered, it is read back and compared with the original: the same parts, the same number of
//! paragraphs or rows, and only the targeted ones different. The file reaches the workspace as a
//! staged artifact at the same path, so the runtime keeps the replaced original under
//! `.delta/versions/<run>/` like any other replacement.

mod docx;
mod pptx;
mod runs;
mod xlsx;

#[cfg(test)]
mod tests;

use std::io::{Cursor, Write};
use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};
use zip::{ZipArchive, ZipWriter};

use super::ooxml;
use super::write::strip_xml_controls;
use super::{kind_of, parse, Kind, MAX_CHARS, MAX_FILE_BYTES};
use crate::capability::{
    CapabilityArtifact, CapabilityGrants, CapabilityRegistration, CapabilityResult,
    NativeCapabilityRunner,
};

/// Most edits one call may make.
const MAX_EDITS: usize = 200;
/// Longest text an edit may look for.
const MAX_FIND_CHARS: usize = 1_000;

/// What an edit changed: the parts it rewrote, a line per edit for the summary, and what it
/// touched, so the result can be checked against exactly that.
pub(super) struct Changes {
    pub parts: Vec<(String, Vec<u8>)>,
    pub applied: Vec<Value>,
    /// Per rewritten part, how many paragraphs an edit touched (`.docx`, `.pptx`).
    pub paragraphs: Vec<(String, usize)>,
    /// The rows an edit wrote to, as (sheet name, row number) (`.xlsx`).
    pub rows: Vec<(String, usize)>,
}

/// Edit the file at `path` (its current `bytes`) as `arguments` ask. The new bytes come back only
/// when the check passed.
pub fn edit(path: &str, bytes: &[u8], arguments: &Value) -> Result<(Vec<u8>, Value), String> {
    let relative = Path::new(path);
    if path.trim().is_empty()
        || relative.has_root()
        || relative
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("path must be relative to the workspace and stay inside it".to_owned());
    }
    let kind = kind_of(relative)?;
    let edits = arguments
        .get("edits")
        .and_then(Value::as_array)
        .filter(|edits| !edits.is_empty())
        .ok_or_else(|| "edits is required and must be a non-empty list".to_owned())?;
    if edits.len() > MAX_EDITS {
        return Err(format!("at most {MAX_EDITS} edits may be made in one call"));
    }
    let changes = match kind {
        Kind::Docx => docx::edit(bytes, edits)?,
        Kind::Pptx => pptx::edit(bytes, edits)?,
        Kind::Xlsx => xlsx::edit(bytes, edits)?,
        Kind::Pdf => {
            return Err(
                "edit_document edits DOCX, XLSX and PPTX files; a PDF cannot be edited".to_owned(),
            )
        }
    };
    let edited = repack(bytes, &changes.parts)?;
    verify(kind, bytes, &edited, &changes)?;
    let blocks = parse(kind, &edited).map(|d| d.blocks.len()).unwrap_or(0);
    let summary = json!({
        "ok": true, "path": path, "kind": kind.as_str(), "bytes": edited.len(),
        "block_count": blocks, "edits": changes.applied,
    });
    Ok((edited, summary))
}

/// One `find` / `replace` edit as the text formats take it.
fn text_edit(edit: &Value, number: usize) -> Result<runs::TextEdit, String> {
    let find = edit
        .get("find")
        .and_then(Value::as_str)
        .filter(|find| !find.is_empty())
        .ok_or_else(|| format!("edit {number}: find is required"))?;
    if find.chars().count() > MAX_FIND_CHARS {
        return Err(format!(
            "edit {number}: find is too long (over {MAX_FIND_CHARS} characters)"
        ));
    }
    if find.contains(['\n', '\r']) {
        return Err(format!(
            "edit {number}: find must sit inside one paragraph: no line breaks"
        ));
    }
    let replace = edit.get("replace").and_then(Value::as_str).ok_or_else(|| {
        format!("edit {number}: replace is required (use an empty text to delete)")
    })?;
    if replace.chars().count() > MAX_CHARS {
        return Err(format!(
            "edit {number}: replace is too long (over {MAX_CHARS} characters)"
        ));
    }
    if replace.contains(['\n', '\r']) {
        return Err(format!(
            "edit {number}: replace must stay one paragraph: no line breaks"
        ));
    }
    let occurrence = match edit.get("occurrence") {
        None | Some(Value::Null) => runs::Occurrence::Only,
        Some(Value::String(all)) if all == "all" => runs::Occurrence::All,
        Some(other) => match other.as_u64() {
            Some(n) if n >= 1 => runs::Occurrence::Nth(n as usize),
            _ => {
                return Err(format!(
                    "edit {number}: occurrence must be a number from 1, or \"all\""
                ))
            }
        },
    };
    Ok(runs::TextEdit {
        number,
        find: find.to_owned(),
        replace: strip_xml_controls(replace),
        occurrence,
    })
}

/// The original package with `replaced` parts swapped in. Every other part is copied as it is
/// stored, so nothing the edit did not touch can change.
fn repack(original: &[u8], replaced: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut archive = ZipArchive::new(Cursor::new(original))
        .map_err(|error| format!("the file is not a valid Office document: {error}"))?;
    let mut out = Vec::new();
    {
        let mut writer = ZipWriter::new(Cursor::new(&mut out));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for index in 0..archive.len() {
            let name = archive
                .by_index_raw(index)
                .map_err(|error| format!("could not read the file: {error}"))?
                .name()
                .to_owned();
            match replaced.iter().find(|(part, _)| *part == name) {
                Some((_, content)) => {
                    writer
                        .start_file(name, options)
                        .and_then(|()| writer.write_all(content).map_err(Into::into))
                        .map_err(|error| format!("could not build the file: {error}"))?;
                }
                None => {
                    let file = archive
                        .by_index_raw(index)
                        .map_err(|error| format!("could not read the file: {error}"))?;
                    writer
                        .raw_copy_file(file)
                        .map_err(|error| format!("could not build the file: {error}"))?;
                }
            }
        }
        writer
            .finish()
            .map_err(|error| format!("could not build the file: {error}"))?;
    }
    Ok(out)
}

/// Read the edited file back and compare it with the original. The edit is refused when it left a
/// different set of parts, a different number of paragraphs or rows, or changed more than it
/// touched.
fn verify(kind: Kind, before: &[u8], after: &[u8], changes: &Changes) -> Result<(), String> {
    let mut old = ooxml::open(before)?;
    let mut new = ooxml::open(after)?;
    let names = |archive: &ooxml::Archive<'_>| {
        let mut names: Vec<String> = archive.file_names().map(str::to_owned).collect();
        names.sort();
        names
    };
    if names(&old) != names(&new) {
        return Err("the edit changed the parts of the file, so nothing was changed".to_owned());
    }
    parse(kind, after).map_err(|error| format!("the edited file cannot be read back: {error}"))?;
    for (part, allowed) in &changes.paragraphs {
        let before_texts = runs::paragraph_texts(&ooxml::required_part(&mut old, part)?, part)?;
        let after_texts = runs::paragraph_texts(&ooxml::required_part(&mut new, part)?, part)?;
        if before_texts.len() != after_texts.len() {
            return Err(format!(
                "the edit changed the number of paragraphs in {part}, so nothing was changed"
            ));
        }
        let differing = before_texts
            .iter()
            .zip(&after_texts)
            .filter(|(left, right)| left != right)
            .count();
        if differing > *allowed {
            return Err(format!(
                "the edit changed more of {part} than it was asked to, so nothing was changed"
            ));
        }
    }
    if kind == Kind::Xlsx {
        xlsx::verify(before, after, &changes.rows)?;
    }
    Ok(())
}

pub fn edit_registration() -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "document.edit".to_owned(),
        tool_name: "edit_document".to_owned(),
        side_effect: crate::skills::SideEffectClass::LocalWrite,
        description: "Change an existing Word, Excel or PowerPoint file in place; use \
            create_document only for a new file. Read it with read_document first. Small, located \
            edits only; formatting, other paragraphs, cells and slides stay as they are, and the \
            file is read back and compared before it is reported as changed. The original is kept \
            as an earlier version. \
            .docx: edits of {find, replace, occurrence?} replace text inside one paragraph. \
            .pptx: edits of {slide (1-based), find, replace, occurrence?}. \
            For both, find must match exactly once unless occurrence is a number (1-based) or \
            \"all\"; a match stays inside one paragraph; replace is one paragraph of text. \
            .xlsx: edits of {sheet, cell, value} or {sheet, range, values: [[...]]}; a value is \
            text, a number, true/false or null (clears the cell), and text starting with = is a \
            formula. Other cells, styles and formulas are untouched."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["path", "edits"],
            "properties": {
                "path": {"type": "string", "description": "The file to edit, relative to the workspace, ending in .docx, .xlsx or .pptx."},
                "edits": {"type": "array", "items": {"type": "object"}, "description": "The changes to make, in order."}
            }
        }),
        metadata: json!({
            "risk_level": "medium", "requires_approval": true,
            "category": "filesystem", "capabilities": ["document.edit"]
        }),
        grants: CapabilityGrants::default(),
        workspace_write: true,
        runner: Arc::new(NativeCapabilityRunner::new(|job, control| {
            let Some(path) = job.arguments.get("path").and_then(Value::as_str) else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "path is required",
                    Some("arguments"),
                );
            };
            let resolved = match crate::capability::resolve_read_path(job, path) {
                Ok(resolved) => resolved,
                Err((code, message)) => {
                    return CapabilityResult::failed(&job.job_id, &message, Some(code))
                }
            };
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            let Some(staging_dir) = job.artifact_staging_dir.as_deref() else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "artifact staging is unavailable",
                    Some("staging"),
                );
            };
            let size = match std::fs::metadata(&resolved) {
                Ok(metadata) => metadata.len(),
                Err(error) => {
                    return CapabilityResult::failed(
                        &job.job_id,
                        &format!("could not read the file: {error}"),
                        Some("read_failed"),
                    )
                }
            };
            if size > MAX_FILE_BYTES {
                return CapabilityResult::failed(
                    &job.job_id,
                    &format!("the file is too large to edit ({size} bytes)"),
                    Some("read_failed"),
                );
            }
            let bytes = match std::fs::read(&resolved) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return CapabilityResult::failed(
                        &job.job_id,
                        &format!("could not read the file: {error}"),
                        Some("read_failed"),
                    )
                }
            };
            let (edited, summary) = match edit(path, &bytes, &job.arguments) {
                Ok(edited) => edited,
                Err(error) => {
                    return CapabilityResult::failed(&job.job_id, &error, Some("edit_failed"))
                }
            };
            let staged = std::path::PathBuf::from(staging_dir).join("candidate");
            if let Err(error) = std::fs::write(&staged, &edited) {
                return CapabilityResult::failed(
                    &job.job_id,
                    &error.to_string(),
                    Some("write_failed"),
                );
            }
            CapabilityResult::completed(&job.job_id, summary).with_artifact(CapabilityArtifact {
                staging_path: staged.to_string_lossy().to_string(),
                relative_path: path.to_owned(),
                kind: Some("file".to_owned()),
                sha256: None,
                size: Some(edited.len() as u64),
                incomplete: false,
            })
        })),
    }
}
