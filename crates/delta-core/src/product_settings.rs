//! Product-facing preferences kept separate from model/provider semantics.
//!
//! Persistence remains owned by ModelAuthority for now so existing atomic writes
//! and on-disk compatibility stay unchanged. This module owns only the meaning,
//! defaults and validation rules of non-model product settings.

use serde_json::{json, Map, Value};

pub(crate) struct ProductSettings;

impl ProductSettings {
    pub(crate) fn snapshot(prefs: &Map<String, Value>) -> Map<String, Value> {
        let mut output = Map::new();
        output.insert(
            "onboarded".to_string(),
            json!(prefs
                .get("onboarded")
                .and_then(Value::as_bool)
                .unwrap_or(false)),
        );
        output.insert(
            "language".to_string(),
            prefs.get("language").cloned().unwrap_or(Value::Null),
        );
        output.insert(
            "sessions_peek".to_string(),
            json!(bounded_i64(prefs.get("sessions_peek"), 5, 1, 50)),
        );
        output.insert(
            "context_bar".to_string(),
            json!(prefs
                .get("context_bar")
                .and_then(Value::as_bool)
                .unwrap_or(false)),
        );
        output.insert(
            "scratch_base".to_string(),
            json!(prefs
                .get("scratch_base")
                .and_then(Value::as_str)
                .unwrap_or("~/Delta")),
        );
        output.insert(
            "pdf_fallback".to_string(),
            json!(match prefs.get("pdf_fallback").and_then(Value::as_str) {
                Some("images") => "images",
                _ => "text",
            }),
        );
        output.insert(
            "pdf_max_pages".to_string(),
            json!(bounded_i64(prefs.get("pdf_max_pages"), 20, 1, 100)),
        );
        output.insert(
            "pdf_max_mb".to_string(),
            json!(bounded_i64(prefs.get("pdf_max_mb"), 10, 1, 10)),
        );
        output.insert(
            "compaction_threshold_pct".to_string(),
            json!(bounded_f64(
                prefs.get("compaction_threshold_pct"),
                0.8,
                0.10,
                0.95,
            )),
        );
        output.insert(
            "compaction_cap_tokens".to_string(),
            json!(bounded_i64(
                prefs.get("compaction_cap_tokens"),
                250_000,
                10_000,
                2_000_000,
            )),
        );
        output.insert(
            "compaction_model".to_string(),
            json!(prefs
                .get("compaction_model")
                .and_then(Value::as_str)
                .unwrap_or("")),
        );
        output
    }

    pub(crate) fn set_onboarded(prefs: &mut Map<String, Value>, value: bool) {
        prefs.insert("onboarded".to_string(), json!(value));
    }

    pub(crate) fn set_language(prefs: &mut Map<String, Value>, language: &str) {
        let value = language.trim();
        if value.is_empty() {
            prefs.remove("language");
        } else {
            prefs.insert("language".to_string(), json!(value));
        }
    }

    pub(crate) fn set_context_bar(prefs: &mut Map<String, Value>, shown: bool) {
        prefs.insert("context_bar".to_string(), json!(shown));
    }

    pub(crate) fn set_sessions_peek(prefs: &mut Map<String, Value>, count: i64) -> i64 {
        let value = count.clamp(1, 50);
        prefs.insert("sessions_peek".to_string(), json!(value));
        value
    }

    pub(crate) fn set_scratch_base<'a>(prefs: &mut Map<String, Value>, path: &'a str) -> &'a str {
        let value = path.trim();
        prefs.insert("scratch_base".to_string(), json!(value));
        value
    }

    pub(crate) fn set_pdf_settings(
        prefs: &mut Map<String, Value>,
        patch: &Value,
    ) -> Result<(), String> {
        if let Some(mode) = patch.get("pdf_fallback").and_then(Value::as_str) {
            if !matches!(mode, "text" | "images") {
                return Err("pdf_fallback must be 'text' or 'images'".to_string());
            }
            prefs.insert("pdf_fallback".to_string(), json!(mode));
        }
        if let Some(value) = patch.get("pdf_max_pages").and_then(Value::as_i64) {
            prefs.insert("pdf_max_pages".to_string(), json!(value.clamp(1, 100)));
        }
        if let Some(value) = patch.get("pdf_max_mb").and_then(Value::as_i64) {
            prefs.insert("pdf_max_mb".to_string(), json!(value.clamp(1, 10)));
        }
        Ok(())
    }

    pub(crate) fn set_compaction_settings(
        prefs: &mut Map<String, Value>,
        patch: &Value,
    ) -> Result<(), String> {
        if let Some(value) = patch
            .get("compaction_threshold_pct")
            .and_then(Value::as_f64)
        {
            if !(0.10..=0.95).contains(&value) {
                return Err("compaction_threshold_pct must be between 0.10 and 0.95".to_string());
            }
            prefs.insert("compaction_threshold_pct".to_string(), json!(value));
        }
        if let Some(value) = patch.get("compaction_cap_tokens").and_then(Value::as_i64) {
            prefs.insert(
                "compaction_cap_tokens".to_string(),
                json!(value.clamp(10_000, 2_000_000)),
            );
        }
        if let Some(value) = patch.get("compaction_model").and_then(Value::as_str) {
            prefs.insert("compaction_model".to_string(), json!(value));
        }
        Ok(())
    }
}

fn bounded_i64(value: Option<&Value>, default: i64, min: i64, max: i64) -> i64 {
    value
        .and_then(Value::as_i64)
        .unwrap_or(default)
        .clamp(min, max)
}

fn bounded_f64(value: Option<&Value>, default: f64, min: f64, max: f64) -> f64 {
    value
        .and_then(Value::as_f64)
        .unwrap_or(default)
        .clamp(min, max)
}
