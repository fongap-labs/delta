//! The model's two tools for bound reports: render one from a template, and check one.

use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};

use super::report::{check_report, render_template, MAX_BYTES};
use crate::capability::{
    CapabilityArtifact, CapabilityGrants, CapabilityJob, CapabilityRegistration, CapabilityResult,
    NativeCapabilityRunner,
};
use crate::skills::SideEffectClass;

pub fn registrations() -> Vec<CapabilityRegistration> {
    vec![render_registration(), check_registration()]
}

/// Read a workspace file the job may read, at most `MAX_BYTES` of it.
fn read_bounded(job: &CapabilityJob, path: &str) -> Result<Vec<u8>, String> {
    let resolved =
        crate::capability::resolve_read_path(job, path).map_err(|(_, message)| message)?;
    let size = std::fs::metadata(&resolved)
        .map_err(|error| format!("could not read {path}: {error}"))?
        .len();
    if size > MAX_BYTES {
        return Err(format!(
            "{path} is larger than {} MB",
            MAX_BYTES / (1024 * 1024)
        ));
    }
    std::fs::read(&resolved).map_err(|error| format!("could not read {path}: {error}"))
}

/// A relative path inside the workspace with the given extension.
pub(crate) fn valid_output_path(path: &str, extension: &str) -> bool {
    let path = Path::new(path);
    !path.is_absolute()
        && path.components().next().is_some()
        && path
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
        && path
            .extension()
            .and_then(|found| found.to_str())
            .is_some_and(|found| found.eq_ignore_ascii_case(extension))
}

fn render_registration() -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "report.render".to_owned(),
        tool_name: "report_render".to_owned(),
        side_effect: SideEffectClass::LocalWrite,
        description: "Render a report from a Markdown template whose computed numbers are written \
            as bindings, {{result:<result file>#<key>|<format>}} (for example \
            {{result:out/t.json#p_value|%.4f}}). Each binding is replaced by the number in that \
            result file, rounded half up to the format (%.Nf, or integer). The report is written \
            to `report_path`, and next to it a manifest (`<report_path>.bindings.json`) records, \
            for every number, its result file, that file's SHA-256, the key, the raw value, the \
            rendered text and its place. Write numbers in a report only as bindings: use an \
            analysis engine to make the result files (ADR-0079). Literature claims are bound the \
            same way: {{cite:<reference file>#<id>}} becomes a numbered marker and \
            {{references:<reference file>}} the list of what was cited, both read from the file \
            scholar_cite writes with `output_path`; a reference that no published record backs \
            stops the render. Markdown only."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["template_path", "report_path"],
            "properties": {
                "template_path": {"type": "string", "description": "The Markdown template, relative to the workspace."},
                "report_path": {"type": "string", "description": "Where to write the report, relative to the workspace, ending in .md."}
            }
        }),
        metadata: json!({
            "risk_level": "medium", "requires_approval": true,
            "category": "filesystem", "capabilities": ["report.render"]
        }),
        grants: CapabilityGrants::default(),
        workspace_write: true,
        runner: Arc::new(NativeCapabilityRunner::new(|job, control| {
            let argument = |name: &str| job.arguments.get(name).and_then(Value::as_str);
            let (Some(template_path), Some(report_path)) =
                (argument("template_path"), argument("report_path"))
            else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "template_path and report_path are required",
                    Some("arguments"),
                );
            };
            if !valid_output_path(report_path, "md") {
                return CapabilityResult::failed(
                    &job.job_id,
                    "report_path must be a relative path inside the workspace, ending in .md",
                    Some("arguments"),
                );
            }
            let Some(staging_dir) = job.artifact_staging_dir.as_deref() else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "artifact staging is unavailable",
                    Some("staging"),
                );
            };
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            let template = match read_bounded(job, template_path).and_then(|bytes| {
                String::from_utf8(bytes).map_err(|_| format!("{template_path} is not UTF-8 text"))
            }) {
                Ok(template) => template,
                Err(message) => {
                    return CapabilityResult::failed(&job.job_id, &message, Some("read_failed"))
                }
            };
            let rendered = match render_template(&template, &|path| read_bounded(job, path)) {
                Ok(rendered) => rendered,
                Err(problems) => {
                    return CapabilityResult::failed(
                        &job.job_id,
                        &format!("the report was not written: {}", problems.join("; ")),
                        Some("render_failed"),
                    )
                }
            };
            let manifest_path = format!("{report_path}.bindings.json");
            let manifest = rendered.manifest(report_path, template_path);
            let manifest_bytes = match serde_json::to_vec_pretty(&manifest) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return CapabilityResult::failed(
                        &job.job_id,
                        &error.to_string(),
                        Some("write_failed"),
                    )
                }
            };
            let staging = Path::new(staging_dir);
            let report_staged = staging.join("candidate");
            let manifest_staged = staging.join("candidate_manifest");
            let written = std::fs::write(&report_staged, rendered.text.as_bytes())
                .and_then(|()| std::fs::write(&manifest_staged, &manifest_bytes));
            if let Err(error) = written {
                return CapabilityResult::failed(
                    &job.job_id,
                    &error.to_string(),
                    Some("write_failed"),
                );
            }
            let artifact = |staged: &Path, relative: &str, size: usize| CapabilityArtifact {
                staging_path: staged.to_string_lossy().to_string(),
                relative_path: relative.to_owned(),
                kind: Some("file".to_owned()),
                sha256: None,
                size: Some(size as u64),
                incomplete: false,
            };
            CapabilityResult::completed(
                &job.job_id,
                json!({
                    "ok": true, "report_path": report_path, "manifest_path": manifest_path,
                    "bindings": rendered.bindings.len(),
                    "rendered": rendered.bindings.iter()
                        .map(|binding| json!({"key": binding.key, "artifact": binding.artifact, "text": binding.rendered}))
                        .collect::<Vec<_>>(),
                }),
            )
            .with_artifact(artifact(&report_staged, report_path, rendered.text.len()))
            .with_artifact(artifact(&manifest_staged, &manifest_path, manifest_bytes.len()))
        })),
    }
}

fn check_registration() -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "report.check".to_owned(),
        tool_name: "report_check".to_owned(),
        side_effect: SideEffectClass::Read,
        description: "Check a rendered report against its binding manifest: every bound number \
            must still be the number in its result file (same SHA-256, same key), must be the raw \
            value rounded to the declared format, and must sit where the manifest says. A formal \
            report (the default) may also contain no other number, except in headings and numbered \
            lists and where one of the `allow` regular expressions covers it (for example a date \
            or 25 °C / 60 % RH). In a formal report every bound result must also be validated or \
            attested (ADR-0079); an exploratory result fails, and a version 1 result without an \
            assurance level is accepted as legacy. Citations are checked too: the reference file \
            must be the one the report was rendered from, and every cited reference must still be \
            backed by a published record. Returns whether it passed and every problem found."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["report_path"],
            "properties": {
                "report_path": {"type": "string", "description": "The rendered report, relative to the workspace."},
                "manifest_path": {"type": "string", "description": "Its manifest. Default: <report_path>.bindings.json."},
                "formal": {"type": "boolean", "description": "Refuse numbers outside the bindings. Default true."},
                "allow": {"type": "array", "items": {"type": "string"}, "description": "Regular expressions for numbers that may appear unbound, such as \\d{4}-\\d{2}-\\d{2} for dates."}
            }
        }),
        metadata: json!({
            "risk_level": "low", "requires_approval": false,
            "category": "read", "capabilities": ["report.check"]
        }),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(|job, control| {
            let Some(report_path) = job.arguments.get("report_path").and_then(Value::as_str) else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "report_path is required",
                    Some("arguments"),
                );
            };
            let manifest_path = job
                .arguments
                .get("manifest_path")
                .and_then(Value::as_str)
                .map_or_else(|| format!("{report_path}.bindings.json"), str::to_owned);
            let is_formal = job
                .arguments
                .get("formal")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let allow: Vec<String> = job
                .arguments
                .get("allow")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            let report = match read_bounded(job, report_path).and_then(|bytes| {
                String::from_utf8(bytes).map_err(|_| format!("{report_path} is not UTF-8 text"))
            }) {
                Ok(report) => report,
                Err(message) => {
                    return CapabilityResult::failed(&job.job_id, &message, Some("read_failed"))
                }
            };
            let manifest: Value = match read_bounded(job, &manifest_path).and_then(|bytes| {
                serde_json::from_slice(&bytes)
                    .map_err(|_| format!("{manifest_path} is not a JSON manifest"))
            }) {
                Ok(manifest) => manifest,
                Err(message) => {
                    return CapabilityResult::failed(&job.job_id, &message, Some("read_failed"))
                }
            };
            let check = check_report(
                &report,
                &manifest,
                &|path| read_bounded(job, path),
                is_formal,
                &allow,
            );
            let mut out = check.to_json();
            out["ok"] = json!(true);
            out["report_path"] = json!(report_path);
            CapabilityResult::completed(&job.job_id, out)
        })),
    }
}
