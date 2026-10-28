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
        // Network egress (ADR-0057): which proxy, if any, every outbound door uses. The
        // `system` mode reports what the OS currently says without persisting it.
        output.insert(
            "proxy".to_string(),
            crate::net::proxy::ProxySettings::snapshot(prefs),
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

    /// Vision bridge settings. An empty `vision_model` turns the bridge off. `vision_support` maps
    /// a model id to whether that model takes pictures itself; it is merged, not replaced.
    pub(crate) fn set_vision_settings(
        prefs: &mut Map<String, Value>,
        patch: &Value,
    ) -> Result<(), String> {
        if let Some(model) = patch.get("vision_model").and_then(Value::as_str) {
            prefs.insert("vision_model".to_string(), json!(model.trim()));
        }
        if let Some(protocol) = patch.get("vision_protocol").and_then(Value::as_str) {
            let protocol = protocol.trim();
            if !protocol.is_empty() && !crate::vision::PROTOCOLS.contains(&protocol) {
                return Err(format!(
                    "vision_protocol must be one of {}",
                    crate::vision::PROTOCOLS.join(", ")
                ));
            }
            prefs.insert("vision_protocol".to_string(), json!(protocol));
        }
        if let Some(value) = patch.get("vision_max_image_mb").and_then(Value::as_i64) {
            prefs.insert("vision_max_image_mb".to_string(), json!(value.clamp(1, 8)));
        }
        if let Some(value) = patch.get("vision_max_tokens").and_then(Value::as_i64) {
            prefs.insert("vision_max_tokens".to_string(), json!(value.clamp(1, 4096)));
        }
        if let Some(value) = patch.get("vision_timeout_secs").and_then(Value::as_f64) {
            prefs.insert(
                "vision_timeout_secs".to_string(),
                json!(value.clamp(5.0, 300.0)),
            );
        }
        if let Some(prompt) = patch.get("vision_prompt").and_then(Value::as_str) {
            let prompt = prompt.trim();
            if prompt.chars().count() > crate::vision::MAX_PROMPT_CHARS {
                return Err(format!(
                    "vision_prompt must be at most {} characters",
                    crate::vision::MAX_PROMPT_CHARS
                ));
            }
            prefs.insert("vision_prompt".to_string(), json!(prompt));
        }
        if let Some(support) = patch.get("vision_support").and_then(Value::as_object) {
            let mut merged = prefs
                .get("vision_support")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            // Clearing the user's mark also clears what was learned, so the model is judged afresh.
            let mut learned = prefs
                .get("vision_learned")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            for (model, takes_images) in support {
                if takes_images.is_null() {
                    merged.remove(model);
                    learned.remove(model);
                    continue;
                }
                let flag = takes_images
                    .as_bool()
                    .ok_or_else(|| format!("vision_support.{model} must be true, false or null"))?;
                merged.insert(model.clone(), json!(flag));
            }
            prefs.insert("vision_support".to_string(), Value::Object(merged));
            prefs.insert("vision_learned".to_string(), Value::Object(learned));
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

    /// Apply a network-proxy patch and return what is now configured.
    ///
    /// Deliberately its own door rather than a branch inside `set_pdf_settings`: proxy
    /// configuration is the one setting a user changes while debugging why their network
    /// is broken, and it needs its own validation errors rather than sharing a generic
    /// "invalid patch" path.
    pub(crate) fn set_proxy_settings(
        prefs: &mut Map<String, Value>,
        patch: &Value,
    ) -> Result<crate::net::proxy::ProxySettings, String> {
        crate::net::proxy::ProxySettings::write_prefs(prefs, patch)
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
