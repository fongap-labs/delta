//! The sequential part of a study: does the rule say stop, and what could the next round be.
//!
//! Everything here is deterministic arithmetic on the study record. No model decides whether a
//! study has stopped, and a recommendation is a suggestion: a round starts only when the user
//! decides it (ADR-0056 point 3).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{Basis, Direction, Factor, Round, StopRule, Study};

/// Whether the rule has been met.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Not met, and rounds remain.
    Continue,
    /// The response met its threshold in enough consecutive rounds.
    StoppedMet,
    /// `max_rounds` rounds were analysed and the rule was never met.
    StoppedExhausted,
}

/// What the rule says about the rounds so far, as it is written into the study record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interim {
    /// Rounds that have a result for the rule's response.
    pub rounds_analysed: u32,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    pub status: Status,
    pub rounds_analysed: u32,
    /// The numbers of the analysed rounds in which the response met its threshold.
    pub rounds_meeting: Vec<u32>,
    pub reason: String,
}

impl Evaluation {
    pub fn interim(&self) -> Interim {
        Interim {
            rounds_analysed: self.rounds_analysed,
            status: self.status,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "status": self.status, "rounds_analysed": self.rounds_analysed,
            "rounds_meeting": self.rounds_meeting, "reason": self.reason,
        })
    }
}

/// The value of a round's result that the rule compares with its threshold, or `None` when the
/// round has no result for the response or lacks the bound the basis needs.
fn compared_value(rule: &StopRule, round: &Round) -> Option<f64> {
    let result = round.results.get(&rule.response)?;
    match (rule.basis, rule.direction) {
        (Basis::Estimate, _) => Some(result.value),
        (Basis::ConservativeBound, Direction::AtLeast) => result.ci_lower,
        (Basis::ConservativeBound, Direction::AtMost) => result.ci_upper,
    }
}

fn meets(rule: &StopRule, value: f64) -> bool {
    match rule.direction {
        Direction::AtLeast => value >= rule.threshold,
        Direction::AtMost => value <= rule.threshold,
    }
}

/// Apply the study's stopping rule to its rounds. Errors when the study has no rule or its
/// rounds can not be read.
pub fn evaluate(study: &Study) -> Result<Evaluation, Vec<String>> {
    let Some(sequential) = &study.sequential else {
        return Err(vec!["this study has no sequential stopping rule".to_owned()]);
    };
    let rule = &sequential.rule;
    let mut rounds = study.typed_rounds()?;
    rounds.sort_by_key(|round| round.number);
    let analysed: Vec<(&Round, Option<f64>)> = rounds
        .iter()
        .filter(|round| round.results.contains_key(&rule.response))
        .map(|round| (round, compared_value(rule, round)))
        .collect();
    let rounds_analysed = analysed.len() as u32;
    let rounds_meeting: Vec<u32> = analysed
        .iter()
        .filter(|(_, value)| value.is_some_and(|value| meets(rule, value)))
        .map(|(round, _)| round.number)
        .collect();

    let needed = rule.consecutive_rounds as usize;
    let tail = &analysed[analysed.len().saturating_sub(needed)..];
    let is_met = needed > 0
        && tail.len() == needed
        && tail
            .iter()
            .all(|(_, value)| value.is_some_and(|value| meets(rule, value)))
        && tail
            .windows(2)
            .all(|pair| pair[1].0.number == pair[0].0.number + 1);
    let (status, reason) = if is_met {
        (
            Status::StoppedMet,
            format!(
                "{} met its threshold in {} consecutive rounds",
                rule.response, rule.consecutive_rounds
            ),
        )
    } else if rounds_analysed >= rule.max_rounds {
        (
            Status::StoppedExhausted,
            format!(
                "{} rounds were analysed and the rule was not met",
                rule.max_rounds
            ),
        )
    } else {
        (
            Status::Continue,
            format!(
                "{} of {} rounds analysed; the rule is not met yet",
                rounds_analysed, rule.max_rounds
            ),
        )
    };
    Ok(Evaluation {
        status,
        rounds_analysed,
        rounds_meeting,
        reason,
    })
}

/// A suggestion for the next round. It is not a decision and starts nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct Recommendation {
    /// The number the next round would have.
    pub round: u32,
    pub factors: Vec<Factor>,
    pub basis: String,
}

impl Recommendation {
    pub fn to_json(&self) -> Value {
        json!({
            "round": self.round,
            "factors": self.factors.iter().map(|factor| json!({"name": factor.name, "low": factor.low, "high": factor.high})).collect::<Vec<_>>(),
            "basis": self.basis,
        })
    }
}

/// Where the next round could look. `None` when the rule says the study has stopped.
///
/// With no analysed round yet it suggests the current ranges. After that it centres every factor
/// on the best round's best run (or on the middle of its range when the round recorded none) and
/// halves the width, then moves the range inside the allowed bounds without narrowing it further.
/// It never adds a factor and never leaves the bounds.
pub fn recommend_next(study: &Study) -> Result<Option<Recommendation>, Vec<String>> {
    let evaluation = evaluate(study)?;
    if evaluation.status != Status::Continue {
        return Ok(None);
    }
    let rule = &study.sequential.as_ref().expect("evaluate checked it").rule;
    let rounds = study.typed_rounds()?;
    let next_number = rounds.iter().map(|round| round.number).max().unwrap_or(0) + 1;
    let best = rounds
        .iter()
        .filter_map(|round| compared_value(rule, round).map(|value| (round, value)))
        .reduce(|best, candidate| {
            let better = match rule.direction {
                Direction::AtLeast => candidate.1 > best.1,
                Direction::AtMost => candidate.1 < best.1,
            };
            if better {
                candidate
            } else {
                best
            }
        });
    let Some((best_round, best_value)) = best else {
        return Ok(Some(Recommendation {
            round: next_number,
            factors: study.factors.clone(),
            basis: "no round has a result yet, so the current ranges are kept".to_owned(),
        }));
    };
    let mut factors = Vec::with_capacity(best_round.factors.len());
    for factor in &best_round.factors {
        let bounds = study
            .allowed_adjustments
            .factors
            .iter()
            .find(|bounds| bounds.name == factor.name);
        let (min, max) =
            bounds.map_or((factor.low, factor.high), |bounds| (bounds.min, bounds.max));
        let width = ((factor.high - factor.low) / 2.0).min(max - min);
        let centre = best_round
            .best_point
            .as_ref()
            .and_then(|point| point.get(&factor.name))
            .copied()
            .filter(|value| value.is_finite())
            .map_or((factor.low + factor.high) / 2.0, |value| {
                value.clamp(factor.low, factor.high)
            });
        let mut low = centre - width / 2.0;
        let mut high = centre + width / 2.0;
        if low < min {
            low = min;
            high = min + width;
        }
        if high > max {
            high = max;
            low = max - width;
        }
        factors.push(Factor {
            name: factor.name.clone(),
            low,
            high,
        });
    }
    Ok(Some(Recommendation {
        round: next_number,
        factors,
        basis: format!(
            "centred on the best run of round {} ({} = {}), each range halved and kept inside its bounds",
            best_round.number, rule.response, best_value
        ),
    }))
}

/// The problems with the sequential parts of a study; empty when there are none or when the study
/// has no stopping rule and no interim record.
pub fn validate(study: &Study) -> Vec<String> {
    let mut problems = Vec::new();
    let Some(sequential) = &study.sequential else {
        if study.interim.is_some() {
            problems.push("interim needs a sequential stopping rule".to_owned());
        }
        return problems;
    };
    let rule = &sequential.rule;
    if !study
        .responses
        .iter()
        .any(|response| response.name == rule.response)
    {
        problems.push(format!(
            "sequential.rule.response {} is not one of the responses",
            rule.response
        ));
    }
    if !rule.threshold.is_finite() {
        problems.push("sequential.rule.threshold must be a finite number".to_owned());
    }
    if rule.consecutive_rounds == 0 {
        problems.push("sequential.rule.consecutive_rounds must be at least 1".to_owned());
    }
    if rule.max_rounds == 0 || rule.max_rounds < rule.consecutive_rounds {
        problems.push(
            "sequential.rule.max_rounds must be at least consecutive_rounds, and at least 1"
                .to_owned(),
        );
    }
    let rounds = match study.typed_rounds() {
        Ok(rounds) => rounds,
        Err(found) => {
            problems.extend(found);
            return problems;
        }
    };
    for (index, round) in rounds.iter().enumerate() {
        let expected = index as u32 + 1;
        if round.number != expected {
            problems.push(format!(
                "round {} is numbered {}; rounds are numbered 1, 2, 3 … in order",
                expected, round.number
            ));
        }
        for factor in &round.factors {
            let inside = study
                .allowed_adjustments
                .factors
                .iter()
                .find(|bounds| bounds.name == factor.name)
                .is_some_and(|bounds| factor.low >= bounds.min && factor.high <= bounds.max);
            if !factor.low.is_finite()
                || !factor.high.is_finite()
                || factor.low >= factor.high
                || !inside
            {
                problems.push(format!(
                    "round {}: the range of {} is not a range inside its bounds",
                    round.number, factor.name
                ));
            }
        }
        for (name, result) in &round.results {
            let at = format!("round {}, {name}", round.number);
            if !study
                .responses
                .iter()
                .any(|response| response.name == *name)
            {
                problems.push(format!("{at}: not one of the responses"));
            }
            if !result.value.is_finite() {
                problems.push(format!("{at}: the value is not a finite number"));
            }
            let (lower, upper) = (result.ci_lower, result.ci_upper);
            if lower.is_some_and(|v| !v.is_finite()) || upper.is_some_and(|v| !v.is_finite()) {
                problems.push(format!("{at}: a confidence bound is not a finite number"));
            }
            if lower.is_some_and(|lower| lower > result.value)
                || upper.is_some_and(|upper| upper < result.value)
            {
                problems.push(format!("{at}: the value is outside its confidence bounds"));
            }
            let source = &result.source;
            let is_hash = source.sha256.len() == 64
                && source
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
            if source.artifact.trim().is_empty() || source.key.trim().is_empty() || !is_hash {
                problems.push(format!(
                    "{at}: the source needs an artifact, a key and a lowercase SHA-256"
                ));
            }
            if lower.is_some() != source.ci_lower_key.is_some()
                || upper.is_some() != source.ci_upper_key.is_some()
            {
                problems.push(format!(
                    "{at}: every confidence bound needs its own key in the source, and no key without a bound"
                ));
            }
        }
    }
    if let Some(recorded) = &study.interim {
        match evaluate(study) {
            Ok(evaluation) if evaluation.interim() == *recorded => {}
            Ok(evaluation) => problems.push(format!(
                "interim says {} rounds analysed and {:?}, but the rounds give {} and {:?}",
                recorded.rounds_analysed,
                recorded.status,
                evaluation.rounds_analysed,
                evaluation.status
            )),
            Err(found) => problems.extend(found),
        }
    }
    problems
}
