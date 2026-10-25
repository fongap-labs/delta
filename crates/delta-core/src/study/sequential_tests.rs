use super::sequential::{evaluate, recommend_next, Interim, Status};
use super::verify::verify_sources;
use super::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::cell::RefCell;

const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn rule(extra: Value) -> Value {
    let mut rule = json!({
        "response": "dissolution_30", "direction": "at_least", "threshold": 85.0,
        "basis": "estimate", "consecutive_rounds": 2, "max_rounds": 4
    });
    for (key, value) in extra.as_object().cloned().unwrap_or_default() {
        rule[key] = value;
    }
    rule
}

/// A round with one response result. `ci` is (lower, upper).
fn round(
    number: u32,
    low: f64,
    high: f64,
    value: f64,
    ci: Option<(f64, f64)>,
    best: Option<(f64, f64)>,
) -> Value {
    let mut result = json!({
        "value": value,
        "source": {"artifact": format!("out/round{number}.json"), "sha256": HASH, "key": "dissolution"}
    });
    if let Some((lower, upper)) = ci {
        result["ci_lower"] = json!(lower);
        result["ci_upper"] = json!(upper);
        result["source"]["ci_lower_key"] = json!("dissolution_lo");
        result["source"]["ci_upper_key"] = json!("dissolution_hi");
    }
    let mut round = json!({
        "number": number,
        "factors": [{"name": "binder", "low": low, "high": high}, {"name": "compression", "low": 10.0, "high": 14.0}],
        "results": {"dissolution_30": result},
    });
    if let Some((binder, compression)) = best {
        round["best_point"] = json!({"binder": binder, "compression": compression});
    }
    round
}

fn unanalysed(number: u32) -> Value {
    json!({"number": number, "factors": [{"name": "binder", "low": 2.0, "high": 4.0}]})
}

fn study(rule_extra: Value, rounds: Vec<Value>) -> Study {
    let value = json!({
        "schema_version": 1,
        "objective": "Maximise dissolution at 30 minutes",
        "responses": [{"name": "dissolution_30", "criterion": ">= 85 %"}],
        "factors": [{"name": "binder", "low": 2.0, "high": 4.0}, {"name": "compression", "low": 10.0, "high": 14.0}],
        "allowed_adjustments": {"factors": [
            {"name": "binder", "min": 1.0, "max": 6.0},
            {"name": "compression", "min": 8.0, "max": 16.0}
        ]},
        "stopping_rule": "Stop when dissolution_30 is at least 85 % in two consecutive rounds, or after four rounds",
        "analysis_plan": {"model": "linear", "significance_level": 0.05, "confidence_level": 0.95},
        "sequential": {"rule": rule(rule_extra)},
        "rounds": rounds,
    });
    // Not parse_study: several tests need a study that is not valid.
    serde_json::from_value(value).unwrap()
}

fn problems_of(study: &Study) -> Vec<String> {
    validate_study(study)
}

#[test]
fn a_study_with_a_rule_and_traced_rounds_is_valid() {
    let study = study(
        json!({}),
        vec![
            round(1, 2.0, 4.0, 80.0, None, None),
            round(2, 2.5, 3.5, 86.0, None, None),
        ],
    );
    assert!(problems_of(&study).is_empty(), "{:?}", problems_of(&study));
}

#[test]
fn a_study_without_a_rule_keeps_its_old_freedom() {
    // Rounds are the study's own business until it declares a rule.
    let mut value: Value =
        serde_json::from_str(&serde_json::to_string(&study(json!({}), vec![])).unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("sequential");
    value["rounds"] = json!([{"anything": "goes"}, "even this"]);
    assert!(parse_study(&value.to_string()).is_ok());
}

#[test]
fn badly_formed_rules_and_rounds_are_each_reported() {
    let cases: Vec<(Study, &str)> = vec![
        (
            study(json!({"response": "nope"}), vec![]),
            "not one of the responses",
        ),
        (
            study(json!({"consecutive_rounds": 0}), vec![]),
            "consecutive_rounds",
        ),
        (study(json!({"max_rounds": 1}), vec![]), "max_rounds"),
        (
            study(json!({}), vec![round(2, 2.0, 4.0, 80.0, None, None)]),
            "numbered 2",
        ),
        (
            study(json!({}), vec![round(1, 2.0, 9.0, 80.0, None, None)]),
            "inside its bounds",
        ),
        (
            study(json!({}), vec![round(1, 4.0, 2.0, 80.0, None, None)]),
            "inside its bounds",
        ),
        (
            study(
                json!({}),
                vec![round(1, 2.0, 4.0, 80.0, Some((81.0, 90.0)), None)],
            ),
            "outside its confidence bounds",
        ),
    ];
    for (study, needle) in cases {
        let problems = problems_of(&study);
        assert!(
            problems.iter().any(|p| p.contains(needle)),
            "{needle}: {problems:?}"
        );
    }
}

#[test]
fn a_number_without_a_traceable_source_is_refused() {
    let mut bad_hash = round(1, 2.0, 4.0, 80.0, None, None);
    bad_hash["results"]["dissolution_30"]["source"]["sha256"] = json!("ABC");
    assert!(problems_of(&study(json!({}), vec![bad_hash]))
        .iter()
        .any(|p| p.contains("SHA-256")));
    let mut no_key = round(1, 2.0, 4.0, 80.0, None, None);
    no_key["results"]["dissolution_30"]["source"]["key"] = json!(" ");
    assert!(problems_of(&study(json!({}), vec![no_key]))
        .iter()
        .any(|p| p.contains("SHA-256")));
    // A confidence bound with no key of its own, and a key with no bound.
    let mut orphan = round(1, 2.0, 4.0, 80.0, Some((78.0, 82.0)), None);
    orphan["results"]["dissolution_30"]["source"]
        .as_object_mut()
        .unwrap()
        .remove("ci_lower_key");
    assert!(problems_of(&study(json!({}), vec![orphan]))
        .iter()
        .any(|p| p.contains("its own key")));
    let mut unknown = round(1, 2.0, 4.0, 80.0, None, None);
    unknown["results"]["hardness"] = unknown["results"]["dissolution_30"].clone();
    assert!(problems_of(&study(json!({}), vec![unknown]))
        .iter()
        .any(|p| p.contains("hardness")));
}

#[test]
fn one_good_round_is_not_enough_and_two_in_a_row_stop_the_study() {
    let one = study(json!({}), vec![round(1, 2.0, 4.0, 90.0, None, None)]);
    let evaluation = evaluate(&one).unwrap();
    assert_eq!(evaluation.status, Status::Continue);
    assert_eq!(evaluation.rounds_meeting, vec![1]);

    let two = study(
        json!({}),
        vec![
            round(1, 2.0, 4.0, 86.0, None, None),
            round(2, 2.0, 4.0, 85.0, None, None),
        ],
    );
    let evaluation = evaluate(&two).unwrap();
    assert_eq!(
        evaluation.status,
        Status::StoppedMet,
        "the threshold itself counts as met"
    );
    assert_eq!(evaluation.rounds_analysed, 2);
}

#[test]
fn a_miss_in_between_or_a_round_without_a_result_breaks_the_run() {
    let broken = study(
        json!({}),
        vec![
            round(1, 2.0, 4.0, 90.0, None, None),
            round(2, 2.0, 4.0, 70.0, None, None),
            round(3, 2.0, 4.0, 90.0, None, None),
        ],
    );
    assert_eq!(evaluate(&broken).unwrap().status, Status::Continue);
    let gap = study(
        json!({}),
        vec![
            round(1, 2.0, 4.0, 90.0, None, None),
            unanalysed(2),
            round(3, 2.0, 4.0, 90.0, None, None),
        ],
    );
    let evaluation = evaluate(&gap).unwrap();
    assert_eq!(
        evaluation.status,
        Status::Continue,
        "rounds 1 and 3 are not consecutive"
    );
    assert_eq!(evaluation.rounds_analysed, 2);
}

#[test]
fn giving_up_after_the_last_round_and_meeting_the_rule_on_it_are_told_apart() {
    let misses: Vec<Value> = (1..=4)
        .map(|n| round(n, 2.0, 4.0, 70.0, None, None))
        .collect();
    assert_eq!(
        evaluate(&study(json!({}), misses)).unwrap().status,
        Status::StoppedExhausted
    );
    let mut last_two_meet: Vec<Value> = (1..=2)
        .map(|n| round(n, 2.0, 4.0, 70.0, None, None))
        .collect();
    last_two_meet.extend((3..=4).map(|n| round(n, 2.0, 4.0, 90.0, None, None)));
    assert_eq!(
        evaluate(&study(json!({}), last_two_meet)).unwrap().status,
        Status::StoppedMet,
        "met wins over exhausted"
    );
}

#[test]
fn the_conservative_basis_uses_the_bound_on_the_safe_side() {
    let rounds = vec![
        round(1, 2.0, 4.0, 90.0, Some((84.0, 96.0)), None),
        round(2, 2.0, 4.0, 90.0, Some((86.0, 94.0)), None),
    ];
    let estimate = study(json!({}), rounds.clone());
    assert_eq!(evaluate(&estimate).unwrap().status, Status::StoppedMet);
    let conservative = study(json!({"basis": "conservative_bound"}), rounds);
    assert_eq!(
        evaluate(&conservative).unwrap().status,
        Status::Continue,
        "round 1's lower bound is 84"
    );
    // A round with no bound can not meet a rule that needs one.
    let no_bound = study(
        json!({"basis": "conservative_bound"}),
        vec![round(1, 2.0, 4.0, 99.0, None, None)],
    );
    assert!(evaluate(&no_bound).unwrap().rounds_meeting.is_empty());
}

#[test]
fn at_most_flips_the_comparison_and_the_upper_bound_is_the_safe_one() {
    let rounds = vec![
        round(1, 2.0, 4.0, 4.0, Some((3.0, 4.8)), None),
        round(2, 2.0, 4.0, 3.0, Some((2.0, 4.9)), None),
    ];
    let estimate = study(
        json!({"direction": "at_most", "threshold": 5.0}),
        rounds.clone(),
    );
    assert_eq!(evaluate(&estimate).unwrap().status, Status::StoppedMet);
    let strict = study(
        json!({"direction": "at_most", "threshold": 4.85, "basis": "conservative_bound"}),
        rounds,
    );
    assert_eq!(
        evaluate(&strict).unwrap().status,
        Status::Continue,
        "round 2's upper bound is 4.9"
    );
}

#[test]
fn the_recorded_interim_must_agree_with_the_rounds() {
    let rounds = vec![round(1, 2.0, 4.0, 90.0, None, None)];
    let mut honest = study(json!({}), rounds.clone());
    honest.interim = Some(Interim {
        rounds_analysed: 1,
        status: Status::Continue,
    });
    assert!(
        problems_of(&honest).is_empty(),
        "{:?}",
        problems_of(&honest)
    );
    // A record that declares victory the rounds do not support is refused.
    let mut claimed = study(json!({}), rounds);
    claimed.interim = Some(Interim {
        rounds_analysed: 1,
        status: Status::StoppedMet,
    });
    assert!(
        problems_of(&claimed)
            .iter()
            .any(|p| p.contains("interim says")),
        "{:?}",
        problems_of(&claimed)
    );
    // And an interim with no rule to derive it from.
    let mut orphan = study(json!({}), vec![]);
    orphan.sequential = None;
    orphan.interim = Some(Interim {
        rounds_analysed: 0,
        status: Status::Continue,
    });
    assert!(problems_of(&orphan)
        .iter()
        .any(|p| p.contains("needs a sequential")));
}

#[test]
fn the_machine_rule_is_locked_but_rounds_interim_and_recommendations_are_not() {
    let previous = study(json!({}), vec![round(1, 2.0, 4.0, 80.0, None, None)]);
    let mut moved_goalposts = previous.clone();
    moved_goalposts.sequential.as_mut().unwrap().rule.threshold = 70.0;
    assert_eq!(
        locked_changes(&previous, &moved_goalposts),
        vec!["sequential"]
    );
    let mut dropped = previous.clone();
    dropped.sequential = None;
    assert_eq!(locked_changes(&previous, &dropped), vec!["sequential"]);
    let mut more_rounds = previous.clone();
    more_rounds
        .rounds
        .push(serde_json::to_value(round(2, 2.0, 4.0, 90.0, None, None)).unwrap());
    more_rounds.interim = Some(Interim {
        rounds_analysed: 2,
        status: Status::Continue,
    });
    more_rounds
        .recommendations
        .push(json!({"text": "keep going"}));
    assert!(locked_changes(&previous, &more_rounds).is_empty());
}

#[test]
fn no_recommendation_once_the_rule_says_stop() {
    let done = study(
        json!({}),
        vec![
            round(1, 2.0, 4.0, 90.0, None, None),
            round(2, 2.0, 4.0, 90.0, None, None),
        ],
    );
    assert!(recommend_next(&done).unwrap().is_none());
}

#[test]
fn with_no_result_yet_the_current_ranges_are_suggested_for_the_first_round() {
    let fresh = study(json!({}), vec![]);
    let next = recommend_next(&fresh).unwrap().unwrap();
    assert_eq!(next.round, 1);
    assert_eq!(next.factors, fresh.factors);
    let waiting = study(json!({}), vec![unanalysed(1)]);
    assert_eq!(recommend_next(&waiting).unwrap().unwrap().round, 2);
}

#[test]
fn the_next_range_is_centred_on_the_best_run_and_halved() {
    let study = study(
        json!({}),
        vec![round(1, 2.0, 4.0, 80.0, None, Some((3.5, 12.0)))],
    );
    let next = recommend_next(&study).unwrap().unwrap();
    assert_eq!(next.round, 2);
    let binder = next.factors.iter().find(|f| f.name == "binder").unwrap();
    // Width 2 halves to 1, centred on 3.5.
    assert_eq!((binder.low, binder.high), (3.0, 4.0));
    let compression = next
        .factors
        .iter()
        .find(|f| f.name == "compression")
        .unwrap();
    assert_eq!((compression.low, compression.high), (11.0, 13.0));
    assert!(next.basis.contains("round 1"));
}

#[test]
fn the_best_round_is_the_highest_for_at_least_and_the_lowest_for_at_most() {
    let rounds = vec![
        round(1, 2.0, 4.0, 70.0, None, Some((2.2, 11.0))),
        round(2, 2.0, 4.0, 84.0, None, Some((3.8, 13.0))),
        round(3, 2.0, 4.0, 60.0, None, Some((2.0, 10.0))),
    ];
    let up = recommend_next(&study(json!({"max_rounds": 6}), rounds.clone()))
        .unwrap()
        .unwrap();
    assert!(up.basis.contains("round 2"), "{}", up.basis);
    let down = recommend_next(&study(
        json!({"max_rounds": 6, "direction": "at_most", "threshold": 50.0}),
        rounds,
    ))
    .unwrap()
    .unwrap();
    assert!(down.basis.contains("round 3"), "{}", down.basis);
}

#[test]
fn a_range_near_an_edge_is_moved_inside_its_bounds_without_leaving_them() {
    // The best run sits at the very top of compression's range, and bounds end at 16.
    let mut value = round(1, 2.0, 4.0, 80.0, None, Some((3.0, 16.0)));
    value["factors"][1] = json!({"name": "compression", "low": 14.0, "high": 16.0});
    let near_edge = recommend_next(&study(json!({}), vec![value]))
        .unwrap()
        .unwrap();
    let compression = near_edge
        .factors
        .iter()
        .find(|f| f.name == "compression")
        .unwrap();
    assert_eq!((compression.low, compression.high), (15.0, 16.0));
    for factor in &near_edge.factors {
        let bounds = [(1.0, 6.0), (8.0, 16.0)][near_edge
            .factors
            .iter()
            .position(|f| f.name == factor.name)
            .unwrap()];
        assert!(
            factor.low >= bounds.0 && factor.high <= bounds.1 && factor.low < factor.high,
            "{factor:?}"
        );
    }
    // Without a recorded best run the middle of the range is used.
    let plain = recommend_next(&study(
        json!({}),
        vec![round(1, 2.0, 4.0, 80.0, None, None)],
    ))
    .unwrap()
    .unwrap();
    let binder = plain.factors.iter().find(|f| f.name == "binder").unwrap();
    assert_eq!((binder.low, binder.high), (2.5, 3.5));
}

fn artifact(values: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"values": values, "provenance": {"engine": "R"}})).unwrap()
}

fn with_hash(round_number: u32, bytes: &[u8], has_ci: bool) -> Value {
    let mut value = round(
        round_number,
        2.0,
        4.0,
        88.5,
        has_ci.then_some((84.0, 93.0)),
        None,
    );
    value["results"]["dissolution_30"]["source"]["sha256"] =
        json!(format!("{:x}", Sha256::digest(bytes)));
    value
}

#[test]
fn a_number_is_verified_when_its_artifact_has_the_hash_the_key_and_the_value() {
    let bytes = artifact(
        json!({"dissolution": {"value": 88.5}, "dissolution_lo": {"value": 84.0}, "dissolution_hi": {"value": 93.0}}),
    );
    let study = study(json!({}), vec![with_hash(1, &bytes, true)]);
    let read = |_: &str| Ok(bytes.clone());
    let checks = verify_sources(&study, &read);
    assert_eq!(checks.len(), 1);
    assert!(checks[0].is_verified, "{:?}", checks[0]);
}

#[test]
fn a_changed_artifact_a_missing_key_a_different_number_or_an_unreadable_file_is_not_verified() {
    let bytes = artifact(json!({"dissolution": {"value": 88.5}}));
    let study = study(json!({}), vec![with_hash(1, &bytes, false)]);

    let changed = artifact(json!({"dissolution": {"value": 88.5}, "extra": {"value": 1}}));
    let checks = verify_sources(&study, &|_: &str| Ok(changed.clone()));
    assert!(checks[0]
        .problem
        .as_deref()
        .unwrap()
        .contains("changed since"));

    // Same hash is impossible with other content, so test key and value with matching hashes.
    for (content, needle) in [
        (
            artifact(json!({"other": {"value": 88.5}})),
            "no value under dissolution",
        ),
        (
            artifact(json!({"dissolution": {"value": 70.0}})),
            "not 88.5",
        ),
    ] {
        let study = super::tests_support_hash(&study, &content);
        let checks = verify_sources(&study, &|_: &str| Ok(content.clone()));
        assert!(!checks[0].is_verified);
        assert!(
            checks[0].problem.as_deref().unwrap().contains(needle),
            "{:?}",
            checks[0]
        );
    }
    let checks = verify_sources(&study, &|_: &str| Err("gone".to_owned()));
    assert_eq!(checks[0].problem.as_deref(), Some("gone"));
    let not_json = b"plain text".to_vec();
    let study = super::tests_support_hash(&study, &not_json);
    let checks = verify_sources(&study, &|_: &str| Ok(not_json.clone()));
    assert!(checks[0]
        .problem
        .as_deref()
        .unwrap()
        .contains("not a JSON result file"));
}

#[test]
fn an_artifact_shared_by_several_results_is_read_once() {
    let bytes = artifact(json!({"dissolution": {"value": 88.5}}));
    let mut first = with_hash(1, &bytes, false);
    let mut second = with_hash(2, &bytes, false);
    for round in [&mut first, &mut second] {
        round["results"]["dissolution_30"]["source"]["artifact"] = json!("out/shared.json");
    }
    let study = study(json!({}), vec![first, second]);
    let reads = RefCell::new(0);
    let checks = verify_sources(&study, &|_: &str| {
        *reads.borrow_mut() += 1;
        Ok(bytes.clone())
    });
    assert!(checks.iter().all(|check| check.is_verified));
    assert_eq!(*reads.borrow(), 1);
}

#[test]
fn the_published_schema_names_the_sequential_fields_and_the_locked_set() {
    let schema: Value =
        serde_json::from_str(include_str!("../../../../schemas/study.schema.json")).unwrap();
    for field in ["sequential", "interim", "rounds"] {
        assert!(schema["properties"].get(field).is_some(), "{field}");
    }
    let rule = &schema["properties"]["sequential"]["properties"]["rule"]["properties"];
    for field in [
        "response",
        "direction",
        "threshold",
        "basis",
        "consecutive_rounds",
        "max_rounds",
    ] {
        assert!(rule.get(field).is_some(), "{field}");
    }
    let description = schema["description"].as_str().unwrap();
    for locked in LOCKED_FIELDS {
        assert!(
            description.contains(locked),
            "the schema description must name the locked field {locked}"
        );
    }
    assert!(schema["definitions"]["source"]["properties"]["sha256"]["pattern"].is_string());
}

// --- the tool ---

fn call(workspace: &std::path::Path, arguments: Value) -> crate::runtime::ToolResult {
    use crate::runtime::{ToolCall, ToolExecutionContext, ToolExecutor};
    let host = crate::capability::CapabilityHost::product_defaults().unwrap();
    host.execute(
        &ToolCall {
            id: "c".into(),
            name: "study_evaluate".into(),
            arguments,
        },
        &ToolExecutionContext {
            session_id: "s".into(),
            run_id: "r".into(),
            workspace: Some(workspace.to_string_lossy().to_string()),
            timeout: std::time::Duration::from_secs(10),
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            progress: std::sync::Arc::new(|_| {}),
            secrets: Default::default(),
            proxy: None,
        },
    )
}

fn write_study(workspace: &std::path::Path, study: &Study) {
    std::fs::write(
        workspace.join("study.json"),
        serde_json::to_vec_pretty(study).unwrap(),
    )
    .unwrap();
}

#[test]
fn the_tool_reports_the_rule_the_trust_and_the_suggestion_from_files_in_the_workspace() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("out")).unwrap();
    let mut rounds = Vec::new();
    for number in 1..=2u32 {
        let bytes = artifact(json!({"dissolution": {"value": 88.5}}));
        std::fs::write(
            workspace.path().join(format!("out/round{number}.json")),
            &bytes,
        )
        .unwrap();
        rounds.push(with_hash(number, &bytes, false));
    }
    let mut study = study(json!({}), rounds);
    study.interim = Some(Interim {
        rounds_analysed: 2,
        status: Status::StoppedMet,
    });
    write_study(workspace.path(), &study);

    let result = call(workspace.path(), json!({"path": "study.json"}));
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(result.output["valid"], true);
    assert_eq!(result.output["numbers_trusted"], true);
    assert_eq!(result.output["evaluation"]["status"], "stopped_met");
    assert_eq!(result.output["interim"]["status"], "stopped_met");
    assert!(
        result.output["recommendation"].is_null(),
        "a stopped study gets no suggestion"
    );
    assert_eq!(result.output["sources"].as_array().unwrap().len(), 2);

    // Tamper with one artifact: the numbers are no longer trusted.
    std::fs::write(
        workspace.path().join("out/round2.json"),
        artifact(json!({"dissolution": {"value": 99.0}})),
    )
    .unwrap();
    let result = call(workspace.path(), json!({"path": "study.json"}));
    assert_eq!(result.output["numbers_trusted"], false);
    assert!(result.output["note"]
        .as_str()
        .unwrap()
        .contains("do not rely"));
}

#[test]
fn the_tool_suggests_the_next_round_while_the_study_goes_on() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("out")).unwrap();
    let bytes = artifact(json!({"dissolution": {"value": 88.5}}));
    std::fs::write(workspace.path().join("out/round1.json"), &bytes).unwrap();
    let mut round = with_hash(1, &bytes, false);
    round["best_point"] = json!({"binder": 3.0, "compression": 12.0});
    write_study(workspace.path(), &study(json!({}), vec![round]));
    let result = call(workspace.path(), json!({"path": "study.json"}));
    assert_eq!(result.output["evaluation"]["status"], "continue");
    assert_eq!(result.output["recommendation"]["round"], 2);
}

#[test]
fn the_tool_lists_every_problem_of_an_invalid_study_and_refuses_files_outside_the_workspace() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(
        workspace.path().join("study.json"),
        "{\"schema_version\": 2}",
    )
    .unwrap();
    let invalid = call(workspace.path(), json!({"path": "study.json"}));
    assert_eq!(invalid.output["valid"], false);
    assert!(!invalid.output["problems"].as_array().unwrap().is_empty());
    let outside = call(workspace.path(), json!({"path": "../elsewhere.json"}));
    assert!(outside.error.is_some());
    assert!(call(workspace.path(), json!({})).error.is_some());
}
