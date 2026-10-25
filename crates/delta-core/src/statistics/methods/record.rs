//! `statistics_record`: run one of the inference methods and write its numbers to a result file
//! (ADR-0055 point 1), so a report or a study round can point at them.
//!
//! The file is `{"values": {key: {"value": number}}, "provenance": {…}, …}`. R computes every
//! number again here; none is taken from the model. A key is the path to the number in the
//! method's own result, joined with dots (`p_value`, `groups.1.mean`, `coefficients.2.estimate`),
//! so the same key means the same thing every time.

use std::sync::Arc;

use serde_json::{json, Map, Value};

use super::{
    find_rscript, run, Method, ANOVA, CHISQ, CORRELATION, METHOD_TIMEOUT, REGRESSION, TTEST,
};
use crate::capability::{
    CapabilityArtifact, CapabilityGrants, CapabilityRegistration, CapabilityResult,
    NativeCapabilityRunner,
};
use crate::skills::SideEffectClass;

/// Methods that give numbers worth recording. The design generator gives a table, not results.
fn method_named(name: &str) -> Option<&'static Method> {
    match name {
        "ttest" => Some(&TTEST),
        "anova" => Some(&ANOVA),
        "chisq" => Some(&CHISQ),
        "correlation" => Some(&CORRELATION),
        "regression" => Some(&REGRESSION),
        _ => None,
    }
}

/// Every finite number in `result`, keyed by its dotted path. The provenance and the warnings are
/// not results and are skipped. Arrays are numbered from 1.
pub(super) fn flatten(result: &Value) -> Map<String, Value> {
    fn walk(value: &Value, path: &str, out: &mut Map<String, Value>) {
        match value {
            Value::Number(number) if number.as_f64().is_some_and(f64::is_finite) => {
                out.insert(path.to_owned(), json!({"value": number}));
            }
            Value::Object(object) => {
                for (key, inner) in object {
                    if path.is_empty() && matches!(key.as_str(), "provenance" | "warnings") {
                        continue;
                    }
                    let next = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    walk(inner, &next, out);
                }
            }
            Value::Array(items) => {
                for (index, inner) in items.iter().enumerate() {
                    walk(inner, &format!("{path}.{}", index + 1), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Map::new();
    walk(result, "", &mut out);
    out
}

/// A result file name: relative, inside the workspace, ending in `.json`.
pub(super) fn valid_result_path(path: &str) -> Result<(), String> {
    let path = std::path::Path::new(path);
    let is_relative = !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)));
    let is_json = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
    if !is_relative || !is_json || path.components().next().is_none() {
        return Err(
            "result_path must be a relative path inside the workspace, ending in .json".to_owned(),
        );
    }
    Ok(())
}

pub(super) fn registration() -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "statistics.record".to_owned(),
        tool_name: "statistics_record".to_owned(),
        side_effect: SideEffectClass::LocalWrite,
        description:
            "Run a statistics method (ttest, anova, chisq, correlation or regression) on a \
            CSV file in the workspace, exactly as the matching statistics_* tool does, and write \
            every number of the result to a JSON result file. A report or a study round points at \
            the numbers by file and key (`p_value`, `conf_low`, `groups.1.mean`, \
            `coefficients.2.estimate`), and the file's SHA-256 shows they were not changed. R \
            computes the numbers again; none comes from the model. `options` holds the other \
            arguments of that method."
                .to_owned(),
        parameters: json!({
            "type": "object", "required": ["method", "path", "result_path"],
            "properties": {
                "method": {"type": "string", "enum": ["ttest", "anova", "chisq", "correlation", "regression"]},
                "path": {"type": "string", "description": "CSV file path, relative to the workspace."},
                "options": {"type": "object", "description": "The method's other arguments, such as {\"column\": \"y\", \"kind\": \"one_sample\"} for ttest."},
                "result_path": {"type": "string", "description": "Where to write the result file, relative to the workspace, ending in .json."}
            }
        }),
        metadata: json!({
            "risk_level": "medium", "requires_approval": true,
            "category": "filesystem", "capabilities": ["statistics.record"]
        }),
        grants: CapabilityGrants::default(),
        workspace_write: true,
        runner: Arc::new(NativeCapabilityRunner::new(|job, control| {
            let arguments = &job.arguments;
            let Some(method_name) = arguments.get("method").and_then(Value::as_str) else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "method is required",
                    Some("arguments"),
                );
            };
            let Some(method) = method_named(method_name) else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "method must be one of: ttest, anova, chisq, correlation, regression",
                    Some("arguments"),
                );
            };
            let Some(path) = arguments.get("path").and_then(Value::as_str) else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "path is required",
                    Some("arguments"),
                );
            };
            let result_path = arguments
                .get("result_path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Err(message) = valid_result_path(result_path) {
                return CapabilityResult::failed(&job.job_id, &message, Some("arguments"));
            }
            let input = match crate::capability::resolve_read_path(job, path) {
                Ok(input) => input,
                Err((code, message)) => {
                    return CapabilityResult::failed(&job.job_id, &message, Some(code))
                }
            };
            let Some(staging_dir) = job.artifact_staging_dir.as_deref() else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "artifact staging is unavailable",
                    Some("staging"),
                );
            };
            let mut method_arguments = arguments
                .get("options")
                .cloned()
                .filter(Value::is_object)
                .unwrap_or_else(|| json!({}));
            method_arguments["path"] = json!(path);
            if let Err(message) = (method.prepare)(&method_arguments) {
                return CapabilityResult::failed(&job.job_id, &message, Some("arguments"));
            }
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            let rscript = match find_rscript() {
                Ok(rscript) => rscript,
                Err(message) => {
                    return CapabilityResult::failed(&job.job_id, &message, Some("unavailable"))
                }
            };
            let (output, bytes) = match build(method, &rscript, &input, &method_arguments) {
                Ok(built) => built,
                Err(error) => {
                    return CapabilityResult::failed(&job.job_id, &error, Some("statistics_failed"))
                }
            };
            let staged = std::path::PathBuf::from(staging_dir).join("candidate");
            if let Err(error) = std::fs::write(&staged, &bytes) {
                return CapabilityResult::failed(
                    &job.job_id,
                    &error.to_string(),
                    Some("write_failed"),
                );
            }
            let mut output = output;
            output["result_path"] = json!(result_path);
            CapabilityResult::completed(&job.job_id, output).with_artifact(CapabilityArtifact {
                staging_path: staged.to_string_lossy().to_string(),
                relative_path: result_path.to_owned(),
                kind: Some("file".to_owned()),
                sha256: None,
                size: Some(bytes.len() as u64),
                incomplete: false,
            })
        })),
    }
}

/// Run the method with the R at `rscript` and build the result file. Returns what the model is
/// told (the method's result, plus the keys it can cite) and the bytes of the file.
pub(super) fn build(
    method: &Method,
    rscript: &std::path::Path,
    input: &std::path::Path,
    method_arguments: &Value,
) -> Result<(Value, Vec<u8>), String> {
    let prepared = (method.prepare)(method_arguments)?;
    let result = run(method, rscript, Some(input), &prepared, METHOD_TIMEOUT)?;
    let values = flatten(&result);
    let file = json!({
        "format": "delta.result",
        "version": 1,
        "method": method.name,
        "values": values,
        "warnings": result.get("warnings").cloned().unwrap_or_else(|| json!([])),
        "provenance": result.get("provenance").cloned().unwrap_or(Value::Null),
    });
    let bytes = serde_json::to_vec_pretty(&file).map_err(|error| error.to_string())?;
    let mut output = result;
    output["ok"] = json!(true);
    output["keys"] = json!(values.keys().collect::<Vec<_>>());
    Ok((output, bytes))
}
