//! The study record of a multi-round study (ADR-0056).
//!
//! A study is a `study.json` artifact. This module reads it, checks it, and finds the changes to
//! the locked fields between two registrations. Locked fields are the objective, the responses,
//! the outer adjustment bounds, the stopping rule and the analysis plan. Factor ranges inside
//! the bounds, rounds, decisions and recommendations are not locked.
//!
//! A study may also carry a machine-checkable stopping rule (`sequential`), which is locked with
//! the rest. From it, the rounds' results and the bounds, [`sequential`] decides, without any
//! model in the loop, whether the study continues or stops, and what range the next round could
//! use. Every number in a round points at the result artifact it came from (ADR-0055), and
//! [`verify`] checks that it does.

pub mod sequential;
pub mod verify;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// The only study schema version this build reads.
pub const STUDY_SCHEMA_VERSION: u32 = 1;

/// The fields that cannot change once a study has a registration, except through `study.amend`.
pub const LOCKED_FIELDS: &[&str] = &[
    "objective",
    "responses",
    "allowed_adjustments",
    "stopping_rule",
    "analysis_plan",
    "sequential",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Study {
    pub schema_version: u32,
    pub objective: String,
    pub responses: Vec<Response>,
    pub factors: Vec<Factor>,
    pub allowed_adjustments: Adjustments,
    pub stopping_rule: String,
    pub analysis_plan: AnalysisPlan,
    /// The stopping rule in a form a program can evaluate. Locked with `stopping_rule`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequential: Option<Sequential>,
    /// What the rule says about the rounds so far. Derived: a record that disagrees with the
    /// evaluation of its own rounds is refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interim: Option<sequential::Interim>,
    #[serde(default)]
    pub rounds: Vec<Value>,
    #[serde(default)]
    pub decisions: Vec<Value>,
    #[serde(default)]
    pub recommendations: Vec<Value>,
    #[serde(default)]
    pub extensions: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub name: String,
    /// What counts as meeting the response, in words the analysis plan can be checked against.
    pub criterion: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Factor {
    pub name: String,
    pub low: f64,
    pub high: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Adjustments {
    /// The outer bounds every factor range must stay within.
    pub factors: Vec<FactorBounds>,
    /// Factors that may be introduced later, by name.
    #[serde(default)]
    pub may_add: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactorBounds {
    pub name: String,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisPlan {
    pub model: String,
    pub significance_level: f64,
    pub confidence_level: f64,
}

/// Which side of the threshold counts as meeting the response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    AtLeast,
    AtMost,
}

/// What of a round's result is compared with the threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// The estimate itself.
    #[default]
    Estimate,
    /// The confidence bound on the safe side: the lower bound for `at_least`, the upper for `at_most`.
    ConservativeBound,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sequential {
    pub rule: StopRule,
}

/// Stop when `response` meets `threshold` in `consecutive_rounds` rounds in a row, or give up
/// after `max_rounds` analysed rounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopRule {
    pub response: String,
    pub direction: Direction,
    pub threshold: f64,
    #[serde(default)]
    pub basis: Basis,
    pub consecutive_rounds: u32,
    pub max_rounds: u32,
}

/// Where a number in a round came from: a result artifact (ADR-0055), its SHA-256 and the keys of
/// its `values`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub artifact: String,
    pub sha256: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci_lower_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci_upper_key: Option<String>,
}

/// One response's result in one round.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseResult {
    pub value: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci_lower: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci_upper: Option<f64>,
    pub source: Source,
}

/// A round, as the sequential rule reads it. A round in `study.rounds` may carry more than this;
/// the rest is the study's own business.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Round {
    pub number: u32,
    /// The factor ranges the round was run over.
    pub factors: Vec<Factor>,
    /// Results by response name; empty until the round has been analysed.
    #[serde(default)]
    pub results: BTreeMap<String, ResponseResult>,
    /// The factor settings of the best run of the round, when the round recorded one.
    #[serde(default)]
    pub best_point: Option<BTreeMap<String, f64>>,
}

impl Study {
    /// The rounds as the sequential rule reads them, with every problem found listed.
    pub fn typed_rounds(&self) -> Result<Vec<Round>, Vec<String>> {
        let mut problems = Vec::new();
        let mut rounds = Vec::with_capacity(self.rounds.len());
        for (index, value) in self.rounds.iter().enumerate() {
            match serde_json::from_value::<Round>(value.clone()) {
                Ok(round) => rounds.push(round),
                Err(error) => problems.push(format!("round {}: {error}", index + 1)),
            }
        }
        if problems.is_empty() {
            Ok(rounds)
        } else {
            Err(problems)
        }
    }
}

/// Read a study from its JSON text and check it. Every problem found is reported, not only the
/// first one.
pub fn parse_study(text: &str) -> Result<Study, Vec<String>> {
    let study: Study = serde_json::from_str(text).map_err(|error| vec![error.to_string()])?;
    let problems = validate_study(&study);
    if problems.is_empty() {
        Ok(study)
    } else {
        Err(problems)
    }
}

/// The problems with a study's contents: an empty list means it is valid.
pub fn validate_study(study: &Study) -> Vec<String> {
    let mut problems = Vec::new();
    if study.schema_version != STUDY_SCHEMA_VERSION {
        problems.push(format!(
            "schema_version must be {STUDY_SCHEMA_VERSION}, not {}",
            study.schema_version
        ));
    }
    if study.objective.trim().is_empty() {
        problems.push("objective must not be empty".to_owned());
    }
    if study.responses.is_empty() {
        problems.push("at least one response is required".to_owned());
    }
    for response in &study.responses {
        if response.name.trim().is_empty() || response.criterion.trim().is_empty() {
            problems.push("each response needs a name and a criterion".to_owned());
        }
    }
    if study.stopping_rule.trim().is_empty() {
        problems.push("stopping_rule must not be empty".to_owned());
    }
    let plan = &study.analysis_plan;
    if plan.model.trim().is_empty() {
        problems.push("analysis_plan.model must not be empty".to_owned());
    }
    if !(plan.significance_level > 0.0 && plan.significance_level < 1.0) {
        problems.push("analysis_plan.significance_level must be between 0 and 1".to_owned());
    }
    if !(plan.confidence_level > 0.0 && plan.confidence_level < 1.0) {
        problems.push("analysis_plan.confidence_level must be between 0 and 1".to_owned());
    }
    for bounds in &study.allowed_adjustments.factors {
        if !(bounds.min.is_finite() && bounds.max.is_finite() && bounds.min <= bounds.max) {
            problems.push(format!("the bounds of {} are not a range", bounds.name));
        }
    }
    for factor in &study.factors {
        if !(factor.low.is_finite() && factor.high.is_finite() && factor.low < factor.high) {
            problems.push(format!("the range of {} is not a range", factor.name));
            continue;
        }
        let bounds = study
            .allowed_adjustments
            .factors
            .iter()
            .find(|bounds| bounds.name == factor.name);
        match bounds {
            None => problems.push(format!(
                "{} has no bounds in allowed_adjustments",
                factor.name
            )),
            Some(bounds) => {
                if factor.low < bounds.min || factor.high > bounds.max {
                    problems.push(format!(
                        "the range of {} leaves its bounds [{}, {}]",
                        factor.name, bounds.min, bounds.max
                    ));
                }
            }
        }
    }
    problems.extend(sequential::validate(study));
    problems
}

/// The locked fields that differ between the previous registration and the next one. An empty
/// list means the next registration changes nothing locked.
pub fn locked_changes(previous: &Study, next: &Study) -> Vec<&'static str> {
    let before = locked_values(previous);
    let after = locked_values(next);
    LOCKED_FIELDS
        .iter()
        .zip(before.iter().zip(after.iter()))
        .filter(|(_, (old, new))| old != new)
        .map(|(field, _)| *field)
        .collect()
}

/// The locked fields as JSON values, in `LOCKED_FIELDS` order.
fn locked_values(study: &Study) -> Vec<Value> {
    vec![
        Value::String(study.objective.clone()),
        serde_json::to_value(&study.responses).unwrap_or(Value::Null),
        serde_json::to_value(&study.allowed_adjustments).unwrap_or(Value::Null),
        Value::String(study.stopping_rule.clone()),
        serde_json::to_value(&study.analysis_plan).unwrap_or(Value::Null),
        serde_json::to_value(&study.sequential).unwrap_or(Value::Null),
    ]
}

#[cfg(test)]
mod sequential_tests;

/// Test support: a copy of `study` whose every source records the SHA-256 of `bytes`.
#[cfg(test)]
fn tests_support_hash(study: &Study, bytes: &[u8]) -> Study {
    use sha2::{Digest, Sha256};
    let hash = format!("{:x}", Sha256::digest(bytes));
    let mut copy = study.clone();
    for round in &mut copy.rounds {
        if let Some(results) = round.get_mut("results").and_then(Value::as_object_mut) {
            for result in results.values_mut() {
                result["source"]["sha256"] = Value::String(hash.clone());
            }
        }
    }
    copy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn study_json() -> String {
        serde_json::json!({
            "schema_version": 1,
            "objective": "Maximise dissolution at 30 minutes",
            "responses": [{"name": "dissolution_30", "criterion": ">= 85 %"}],
            "factors": [
                {"name": "binder", "low": 2.0, "high": 4.0},
                {"name": "compression", "low": 10.0, "high": 14.0}
            ],
            "allowed_adjustments": {
                "factors": [
                    {"name": "binder", "min": 1.0, "max": 6.0},
                    {"name": "compression", "min": 8.0, "max": 16.0}
                ],
                "may_add": ["lubricant"]
            },
            "stopping_rule": "Stop when the response meets its criterion in two rounds",
            "analysis_plan": {"model": "linear", "significance_level": 0.05, "confidence_level": 0.95},
            "rounds": [],
            "decisions": [],
            "recommendations": [],
            "extensions": {}
        })
        .to_string()
    }

    fn valid() -> Study {
        parse_study(&study_json()).unwrap()
    }

    #[test]
    fn a_complete_study_parses_and_validates() {
        assert_eq!(valid().factors.len(), 2);
    }

    #[test]
    fn a_range_outside_its_bounds_is_refused_and_every_problem_is_listed() {
        let mut study: serde_json::Value = serde_json::from_str(&study_json()).unwrap();
        study["factors"][0]["high"] = serde_json::json!(9.0);
        study["objective"] = serde_json::json!("  ");
        let problems = parse_study(&study.to_string()).unwrap_err();
        assert!(
            problems.iter().any(|p| p.contains("leaves its bounds")),
            "{problems:?}"
        );
        assert!(
            problems.iter().any(|p| p.contains("objective")),
            "{problems:?}"
        );
    }

    #[test]
    fn a_factor_without_bounds_is_refused() {
        let mut study: serde_json::Value = serde_json::from_str(&study_json()).unwrap();
        study["factors"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"name": "moisture", "low": 1.0, "high": 2.0}));
        let problems = parse_study(&study.to_string()).unwrap_err();
        assert!(
            problems.iter().any(|p| p.contains("no bounds")),
            "{problems:?}"
        );
    }

    #[test]
    fn a_significance_level_outside_zero_and_one_is_refused() {
        let mut study: serde_json::Value = serde_json::from_str(&study_json()).unwrap();
        study["analysis_plan"]["significance_level"] = serde_json::json!(1.5);
        let problems = parse_study(&study.to_string()).unwrap_err();
        assert!(
            problems.iter().any(|p| p.contains("significance_level")),
            "{problems:?}"
        );
    }

    #[test]
    fn an_unknown_schema_version_is_refused() {
        let mut study: serde_json::Value = serde_json::from_str(&study_json()).unwrap();
        study["schema_version"] = serde_json::json!(2);
        assert!(parse_study(&study.to_string()).is_err());
    }

    #[test]
    fn a_changed_objective_or_stopping_rule_is_a_locked_change() {
        let previous = valid();
        let mut next = valid();
        next.objective = "Maximise hardness".to_owned();
        next.stopping_rule = "Stop after three rounds".to_owned();
        assert_eq!(
            locked_changes(&previous, &next),
            vec!["objective", "stopping_rule"]
        );
    }

    #[test]
    fn narrowing_a_factor_inside_its_bounds_is_not_a_locked_change() {
        let previous = valid();
        let mut next = valid();
        next.factors[0].low = 2.5;
        next.factors[0].high = 3.5;
        assert!(locked_changes(&previous, &next).is_empty());
    }

    #[test]
    fn a_recommendation_or_a_decision_is_not_a_locked_change() {
        let previous = valid();
        let mut next = valid();
        next.recommendations
            .push(serde_json::json!({"text": "run binder 3.0"}));
        next.decisions
            .push(serde_json::json!({"who": "user", "what": "start round 2"}));
        assert!(locked_changes(&previous, &next).is_empty());
    }

    #[test]
    fn widening_the_bounds_is_a_locked_change() {
        let previous = valid();
        let mut next = valid();
        next.allowed_adjustments.factors[0].max = 10.0;
        assert_eq!(
            locked_changes(&previous, &next),
            vec!["allowed_adjustments"]
        );
    }
}
