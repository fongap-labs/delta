//! Provider metadata and legacy wire-support utilities.
//!
//! Runtime model execution does not depend on this module for routing. The
//! provider transport stays protocol-only; these helpers remain isolated while
//! the existing wire contract is still supported.

use serde_json::{json, Value};

// -- Capabilities (matrix + heuristics) ------------------------------------

#[allow(dead_code)]
struct MatrixEntry {
    id: &'static str,
    tools: bool,
    vision: bool,
    pdf: bool,
    parallel_tool_calls: bool,
    streaming: bool,
    context_window: Option<i64>,
}

const AGENTIC: MatrixEntry = MatrixEntry {
    id: "",
    tools: true,
    vision: false,
    pdf: false,
    parallel_tool_calls: true,
    streaming: true,
    context_window: None,
};
const AGENTIC_VISION: MatrixEntry = MatrixEntry {
    id: "",
    tools: true,
    vision: true,
    pdf: true,
    parallel_tool_calls: true,
    streaming: true,
    context_window: None,
};

const MATRIX: &[MatrixEntry] = &[
    MatrixEntry {
        id: "gpt-5.6-sol",
        context_window: Some(400_000),
        ..AGENTIC_VISION
    },
    MatrixEntry {
        id: "gpt-5.6-terra",
        context_window: Some(400_000),
        ..AGENTIC_VISION
    },
    MatrixEntry {
        id: "gpt-5.6-luna",
        context_window: Some(400_000),
        ..AGENTIC_VISION
    },
    MatrixEntry {
        id: "gpt-5.5",
        context_window: Some(400_000),
        ..AGENTIC_VISION
    },
    MatrixEntry {
        id: "anthropic:claude-fable-5",
        context_window: Some(1_000_000),
        ..AGENTIC_VISION
    },
    MatrixEntry {
        id: "anthropic:claude-opus-4-8",
        context_window: Some(200_000),
        ..AGENTIC_VISION
    },
    MatrixEntry {
        id: "anthropic:claude-sonnet-4-6",
        context_window: Some(200_000),
        ..AGENTIC_VISION
    },
    MatrixEntry {
        id: "anthropic:claude-haiku-4-5",
        context_window: Some(200_000),
        ..AGENTIC_VISION
    },
    MatrixEntry {
        id: "meta:muse-spark-1.1",
        tools: true,
        vision: true,
        pdf: false,
        parallel_tool_calls: true,
        streaming: true,
        context_window: None,
    },
    MatrixEntry {
        id: "zai:glm-5.2",
        context_window: Some(128_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "deepseek:deepseek-v4-flash",
        context_window: Some(128_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "deepseek:deepseek-v4-pro",
        context_window: Some(128_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "kimi:kimi-k2.6",
        context_window: Some(256_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "minimax:MiniMax-M2.5",
        ..AGENTIC
    },
    MatrixEntry {
        id: "qwen:qwen3-max",
        context_window: Some(256_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "xai:grok-4.3",
        context_window: Some(256_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "mistral:mistral-large-latest",
        context_window: Some(128_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "together:thinkingmachines/Inkling",
        ..AGENTIC
    },
    MatrixEntry {
        id: "together:zai-org/GLM-5.2",
        context_window: Some(128_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "together:moonshotai/Kimi-K3",
        tools: true,
        vision: true,
        pdf: false,
        parallel_tool_calls: true,
        streaming: true,
        context_window: Some(1_000_000),
    },
    MatrixEntry {
        id: "together:moonshotai/Kimi-K2.7-Code",
        context_window: Some(256_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "together:moonshotai/Kimi-K2.6",
        context_window: Some(256_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "together:deepseek-ai/DeepSeek-V4-Pro",
        context_window: Some(128_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "together:meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8",
        context_window: Some(1_000_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "fireworks:accounts/fireworks/models/glm-5p2",
        context_window: Some(128_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "fireworks:accounts/fireworks/models/kimi-k2p6",
        context_window: Some(256_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "fireworks:accounts/fireworks/models/deepseek-v4-pro",
        context_window: Some(128_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "fireworks:accounts/fireworks/models/llama4-maverick-instruct-basic",
        context_window: Some(1_000_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "openrouter:z-ai/glm-5.2",
        context_window: Some(128_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "openrouter:moonshotai/kimi-k2.6",
        context_window: Some(256_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "openrouter:deepseek/deepseek-v4-pro",
        context_window: Some(128_000),
        ..AGENTIC
    },
    MatrixEntry {
        id: "openrouter:meta-llama/llama-4-maverick",
        context_window: Some(1_000_000),
        ..AGENTIC
    },
];

fn caps_json(
    has_tools: bool,
    has_vision: bool,
    has_pdf: bool,
    is_parallel: bool,
    is_streaming: bool,
) -> Value {
    let parallel = is_parallel;
    let pdf = has_pdf;
    let streaming = is_streaming;
    let tools = has_tools;
    let vision = has_vision;
    json!({
        "tools": tools,
        "vision": vision,
        "pdf": pdf,
        "parallel_tool_calls": parallel,
        "streaming": streaming,
    })
}

pub fn capabilities_for(model: &str) -> Value {
    if let Some(entry) = MATRIX.iter().find(|e| e.id == model) {
        return caps_json(
            entry.tools,
            entry.vision,
            entry.pdf,
            entry.parallel_tool_calls,
            entry.streaming,
        );
    }
    let (provider, name) = if let Some((p, n)) = model.split_once(':') {
        (p.to_lowercase(), n.to_lowercase())
    } else {
        (String::new(), model.to_lowercase())
    };
    if provider == "anthropic" {
        return caps_json(true, true, true, true, true);
    }
    if name.starts_with("gpt-5") || name.starts_with("gpt-4") {
        return caps_json(true, true, true, true, true);
    }
    if name.starts_with("o1") || name.starts_with("o3") || name.starts_with("o4") {
        return caps_json(true, false, false, false, true);
    }
    if name.starts_with("deepseek")
        || name.starts_with("glm")
        || name.starts_with("kimi")
        || name.starts_with("minimax")
        || name.starts_with("qwen")
        || name.starts_with("grok")
        || name.starts_with("mistral")
        || name.starts_with("magistral")
    {
        return caps_json(true, false, false, true, true);
    }
    caps_json(true, false, false, false, true)
}

// -- Endpoint caps (stateful: endpoint_caps.json) ---------------------------

pub fn endpoint_caps_read(path: &str, endpoint_key: &str) -> Value {
    let p = std::path::PathBuf::from(path);
    let store: Value = match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or(json!({})),
        Err(_) => json!({}),
    };
    store.get(endpoint_key).cloned().unwrap_or(json!({}))
}

pub fn endpoint_reject(path: &str, endpoint_key: &str, field: &str) -> Value {
    let p = std::path::PathBuf::from(path);
    let mut store: Value = match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or(json!({})),
        Err(_) => json!({}),
    };
    if !store.is_object() {
        store = json!({});
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let entry = store
        .as_object_mut()
        .unwrap()
        .entry(endpoint_key.to_string())
        .or_insert(json!({}));
    if let Some(obj) = entry.as_object_mut() {
        obj.insert(field.to_string(), json!(false));
        obj.insert("updated_at".to_string(), json!(now));
    }
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&p, serde_json::to_string_pretty(&store).unwrap_or_default());
    json!({"ok": true})
}

// -- Health (stateful: provider_health.json) -------------------------------

const HEALTH_MAX_SAMPLES: usize = 200;
const HEALTH_MAX_AGE_SECS: f64 = 86400.0;

pub fn health_record(
    path: &str,
    endpoint: &str,
    model: &str,
    is_ok: bool,
    ttft_ms: Option<f64>,
    duration_ms: Option<f64>,
    error_class: Option<&str>,
) -> Value {
    let ok = is_ok;
    let p = std::path::PathBuf::from(path);
    let mut store: Value = match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or(json!({})),
        Err(_) => json!({}),
    };
    if !store.is_object() {
        store = json!({});
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let store_obj = store.as_object_mut().unwrap();
    let bucket = store_obj.entry(endpoint.to_string()).or_insert(json!({}));
    if !bucket.is_object() {
        *bucket = json!({});
    }
    let bucket_obj = bucket.as_object_mut().unwrap();
    let row = bucket_obj
        .entry(model.to_string())
        .or_insert(json!({"samples": 0, "errors": 0, "ttft_ms": [], "duration_ms": [], "last_error_class": null, "last_ts": 0.0}));
    if !row.is_object() {
        *row = json!({"samples": 0, "errors": 0, "ttft_ms": [], "duration_ms": [], "last_error_class": null, "last_ts": 0.0});
    }
    let row_obj = row.as_object_mut().unwrap();
    let samples = row_obj.get("samples").and_then(|v| v.as_i64()).unwrap_or(0) + 1;
    row_obj.insert("samples".to_string(), json!(samples));
    if !ok {
        let errors = row_obj.get("errors").and_then(|v| v.as_i64()).unwrap_or(0) + 1;
        row_obj.insert("errors".to_string(), json!(errors));
        row_obj.insert("last_error_class".to_string(), json!(error_class));
    }
    // Cap rolling buffers.
    if let Some(ttft) = ttft_ms {
        let mut arr = row_obj
            .get("ttft_ms")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        arr.push(json!(ttft));
        if arr.len() > HEALTH_MAX_SAMPLES {
            arr = arr[arr.len() - HEALTH_MAX_SAMPLES..].to_vec();
        }
        row_obj.insert("ttft_ms".to_string(), Value::Array(arr));
    }
    if let Some(dur) = duration_ms {
        let mut arr = row_obj
            .get("duration_ms")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        arr.push(json!(dur));
        if arr.len() > HEALTH_MAX_SAMPLES {
            arr = arr[arr.len() - HEALTH_MAX_SAMPLES..].to_vec();
        }
        row_obj.insert("duration_ms".to_string(), Value::Array(arr));
    }
    row_obj.insert("last_ts".to_string(), json!(now));
    // Drop stale entries for this endpoint.
    let to_drop: Vec<String> = bucket_obj
        .iter()
        .filter_map(|(m, other)| {
            if m == model {
                return None;
            }
            let ts = other.get("last_ts").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if now - ts > HEALTH_MAX_AGE_SECS {
                Some(m.clone())
            } else {
                None
            }
        })
        .collect();
    for m in to_drop {
        bucket_obj.remove(&m);
    }
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&p, serde_json::to_string_pretty(&store).unwrap_or_default());
    json!({"ok": true})
}

pub fn health_profile(path: &str, endpoint: &str, model: &str) -> Value {
    let p = std::path::PathBuf::from(path);
    let store: Value = match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or(json!({})),
        Err(_) => json!({}),
    };
    let row = store
        .get(endpoint)
        .and_then(|b| b.get(model))
        .cloned()
        .unwrap_or(json!({}));
    row
}

pub fn health_all(path: &str) -> Value {
    let p = std::path::PathBuf::from(path);
    let store: Value = match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or(json!({})),
        Err(_) => json!({}),
    };
    store
}

// -- Routing (provider name resolution) -------------------------------------

pub fn route(model: &str, providers: &[String], default: &str) -> Value {
    if let Some((prefix, rest)) = model.split_once(':') {
        if providers.iter().any(|p| p == prefix) {
            return json!({"provider": prefix, "bare": rest});
        }
    }
    json!({"provider": default, "bare": model})
}

// -- Friendly model error (access/quota translation) -----------------------

pub fn friendly_model_error(model: &str, message: &str) -> Value {
    let text = message.to_lowercase();
    let no_access = format!(
        "Your account doesn't have access to {} - new models can roll out gradually or require a plan upgrade. Pick a different model, or check the provider's console for availability.",
        model
    );
    let no_quota = format!(
        "Your account is out of quota for {} - add credits or raise the limit in the provider's billing console, or pick a different model.",
        model
    );
    let no_quota_markers = [
        "insufficient_quota",
        "exceeded your current quota",
        "credit balance is too low",
        "billing hard limit",
    ];
    for m in no_quota_markers {
        if text.contains(m) {
            return json!({"message": no_quota});
        }
    }
    let no_access_markers = [
        "model_not_found",
        "does not exist or you do not have access",
        "does not have access to model",
        "permission_error",
        "permission denied",
    ];
    for m in no_access_markers {
        if text.contains(m) {
            return json!({"message": no_access});
        }
    }
    let bare_model = model.rsplit(':').next().unwrap_or(model).to_lowercase();
    if text.contains("not_found_error") && text.contains(&format!("model: {}", bare_model)) {
        return json!({"message": no_access});
    }
    json!({"message": Value::Null})
}
