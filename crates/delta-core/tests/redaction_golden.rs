//! The Rust redaction policy must give exactly the results recorded in the shared fixture, which
//! `packages/sanitize.py` is tested against too (`tests/packs/delta` in action-worker).
//!
//! Inputs that look like real credentials are written as placeholders in the fixture
//! (`<<AWS_KEY>>`, `<<GITHUB_TOKEN>>`) and assembled here, so the repository never contains a string
//! that a secret scanner would report.

use delta_core::redact::redact_value;
use serde_json::Value;

const FIXTURE: &str = include_str!("fixtures/redaction_golden.json");

fn aws_key() -> String {
    format!("{}{}", "AKIA", "IOSFODNN7EXAMPLE")
}

fn github_token() -> String {
    format!("{}{}", "ghp_", "a".repeat(36))
}

fn expand(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(
            text.replace("<<AWS_KEY>>", &aws_key())
                .replace("<<GITHUB_TOKEN>>", &github_token()),
        ),
        Value::Array(items) => Value::Array(items.iter().map(expand).collect()),
        Value::Object(map) => {
            Value::Object(map.iter().map(|(k, v)| (k.clone(), expand(v))).collect())
        }
        other => other.clone(),
    }
}

fn cases() -> Vec<(String, Value, Value)> {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture is valid JSON");
    assert_eq!(fixture["schema_version"], 1);
    fixture["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|case| {
            (
                case["name"].as_str().expect("name").to_string(),
                expand(&case["input"]),
                expand(&case["expected"]),
            )
        })
        .collect()
}

#[test]
fn every_golden_case_matches() {
    let cases = cases();
    assert!(
        cases.len() >= 15,
        "the fixture shrank to {} cases",
        cases.len()
    );
    for (name, input, expected) in cases {
        assert_eq!(redact_value(&input), expected, "case: {name}");
    }
}

#[test]
fn every_golden_result_is_stable_under_a_second_pass() {
    for (name, _, expected) in cases() {
        assert_eq!(redact_value(&expected), expected, "case: {name}");
    }
}

#[test]
fn no_golden_result_still_contains_the_secrets_it_was_given() {
    let secrets = [
        "hunter2".to_string(),
        "abc123".to_string(),
        "s3cret".to_string(),
        "sk-abcdefghijklmnopqrstuvwxyz0123".to_string(),
        aws_key(),
        github_token(),
        "dBjftJeZ4CVPm".to_string(),
    ];
    for (name, _, expected) in cases() {
        let text = expected.to_string();
        for (index, secret) in secrets.iter().enumerate() {
            assert!(
                !text.contains(secret.as_str()),
                "case {name} leaks sensitive value #{index}"
            );
        }
    }
}

#[test]
fn the_placeholders_really_stand_for_credential_shaped_text() {
    let text = redact_value(&Value::String(format!(
        "k {} {}",
        aws_key(),
        github_token()
    )));
    assert_eq!(text, Value::String("k [redacted] [redacted]".to_string()));
}
