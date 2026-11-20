//! Checking that the numbers in a study's rounds are the numbers in the result artifacts they
//! name (ADR-0055), and the `study_evaluate` tool that reports the study's state.
//!
//! A result artifact is a JSON file whose `values` object maps each key to `{"value": number, …}`.
//! A round's result is trusted only when the artifact still has the SHA-256 the round recorded and
//! holds the same number under the same key.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::sequential::{evaluate, recommend_next};
use super::{parse_study, Study};
use crate::capability::{
    CapabilityGrants, CapabilityRegistration, CapabilityResult, NativeCapabilityRunner,
};
use crate::skills::SideEffectClass;

/// The outcome of checking one number's source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCheck {
    pub round: u32,
    pub response: String,
    pub artifact: String,
    pub is_verified: bool,
    pub problem: Option<String>,
}

impl SourceCheck {
    fn to_json(&self) -> Value {
        json!({
            "round": self.round, "response": self.response, "artifact": self.artifact,
            "verified": self.is_verified, "problem": self.problem,
        })
    }
}

fn same_number(left: f64, right: f64) -> bool {
    left == right || (left - right).abs() <= 1e-9 * left.abs().max(right.abs())
}

/// Check every result of every round against its artifact. `read` returns an artifact's bytes by
/// the path the study names it with.
pub fn verify_sources(
    study: &Study,
    read: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> Vec<SourceCheck> {
    let Ok(rounds) = study.typed_rounds() else {
        return Vec::new();
    };
    let mut cache: HashMap<String, Result<(String, Value), String>> = HashMap::new();
    let mut checks = Vec::new();
    for round in &rounds {
        for (response, result) in &round.results {
            let source = &result.source;
            let loaded = cache.entry(source.artifact.clone()).or_insert_with(|| {
                let bytes = read(&source.artifact)?;
                let hash = format!("{:x}", Sha256::digest(&bytes));
                let value: Value = serde_json::from_slice(&bytes)
                    .map_err(|_| "the artifact is not a JSON result file".to_owned())?;
                Ok((hash, value))
            });
            let outcome = match loaded {
                Err(error) => Err(error.clone()),
                Ok((hash, value)) => {
                    if *hash != source.sha256 {
                        Err("the artifact has changed since the round recorded it".to_owned())
                    } else {
                        let lookup = |key: &str, expected: f64| -> Result<(), String> {
                            let found = value
                                .get("values")
                                .and_then(|values| values.get(key))
                                .and_then(|entry| entry.get("value"))
                                .and_then(Value::as_f64)
                                .ok_or_else(|| format!("the artifact has no value under {key}"))?;
                            if same_number(found, expected) {
                                Ok(())
                            } else {
                                Err(format!("{key} is {found} in the artifact, not {expected}"))
                            }
                        };
                        lookup(&source.key, result.value)
                            .and_then(|()| match (&source.ci_lower_key, result.ci_lower) {
                                (Some(key), Some(expected)) => lookup(key, expected),
                                _ => Ok(()),
                            })
                            .and_then(|()| match (&source.ci_upper_key, result.ci_upper) {
                                (Some(key), Some(expected)) => lookup(key, expected),
                                _ => Ok(()),
                            })
                    }
                }
            };
            checks.push(SourceCheck {
                round: round.number,
                response: response.clone(),
                artifact: source.artifact.clone(),
                is_verified: outcome.is_ok(),
                problem: outcome.err(),
            });
        }
    }
    checks
}

/// The `study_evaluate` tool: read a study record and report whether it is valid, whether its
/// numbers trace to their artifacts, what the stopping rule says, and where the next round could
/// look. It reads only; it changes nothing and starts nothing.
pub fn evaluate_registration() -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "study.evaluate".to_owned(),
        tool_name: "study_evaluate".to_owned(),
        side_effect: SideEffectClass::Read,
        description: "Evaluate a multi-round study record (study.json): check it against its schema \
            and locked fields' rules, check that every number in its rounds is the number in the \
            result artifact it names (SHA-256 and key), apply the study's sequential stopping rule, \
            and suggest where the next round could look. The suggestion is not a decision: a round \
            starts only when the user decides it. The answer comes from arithmetic on the record, \
            never from judgement."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["path"],
            "properties": {
                "path": {"type": "string", "description": "The study record, usually study.json, relative to the workspace."}
            }
        }),
        metadata: json!({
            "risk_level": "low", "requires_approval": false,
            "category": "read", "capabilities": ["study.evaluate"]
        }),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(|job, control| {
            let Some(path) = job.arguments.get("path").and_then(Value::as_str) else {
                return CapabilityResult::failed(&job.job_id, "path is required", Some("arguments"));
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
            let text = match std::fs::read_to_string(&resolved) {
                Ok(text) => text,
                Err(error) => {
                    return CapabilityResult::failed(
                        &job.job_id,
                        &format!("could not read the study: {error}"),
                        Some("read_failed"),
                    )
                }
            };
            let study = match parse_study(&text) {
                Ok(study) => study,
                Err(problems) => {
                    return CapabilityResult::completed(
                        &job.job_id,
                        json!({"ok": true, "valid": false, "problems": problems}),
                    )
                }
            };
            let read = |artifact: &str| -> Result<Vec<u8>, String> {
                let resolved = crate::capability::resolve_read_path(job, artifact)
                    .map_err(|(_, message)| message)?;
                std::fs::read(resolved).map_err(|error| format!("could not read the artifact: {error}"))
            };
            let checks = verify_sources(&study, &read);
            let is_trusted = checks.iter().all(|check| check.is_verified);
            let (evaluation, recommendation) = match (evaluate(&study), recommend_next(&study)) {
                (Ok(evaluation), Ok(recommendation)) => (Some(evaluation), recommendation),
                _ => (None, None),
            };
            CapabilityResult::completed(
                &job.job_id,
                json!({
                    "ok": true,
                    "valid": true,
                    "problems": [],
                    "sources": checks.iter().map(SourceCheck::to_json).collect::<Vec<_>>(),
                    "numbers_trusted": is_trusted,
                    "evaluation": evaluation.as_ref().map(|evaluation| evaluation.to_json()),
                    "interim": evaluation.as_ref().map(|evaluation| evaluation.interim()),
                    "recommendation": recommendation.as_ref().map(|recommendation| recommendation.to_json()),
                    "note": if is_trusted {
                        "Every number in the rounds matches its result artifact."
                    } else {
                        "Some numbers do not match their result artifacts: do not rely on the evaluation until they do."
                    },
                }),
            )
        })),
    }
}
