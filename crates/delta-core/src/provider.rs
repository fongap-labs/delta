//! Provider wire-protocol transport.
//!
//! Implements OpenAI Chat Completions HTTP calls in Rust.
//! Response format matches the Python `AssistantTurn` / `StreamChunk`
//! contract (base.py) so the engine keeps working unchanged.

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct ProviderRequest {
    pub protocol: String,
    pub model: String,
    pub messages: Value,
    #[serde(default)]
    pub tools: Option<Value>,
    #[serde(default)]
    pub settings: Option<Value>,
    pub api_key: String,
    pub base_url: String,
    #[serde(default)]
    pub timeout_secs: Option<f64>,
    /// Re-send without the optional parameters a provider refused. Set by the runtime after the
    /// first attempt comes back as a protocol rejection, and consumed by the body builders.
    #[serde(skip)]
    pub reduced: bool,
    /// When the egress check relied on the endpoint's addresses (ADR-0057), the connection must
    /// use exactly those addresses instead of resolving the name again. Empty means resolve as usual.
    #[serde(skip)]
    pub pinned_addrs: Vec<std::net::SocketAddr>,
    /// Which proxy this request goes through, resolved once per turn from the persisted
    /// network settings. `None` means direct. Set by the runtime; the provider layer never
    /// reads preferences itself, so a test can pin the behaviour.
    #[serde(skip)]
    pub proxy: Option<ureq::Proxy>,
}

/// Optional request parameters, per protocol, that a provider may lawfully refuse.
///
/// These are the fields a compatible gateway or a self-hosted endpoint most often rejects with a
/// 400 — which `fongap-labs/ai-gateway` classifies as a *request-local* failure and stops
/// routing on. `model`, `messages` and `tools` are never stripped: without them the turn cannot
/// run at all.
///
/// `reasoning_effort` deserves a note. It is an OpenAI field; the Anthropic Messages API has no
/// such parameter (it uses `thinking: {type, budget_tokens}`), yet the runtime merges
/// `model_settings` into every protocol's body verbatim. On an Anthropic-compatible route it is
/// guaranteed to be an unknown field, which is exactly the kind of request that makes every
/// dispatched node fail as `client`.
fn optional_request_params(protocol: &str) -> &'static [&'static str] {
    match protocol {
        "anthropic" => &[
            "reasoning_effort",
            "temperature",
            "top_p",
            "top_k",
            "parallel_tool_calls",
            "presence_penalty",
            "frequency_penalty",
            "store",
            "include",
            "reasoning",
        ],
        "openai_responses" => &[
            "reasoning_effort",
            "temperature",
            "top_p",
            "parallel_tool_calls",
            "presence_penalty",
            "frequency_penalty",
            "max_output_tokens",
            "service_tier",
            "store",
            "include",
            "reasoning",
            "stream_options",
        ],
        // openai_chat and any unknown-but-openai-shaped protocol.
        _ => &[
            "reasoning_effort",
            "temperature",
            "top_p",
            "parallel_tool_calls",
            "presence_penalty",
            "frequency_penalty",
            "max_tokens",
            "logit_bias",
            "stop",
            "stream_options",
        ],
    }
}

/// Makes the connection use the addresses the egress check classified.
#[derive(Debug)]
struct Pinned(Vec<std::net::SocketAddr>);

impl ureq::Resolver for Pinned {
    fn resolve(&self, _netloc: &str) -> std::io::Result<Vec<std::net::SocketAddr>> {
        Ok(self.0.clone())
    }
}

/// Join a configured `base_url` with the protocol's endpoint, without duplicating a version
/// segment or the endpoint itself.
///
/// A provider base URL arrives in four shapes, and every one of them is in the wild:
///
/// | base_url | result |
/// |---|---|
/// | `https://host` | `https://host/v1/<endpoint>` |
/// | `https://host/v1` | `https://host/v1/<endpoint>` |
/// | `https://host/v1/messages` | unchanged — already a full endpoint |
/// | `https://host/api/v2` | `https://host/api/v2/<endpoint>` — version kept as given |
///
/// The previous implementation only recognised a literal `/v1` suffix, so a base URL that
/// already named the endpoint (`…/v1/messages`) was spliced into `…/v1/messages/v1/messages`.
/// It was also bypassed entirely by the Chat Completions path, which concatenated
/// `<base>/chat/completions` and therefore never gained the `/v1` a bare host needs — the
/// same field meant different things depending on protocol, which is what the UI help text
/// could not explain. Both paths now share this one function.
fn versioned_endpoint(base_url: &str, endpoint: &str) -> String {
    let base = base_url.trim_end_matches('/');
    // Stripping the endpoint can leave a trailing slash (`…/v1/messages` → `…/v1/`), which must
    // go before the version check or the last segment reads as empty.
    let stripped = match base.strip_suffix(endpoint) {
        Some(rest) => {
            let rest = rest.trim_end_matches('/');
            // The base already names the endpoint itself: `…/v1/messages`. Keep it whole.
            if has_version_segment(rest) {
                return base.to_string();
            }
            rest
        }
        None => base,
    };
    if has_version_segment(stripped) {
        format!("{stripped}/{endpoint}")
    } else {
        format!("{stripped}/v1/{endpoint}")
    }
}

/// True when the last path segment is an API version: `/v1`, `/v2`, `/v1beta`, `/api/v3`.
fn has_version_segment(path: &str) -> bool {
    path.rsplit('/').next().is_some_and(|segment| {
        let segment = segment.to_ascii_lowercase();
        segment.len() >= 2
            && segment.starts_with('v')
            && segment[1..].chars().all(|c| c.is_ascii_digit())
    })
}

#[derive(Default)]
struct ToolCallAccum {
    id: String,
    name: String,
    args: String,
}

const USER_AGENT: &str = concat!("Delta/", env!("CARGO_PKG_VERSION"));

/// How often a waiting provider call looks at its cancel flag.
const CANCEL_POLL: Duration = Duration::from_millis(50);

/// Longest wait for a TCP/TLS connection to open before the attempt counts as failed.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

fn timeout_duration(timeout_secs: Option<f64>) -> Duration {
    Duration::from_secs_f64(timeout_secs.unwrap_or(300.0).max(0.05))
}

/// Agent for one-shot (non-streaming) calls: `timeout` is a deadline for the whole exchange.
fn agent(timeout_secs: Option<f64>, proxy: Option<&ureq::Proxy>) -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new().user_agent(USER_AGENT);
    if let Some(proxy) = proxy {
        // A model call may carry an API key, so where it exits matters as much as for
        // web traffic: an unproxied call from a network that requires a proxy fails,
        // and the failure looks like "the model is down", not "the route is wrong".
        builder = builder.proxy(proxy.clone());
    }
    builder.timeout(timeout_duration(timeout_secs)).build()
}

/// Agent for streamed replies. A long answer legitimately outlives any fixed total deadline,
/// so only a connect limit and an idle limit (no byte for `timeout_secs`) apply: a stream
/// that keeps producing is never cut off, a stalled one still fails and can be retried.
fn stream_agent(
    timeout_secs: Option<f64>,
    pinned_addrs: &[std::net::SocketAddr],
    proxy: Option<&ureq::Proxy>,
) -> ureq::Agent {
    let idle = timeout_duration(timeout_secs);
    let mut builder = ureq::AgentBuilder::new()
        .user_agent(USER_AGENT)
        .timeout_connect(idle.min(CONNECT_TIMEOUT))
        .timeout_read(idle);
    if let Some(proxy) = proxy {
        builder = builder.proxy(proxy.clone());
    }
    if pinned_addrs.is_empty() {
        builder.build()
    } else {
        builder.resolver(Pinned(pinned_addrs.to_vec())).build()
    }
}

/// Machine-readable tags appended to a provider error so the retry decision can be made on
/// protocol signals rather than on the status code alone. See [`is_retryable_error`].
pub const NO_RETRY_TAG: &str = "[x-should-retry:false]";
const FAILURE_KINDS_TAG: &str = "[x-gateway-failure-kinds:";

/// Convert a ureq error into a message string that carries the gateway's retry instruction.
///
/// Two things must survive the conversion:
///
/// 1. **The body**, so the protocol-marker matcher (e.g. `'reasoning_effort' is not supported`)
///    still works across an HTTP boundary.
/// 2. **The headers the gateway sends to say whether retrying is worthwhile.** The status code
///    is not enough: `fongap-labs/ai-gateway` returns 502 for an upstream that rejected the
///    request (`failure_kinds: {client}` — see its `reliability/classify.ts`, where a 400 is
///    classified request-local and non-retryable) and pairs that 502 with the header
///    `x-should-retry: false`. Retrying a request the gateway has already declared terminal is
///    pure waste: it re-sends a body that was refused, and hammers a gateway that said no.
///
/// Both signals are appended as bracketed tags so the message stays human-readable while
/// remaining parseable by [`is_retryable_error`].
fn http_error(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, response) => {
            let no_retry = response
                .header("x-should-retry")
                .is_some_and(|value| value.eq_ignore_ascii_case("false"));
            let kinds = response
                .header("x-gateway-failure-kinds")
                .unwrap_or_default()
                .trim()
                .to_owned();
            let body = response.into_string().unwrap_or_default();
            let truncated: &str = if body.len() > 800 {
                let mut end = 800;
                while !body.is_char_boundary(end) {
                    end -= 1;
                }
                &body[..end]
            } else {
                &body
            };
            let mut message = format!("HTTP {code}: {truncated}");
            if no_retry {
                message.push(' ');
                message.push_str(NO_RETRY_TAG);
            }
            if !kinds.is_empty() {
                message.push(' ');
                message.push_str(FAILURE_KINDS_TAG);
                message.push_str(&kinds);
                message.push(']');
            }
            message
        }
        ureq::Error::Transport(t) => format!("transport error: {t}"),
    }
}

/// A gateway failure family that retrying cannot fix.
///
/// `fongap-labs/ai-gateway` enumerates its failure kinds in `reliability/classify.ts`. The
/// retryable ones (`rate_limit`, `server`, `network`, `*_timeout`, `stream_interrupted`) are
/// the only families whose exhaustion the gateway itself reports as retryable; `client`,
/// `auth`, `model_missing` and `endpoint_not_found` all mean "this request cannot succeed as
/// sent", and retrying the identical body is futile.
///
/// The interpretation lives in [`crate::retry::is_retryable_failure`], next to the rest of
/// the retry policy; this list is the vocabulary it reads.
pub const NON_RETRYABLE_KINDS: &[&str] = &[
    "client",
    "auth",
    "model_missing",
    "endpoint_not_found",
    "client_abort",
    "invalid_base_url",
    "unknown",
    "upstream_200_non_json_body",
    "upstream_200_no_meaningful_output",
];

/// The error a stream that ended without its closing event reports. Retryable: the connection
/// was cut, the request itself was not refused.
const STREAM_TRUNCATED: &str =
    "connection closed before the provider stream reached a terminal event";

/// Error kinds a provider reports inside a stream that a later attempt can get past.
const RETRYABLE_STREAM_ERRORS: &[&str] = &[
    "overloaded_error",
    "engine_overloaded",
    "rate_limit_error",
    "rate_limit_exceeded",
    "api_error",
    "server_error",
    "service_unavailable",
    "timeout",
];

/// The failure a provider announces inside a stream that answered 200: Anthropic's `error`
/// event, a Chat Completions `{"error": ...}` chunk, a Responses `error` or failed response.
/// Ignoring it leaves a stream that just ends, and the real cause is lost.
fn stream_error(error: &Value) -> String {
    let field = |key: &str| {
        error
            .get(key)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
    };
    let kind = field("type").or_else(|| field("code")).unwrap_or("unknown");
    let message = field("message")
        .or_else(|| error.as_str())
        .unwrap_or("no message");
    if RETRYABLE_STREAM_ERRORS.contains(&kind) {
        format!("stream interrupted by the provider ({kind}): {message}")
    } else {
        format!("provider stream error ({kind}): {message} {NO_RETRY_TAG}")
    }
}

/// Send the request and map ureq errors (with body) to String.
fn send_json(req_builder: ureq::Request, body: Value) -> Result<ureq::Response, String> {
    req_builder.send_json(body).map_err(http_error)
}

/// Send the request on a helper thread so the wait for the connection and the response headers
/// stays responsive to `cancel`. `Ok(None)` means the caller cancelled first; the abandoned
/// attempt ends on its own when the connection resolves or its timeout expires.
fn send_cancellable(
    request: ureq::Request,
    body: Value,
    cancel: &AtomicBool,
) -> Result<Option<ureq::Response>, String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(send_json(request, body));
    });
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        match rx.recv_timeout(CANCEL_POLL) {
            Ok(result) => return result.map(Some),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err("transport error: request worker ended unexpectedly".to_string())
            }
        }
    }
}

/// One step of a cancel-aware line stream.
enum SseLine {
    Line(String),
    Cancelled,
    End,
}

/// Reads response lines on a helper thread so a silent stream (a model that is still thinking,
/// a gateway that stopped sending) cannot hold the caller past a cancel.
struct SseLines {
    rx: mpsc::Receiver<std::io::Result<String>>,
}

impl SseLines {
    fn new(reader: Box<dyn Read + Send + Sync + 'static>) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(reader).lines() {
                let failed = line.is_err();
                if tx.send(line).is_err() || failed {
                    break;
                }
            }
        });
        Self { rx }
    }

    fn next(&self, cancel: &AtomicBool) -> Result<SseLine, String> {
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(SseLine::Cancelled);
            }
            match self.rx.recv_timeout(CANCEL_POLL) {
                Ok(Ok(line)) => return Ok(SseLine::Line(line)),
                Ok(Err(e)) => return Err(format!("SSE read error: {e}")),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(SseLine::End),
            }
        }
    }
}

fn cancelled() -> Value {
    json!({"cancelled": true})
}

/// Merge `model_settings` into a request body.
///
/// Settings ride the same body for every protocol, so a key that means something to one protocol
/// is an unknown field to another. When the request is re-sent reduced, the optional parameters
/// this protocol allows a provider to refuse are dropped instead of resent.
fn merge_settings(body: &mut Value, req: &ProviderRequest) {
    let Some(obj) = req.settings.as_ref().and_then(Value::as_object) else {
        return;
    };
    let Some(body_obj) = body.as_object_mut() else {
        return;
    };
    let optional: &[&str] = if req.reduced {
        optional_request_params(&req.protocol)
    } else {
        &[]
    };
    for (key, value) in obj {
        if req.reduced && optional.contains(&key.as_str()) {
            continue;
        }
        body_obj.insert(key.clone(), value.clone());
    }
}

fn build_openai_chat(req: &ProviderRequest, should_stream: bool) -> Value {
    let stream = should_stream;
    let mut body = json!({
        "model": req.model,
        "messages": req.messages,
    });
    if stream {
        body["stream"] = json!(true);
        // `stream_options` is an OpenAI extension that many gateways and Ollama-compatible
        // endpoints reject outright. It only carries a usage sum, so a reduced request drops it.
        if !req.reduced {
            body["stream_options"] = json!({"include_usage": true});
        }
    }
    if let Some(tools) = &req.tools {
        body["tools"] = tools.clone();
    }
    merge_settings(&mut body, req);
    body
}

fn usage_from_json(u: &Value) -> Value {
    let prompt = u.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    let completion = u
        .get("completion_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let cached = u
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(|c| c.as_i64())
        .unwrap_or(0);
    json!({
        "input": prompt - cached,
        "output": completion,
        "cache_read": cached,
        "cache_write": 0,
    })
}

pub fn complete_openai_chat(req: &ProviderRequest) -> Result<Value, String> {
    let url = versioned_endpoint(&req.base_url, "chat/completions");
    let body = build_openai_chat(req, false);
    let resp = send_json(
        agent(req.timeout_secs, req.proxy.as_ref())
            .post(&url)
            .set("Authorization", &format!("Bearer {}", req.api_key))
            .set("Content-Type", "application/json"),
        body,
    )?;
    let resp_json: Value = resp.into_json().map_err(|e| format!("JSON parse: {e}"))?;
    parse_openai_chat(&resp_json)
}

fn parse_openai_chat(resp: &Value) -> Result<Value, String> {
    let choices = resp
        .get("choices")
        .and_then(|c| c.as_array())
        .ok_or("missing choices")?;
    let choice = choices.first().ok_or("empty choices")?;
    let message = choice.get("message").ok_or("missing message")?;
    let text = message
        .get("content")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string());
    let finish_reason = choice
        .get("finish_reason")
        .and_then(|f| f.as_str())
        .map(|s| s.to_string());
    let reasoning = message
        .get("reasoning_content")
        .or_else(|| message.get("reasoning"))
        .and_then(|r| r.as_str())
        .map(|s| s.to_string());
    let tool_calls = message
        .get("tool_calls")
        .and_then(|tc| tc.as_array())
        .map(|tc| {
            tc.iter()
                .map(|call| {
                    let id = call
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let function = call.get("function").unwrap_or(&Value::Null);
                    let name = function
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let args_str = function
                        .get("arguments")
                        .and_then(|v| v.as_str())
                        .unwrap_or("{}")
                        .to_string();
                    let arguments = serde_json::from_str(&args_str).unwrap_or(json!({}));
                    json!({"id": id, "name": name, "arguments": arguments})
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let usage = resp.get("usage").map(usage_from_json);
    Ok(
        json!({"text": text, "tool_calls": tool_calls, "finish_reason": finish_reason, "reasoning": reasoning, "usage": usage, "extras": {}}),
    )
}

// -- Streaming SSE -----------------------------------------------------------

pub fn stream_openai_chat(
    req: &ProviderRequest,
    out: &mut impl Write,
    stream_id: &str,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Value, String> {
    let url = versioned_endpoint(&req.base_url, "chat/completions");
    let body = build_openai_chat(req, true);
    let Some(resp) = send_cancellable(
        stream_agent(req.timeout_secs, &req.pinned_addrs, req.proxy.as_ref())
            .post(&url)
            .set("Authorization", &format!("Bearer {}", req.api_key))
            .set("Content-Type", "application/json"),
        body,
        cancel,
    )?
    else {
        return Ok(cancelled());
    };
    let lines = SseLines::new(resp.into_reader());
    let mut text_parts: Vec<String> = Vec::new();
    let mut reasoning_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<ToolCallAccum> = Vec::new();
    let mut finish_reason: Option<String> = None;
    let mut usage: Option<Value> = None;
    let mut has_seen_done = false;
    loop {
        let line = match lines.next(cancel)? {
            SseLine::Line(line) => line,
            SseLine::Cancelled => return Ok(cancelled()),
            SseLine::End => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(':') {
            continue;
        }
        if !trimmed.starts_with("data: ") {
            continue;
        }
        let payload = &trimmed[6..];
        if payload == "[DONE]" {
            has_seen_done = true;
            break;
        }
        let chunk: Value = match serde_json::from_str(payload) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(error) = chunk.get("error").filter(|e| !e.is_null()) {
            return Err(stream_error(error));
        }
        if let Some(u) = chunk.get("usage") {
            usage = Some(usage_from_json(u));
        }
        let choices = match chunk.get("choices").and_then(|c| c.as_array()) {
            Some(c) => c,
            None => continue,
        };
        let choice = match choices.first() {
            Some(c) => c,
            None => continue,
        };
        let delta = choice.get("delta").unwrap_or(&Value::Null);
        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
            if !content.is_empty() {
                text_parts.push(content.to_string());
                let frame = json!({"ok": true, "stream": "delta", "request_id": stream_id, "data": {"text_delta": content}});
                writeln!(out, "{frame}").ok();
                out.flush().ok();
            }
        }
        if let Some(r) = delta
            .get("reasoning_content")
            .or_else(|| delta.get("reasoning"))
            .and_then(|r| r.as_str())
        {
            if !r.is_empty() {
                reasoning_parts.push(r.to_string());
                let frame = json!({"ok": true, "stream": "delta", "request_id": stream_id, "data": {"reasoning_delta": r}});
                writeln!(out, "{frame}").ok();
                out.flush().ok();
            }
        }
        if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tcs {
                let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                while tool_calls.len() <= idx {
                    tool_calls.push(ToolCallAccum::default());
                }
                let accum = &mut tool_calls[idx];
                if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                    accum.id = id.to_string();
                }
                if let Some(func) = tc.get("function") {
                    if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                        accum.name = name.to_string();
                    }
                    if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                        accum.args.push_str(args);
                    }
                }
            }
        }
        if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
            finish_reason = Some(fr.to_string());
        }
    }
    if !has_seen_done && finish_reason.is_none() {
        return Err(STREAM_TRUNCATED.to_string());
    }
    let text = if text_parts.is_empty() {
        Value::Null
    } else {
        Value::String(text_parts.join(""))
    };
    let reasoning = if reasoning_parts.is_empty() {
        Value::Null
    } else {
        Value::String(reasoning_parts.join(""))
    };
    let tool_calls_json: Vec<Value> = tool_calls
        .iter()
        .map(|tc| {
            let arguments =
                serde_json::from_str(&tc.args).unwrap_or(json!({"_raw": tc.args.clone()}));
            json!({"id": tc.id.clone(), "name": tc.name.clone(), "arguments": arguments})
        })
        .collect();
    Ok(
        json!({"text": text, "tool_calls": tool_calls_json, "finish_reason": finish_reason, "reasoning": reasoning, "usage": usage, "extras": {}}),
    )
}

// -- Anthropic Messages (non-streaming) -------------------------------------

fn anthropic_headers(req: &ProviderRequest) -> Vec<(&str, String)> {
    vec![
        ("x-api-key", req.api_key.clone()),
        ("anthropic-version", "2023-06-01".to_string()),
        ("Content-Type", "application/json".to_string()),
    ]
}

fn build_anthropic_body(req: &ProviderRequest, should_stream: bool) -> Value {
    let stream = should_stream;
    let mut system = Vec::new();
    let mapped = req
        .messages
        .as_array()
        .map(|messages| {
            messages
                .iter()
                .filter_map(|message| {
                    if message.get("role").and_then(Value::as_str) == Some("system") {
                        if let Some(text) = message.get("content").and_then(Value::as_str) {
                            system.push(text.to_string());
                        }
                        None
                    } else if message.get("role").and_then(Value::as_str) == Some("tool") {
                        Some(json!({
                            "role": "user",
                            "content": [{
                                "type": "tool_result",
                                "tool_use_id": message.get("tool_call_id").cloned().unwrap_or(Value::Null),
                                "content": message.get("content").and_then(Value::as_str)
                                    .map(str::to_string)
                                    .unwrap_or_else(|| serde_json::to_string(message.get("content").unwrap_or(&Value::Null)).unwrap_or_default()),
                            }]
                        }))
                    } else if message.get("role").and_then(Value::as_str) == Some("assistant")
                        && message.get("tool_calls").and_then(Value::as_array).is_some_and(|calls| !calls.is_empty())
                    {
                        let mut content = Vec::new();
                        if let Some(text) = message.get("content").and_then(Value::as_str).filter(|text| !text.is_empty()) {
                            content.push(json!({"type": "text", "text": text}));
                        }
                        for call in message.get("tool_calls").and_then(Value::as_array).into_iter().flatten() {
                            let function = call.get("function").unwrap_or(&Value::Null);
                            let input = function.get("arguments").and_then(Value::as_str)
                                .and_then(|arguments| serde_json::from_str::<Value>(arguments).ok())
                                .unwrap_or_else(|| json!({}));
                            content.push(json!({
                                "type": "tool_use",
                                "id": call.get("id").cloned().unwrap_or(Value::Null),
                                "name": function.get("name").cloned().unwrap_or(Value::Null),
                                "input": input,
                            }));
                        }
                        Some(json!({"role": "assistant", "content": content}))
                    } else {
                        Some(message.clone())
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut messages: Vec<Value> = Vec::new();
    for message in mapped {
        let same_role = messages.last().is_some_and(|previous| {
            previous.get("role") == message.get("role")
                && previous.get("content").is_some_and(Value::is_array)
                && message.get("content").is_some_and(Value::is_array)
        });
        if same_role {
            let extra = message
                .get("content")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if let Some(content) = messages
                .last_mut()
                .and_then(|previous| previous.get_mut("content"))
                .and_then(Value::as_array_mut)
            {
                content.extend(extra);
            }
        } else {
            messages.push(message);
        }
    }
    let mut body = json!({
        "model": req.model,
        "messages": messages,
        "max_tokens": 4096,
    });
    if !system.is_empty() {
        body["system"] = Value::String(system.join("\n\n"));
    }
    if stream {
        body["stream"] = json!(true);
    }
    merge_settings(&mut body, req);
    if let Some(tools) = &req.tools {
        body["tools"] = Value::Array(
            tools
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|tool| tool.get("function"))
                .map(|function| json!({
                    "name": function.get("name").cloned().unwrap_or(Value::Null),
                    "description": function.get("description").cloned().unwrap_or(Value::Null),
                    "input_schema": function.get("parameters").cloned().unwrap_or_else(|| json!({"type": "object"})),
                }))
                .collect(),
        );
    }
    body
}

fn anthropic_usage(u: &Value) -> Value {
    let input = u.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    let output = u.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    let cache_read = u
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let cache_write = u
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    json!({
        "input": input,
        "output": output,
        "cache_read": cache_read,
        "cache_write": cache_write,
    })
}

fn map_stop_reason(reason: &str) -> &str {
    match reason {
        "end_turn" => "stop",
        "tool_use" => "tool_calls",
        "max_tokens" => "length",
        "stop_sequence" => "stop",
        _ => reason,
    }
}

pub fn complete_anthropic(req: &ProviderRequest) -> Result<Value, String> {
    let url = versioned_endpoint(&req.base_url, "messages");
    let body = build_anthropic_body(req, false);
    let mut request = agent(req.timeout_secs, req.proxy.as_ref()).post(&url);
    for (k, v) in anthropic_headers(req) {
        request = request.set(k, &v);
    }
    let resp = send_json(request, body)?;
    let resp_json: Value = resp.into_json().map_err(|e| format!("JSON parse: {e}"))?;
    parse_anthropic_response(&resp_json)
}

fn parse_anthropic_response(resp: &Value) -> Result<Value, String> {
    let content = resp
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or("missing content array")?;
    let mut text_parts: Vec<String> = Vec::new();
    let mut reasoning_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<Value> = Vec::new();
    for block in content {
        match block.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                    text_parts.push(t.to_string());
                }
            }
            Some("tool_use") => {
                let id = block
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let input = block.get("input").cloned().unwrap_or(json!({}));
                tool_calls.push(json!({"id": id, "name": name, "arguments": input}));
            }
            Some("thinking") => {
                if let Some(t) = block.get("thinking").and_then(|t| t.as_str()) {
                    reasoning_parts.push(t.to_string());
                }
            }
            Some("redacted_thinking") => {
                reasoning_parts.push("[redacted]".to_string());
            }
            _ => {}
        }
    }
    let stop_reason = resp
        .get("stop_reason")
        .and_then(|s| s.as_str())
        .map(|s| map_stop_reason(s).to_string());
    let usage = resp.get("usage").map(anthropic_usage);
    let text = if text_parts.is_empty() {
        Value::Null
    } else {
        Value::String(text_parts.join(""))
    };
    let reasoning = if reasoning_parts.is_empty() {
        Value::Null
    } else {
        Value::String(reasoning_parts.join(""))
    };
    Ok(json!({
        "text": text,
        "tool_calls": tool_calls,
        "finish_reason": stop_reason,
        "reasoning": reasoning,
        "usage": usage,
        "extras": {},
    }))
}

// -- Anthropic Messages (streaming SSE) -------------------------------------

pub fn stream_anthropic(
    req: &ProviderRequest,
    out: &mut impl Write,
    stream_id: &str,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Value, String> {
    let url = versioned_endpoint(&req.base_url, "messages");
    let body = build_anthropic_body(req, true);
    let mut request =
        stream_agent(req.timeout_secs, &req.pinned_addrs, req.proxy.as_ref()).post(&url);
    for (k, v) in anthropic_headers(req) {
        request = request.set(k, &v);
    }
    let Some(resp) = send_cancellable(request, body, cancel)? else {
        return Ok(cancelled());
    };
    let lines = SseLines::new(resp.into_reader());
    let mut text_parts: Vec<String> = Vec::new();
    let mut reasoning_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<ToolCallAccum> = Vec::new();
    let mut block_types: Vec<String> = Vec::new();
    let mut finish_reason: Option<String> = None;
    let mut usage: Option<Value> = None;
    let mut has_seen_stop = false;
    loop {
        let line = match lines.next(cancel)? {
            SseLine::Line(line) => line,
            SseLine::Cancelled => return Ok(cancelled()),
            SseLine::End => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(':') {
            continue;
        }
        if trimmed.starts_with("event:") {
            continue;
        }
        if !trimmed.starts_with("data: ") {
            continue;
        }
        let payload = &trimmed[6..];
        let chunk: Value = match serde_json::from_str(payload) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let chunk_type = chunk.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match chunk_type {
            "error" => {
                return Err(stream_error(chunk.get("error").unwrap_or(&chunk)));
            }
            "message_start" => {
                if let Some(msg) = chunk.get("message") {
                    if let Some(u) = msg.get("usage") {
                        usage = Some(anthropic_usage(u));
                    }
                }
            }
            "content_block_start" => {
                let idx = chunk.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                while block_types.len() <= idx {
                    block_types.push(String::new());
                    tool_calls.push(ToolCallAccum::default());
                }
                if let Some(block) = chunk.get("content_block") {
                    let bt = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    block_types[idx] = bt.to_string();
                    if bt == "tool_use" {
                        if let Some(id) = block.get("id").and_then(|v| v.as_str()) {
                            tool_calls[idx].id = id.to_string();
                        }
                        if let Some(name) = block.get("name").and_then(|v| v.as_str()) {
                            tool_calls[idx].name = name.to_string();
                        }
                    }
                }
            }
            "content_block_delta" => {
                let idx = chunk.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                if let Some(delta) = chunk.get("delta") {
                    let dt = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match dt {
                        "text_delta" => {
                            if let Some(t) = delta.get("text").and_then(|t| t.as_str()) {
                                if !t.is_empty() {
                                    text_parts.push(t.to_string());
                                    let frame = json!({"ok": true, "stream": "delta", "request_id": stream_id, "data": {"text_delta": t}});
                                    writeln!(out, "{frame}").ok();
                                    out.flush().ok();
                                }
                            }
                        }
                        "thinking_delta" => {
                            if let Some(t) = delta.get("thinking").and_then(|t| t.as_str()) {
                                if !t.is_empty() {
                                    reasoning_parts.push(t.to_string());
                                    let frame = json!({"ok": true, "stream": "delta", "request_id": stream_id, "data": {"reasoning_delta": t}});
                                    writeln!(out, "{frame}").ok();
                                    out.flush().ok();
                                }
                            }
                        }
                        "input_json_delta" => {
                            if let Some(pj) = delta.get("partial_json").and_then(|v| v.as_str()) {
                                if idx < tool_calls.len() {
                                    tool_calls[idx].args.push_str(pj);
                                }
                            }
                        }
                        "signature_delta" => {
                            // Signature deltas are not streamed to UI;
                            // they're part of the thinking block's signature.
                        }
                        _ => {}
                    }
                }
            }
            "message_delta" => {
                if let Some(d) = chunk.get("delta") {
                    if let Some(sr) = d.get("stop_reason").and_then(|s| s.as_str()) {
                        finish_reason = Some(map_stop_reason(sr).to_string());
                    }
                }
                if let Some(u) = chunk.get("usage") {
                    let prev = usage.clone().unwrap_or(json!({}));
                    let mut merged = prev;
                    if let Some(out_tok) = u.get("output_tokens").and_then(|v| v.as_i64()) {
                        if let Some(obj) = merged.as_object_mut() {
                            obj.insert("output".to_string(), json!(out_tok));
                        }
                    }
                    usage = Some(merged);
                }
            }
            "message_stop" => {
                has_seen_stop = true;
                break;
            }
            _ => {}
        }
    }
    if !has_seen_stop && finish_reason.is_none() {
        return Err(STREAM_TRUNCATED.to_string());
    }
    let text = if text_parts.is_empty() {
        Value::Null
    } else {
        Value::String(text_parts.join(""))
    };
    let reasoning = if reasoning_parts.is_empty() {
        Value::Null
    } else {
        Value::String(reasoning_parts.join(""))
    };
    let tool_calls_json: Vec<Value> = tool_calls
        .iter()
        .filter(|tc| !tc.name.is_empty())
        .map(|tc| {
            let arguments =
                serde_json::from_str(&tc.args).unwrap_or(json!({"_raw": tc.args.clone()}));
            json!({"id": tc.id.clone(), "name": tc.name.clone(), "arguments": arguments})
        })
        .collect();
    Ok(json!({
        "text": text,
        "tool_calls": tool_calls_json,
        "finish_reason": finish_reason,
        "reasoning": reasoning,
        "usage": usage,
        "extras": {},
    }))
}

// -- OpenAI Responses (non-streaming) ---------------------------------------

fn build_openai_responses(req: &ProviderRequest, should_stream: bool) -> Value {
    let stream = should_stream;
    let mut body = json!({
        "model": req.model,
        "input": req.messages,
        "store": false,
        "include": ["reasoning.encrypted_content"],
        "reasoning": {"summary": "auto"},
    });
    if stream {
        body["stream"] = json!(true);
    }
    if let Some(tools) = &req.tools {
        body["tools"] = Value::Array(
            tools
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|tool| tool.get("function"))
                .map(|function| json!({
                    "type": "function",
                    "name": function.get("name").cloned().unwrap_or(Value::Null),
                    "description": function.get("description").cloned().unwrap_or(Value::Null),
                    "parameters": function.get("parameters").cloned().unwrap_or_else(|| json!({"type": "object"})),
                    "strict": false,
                }))
                .collect(),
        );
    }
    merge_settings(&mut body, req);
    if let Some(tools) = &req.tools {
        body["tools"] = Value::Array(
            tools
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|tool| tool.get("function"))
                .map(|function| json!({
                    "type": "function",
                    "name": function.get("name").cloned().unwrap_or(Value::Null),
                    "description": function.get("description").cloned().unwrap_or(Value::Null),
                    "parameters": function.get("parameters").cloned().unwrap_or_else(|| json!({"type": "object"})),
                    "strict": false,
                }))
                .collect(),
        );
    }
    body
}

pub fn complete_openai_responses(req: &ProviderRequest) -> Result<Value, String> {
    let url = versioned_endpoint(&req.base_url, "responses");
    let body = build_openai_responses(req, false);
    let resp = send_json(
        agent(req.timeout_secs, req.proxy.as_ref())
            .post(&url)
            .set("Authorization", &format!("Bearer {}", req.api_key))
            .set("Content-Type", "application/json"),
        body,
    )?;
    let resp_json: Value = resp.into_json().map_err(|e| format!("JSON parse: {e}"))?;
    parse_openai_responses(&resp_json)
}

fn parse_openai_responses(resp: &Value) -> Result<Value, String> {
    if let Some(error) = resp.get("error").filter(|e| !e.is_null()) {
        return Err(stream_error(error));
    }
    if resp.get("status").and_then(Value::as_str) == Some("failed") {
        return Err(stream_error(&json!({"message": "the response failed"})));
    }
    let items = resp
        .get("output")
        .and_then(|o| o.as_array())
        .cloned()
        .unwrap_or_default();
    let mut texts: Vec<String> = Vec::new();
    let mut summaries: Vec<String> = Vec::new();
    let mut tool_calls: Vec<Value> = Vec::new();
    for item in &items {
        let kind = item.get("type").and_then(|t| t.as_str());
        match kind {
            Some("message") => {
                if let Some(content) = item.get("content") {
                    if let Some(s) = content.as_str() {
                        texts.push(s.to_string());
                    } else if let Some(parts) = content.as_array() {
                        for part in parts {
                            if part.get("type").and_then(|t| t.as_str()) == Some("output_text") {
                                if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                                    texts.push(t.to_string());
                                }
                            }
                        }
                    }
                }
            }
            Some("reasoning") => {
                if let Some(summary) = item.get("summary") {
                    if let Some(parts) = summary.as_array() {
                        for part in parts {
                            let text = if let Some(s) = part.as_str() {
                                s.to_string()
                            } else if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                                t.to_string()
                            } else {
                                continue;
                            };
                            if !text.is_empty() {
                                summaries.push(text);
                            }
                        }
                    }
                }
            }
            Some("function_call") => {
                let id = item
                    .get("call_id")
                    .or_else(|| item.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let arguments = item.get("arguments").cloned().unwrap_or(json!({}));
                tool_calls.push(json!({"id": id, "name": name, "arguments": arguments}));
            }
            _ => {}
        }
    }
    let incomplete = resp.get("incomplete_details").cloned().unwrap_or(json!({}));
    let finish_reason = if !tool_calls.is_empty() {
        "tool_calls".to_string()
    } else if incomplete.get("reason").and_then(|r| r.as_str()) == Some("max_output_tokens") {
        "length".to_string()
    } else {
        "stop".to_string()
    };
    let usage = resp.get("usage").map(get_openai_usage);
    let text = if texts.is_empty() {
        Value::Null
    } else {
        Value::String(texts.join(""))
    };
    let reasoning = if summaries.is_empty() {
        Value::Null
    } else {
        Value::String(summaries.join(""))
    };
    Ok(json!({
        "text": text,
        "tool_calls": tool_calls,
        "finish_reason": finish_reason,
        "reasoning": reasoning,
        "usage": usage,
        "extras": {},
    }))
}

fn get_openai_usage(u: &Value) -> Value {
    let input = u.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    let output = u.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    json!({
        "input": input,
        "output": output,
        "cache_read": 0,
        "cache_write": 0,
    })
}

// -- OpenAI Responses (streaming SSE) ---------------------------------------

pub fn stream_openai_responses(
    req: &ProviderRequest,
    out: &mut impl Write,
    stream_id: &str,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Value, String> {
    let url = versioned_endpoint(&req.base_url, "responses");
    let body = build_openai_responses(req, true);
    let Some(resp) = send_cancellable(
        stream_agent(req.timeout_secs, &req.pinned_addrs, req.proxy.as_ref())
            .post(&url)
            .set("Authorization", &format!("Bearer {}", req.api_key))
            .set("Content-Type", "application/json"),
        body,
        cancel,
    )?
    else {
        return Ok(cancelled());
    };
    let lines = SseLines::new(resp.into_reader());
    let mut text_parts: Vec<String> = Vec::new();
    let mut reasoning_parts: Vec<String> = Vec::new();
    let mut final_response: Option<Value> = None;
    let mut event_type: Option<String> = None;
    loop {
        let line = match lines.next(cancel)? {
            SseLine::Line(line) => line,
            SseLine::Cancelled => return Ok(cancelled()),
            SseLine::End => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("event: ") {
            event_type = Some(rest.trim().to_string());
            continue;
        }
        if !trimmed.starts_with("data: ") {
            continue;
        }
        let payload = &trimmed[6..];
        let chunk: Value = match serde_json::from_str(payload) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let kind = event_type.clone().unwrap_or_else(|| {
            chunk
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string()
        });
        match kind.as_str() {
            "response.output_text.delta" => {
                if let Some(t) = chunk.get("delta").and_then(|d| d.as_str()) {
                    if !t.is_empty() {
                        text_parts.push(t.to_string());
                        let frame = json!({"ok": true, "stream": "delta", "request_id": stream_id, "data": {"text_delta": t}});
                        writeln!(out, "{frame}").ok();
                        out.flush().ok();
                    }
                }
            }
            "response.reasoning_summary_text.delta" => {
                if let Some(t) = chunk.get("delta").and_then(|d| d.as_str()) {
                    if !t.is_empty() {
                        reasoning_parts.push(t.to_string());
                        let frame = json!({"ok": true, "stream": "delta", "request_id": stream_id, "data": {"reasoning_delta": t}});
                        writeln!(out, "{frame}").ok();
                        out.flush().ok();
                    }
                }
            }
            "error" => {
                return Err(stream_error(chunk.get("error").unwrap_or(&chunk)));
            }
            "response.completed" | "response.incomplete" | "response.failed" => {
                if let Some(r) = chunk.get("response") {
                    final_response = Some(r.clone());
                }
            }
            _ => {}
        }
    }
    // Without a closing response the text so far is a cut-off answer, not a finished one.
    let final_resp = final_response.ok_or_else(|| STREAM_TRUNCATED.to_string())?;
    parse_openai_responses(&final_resp)
}

// -- Dispatch ---------------------------------------------------------------

pub fn complete(req: &ProviderRequest) -> Result<Value, String> {
    match req.protocol.as_str() {
        "openai_chat" => complete_openai_chat(req),
        "anthropic" => complete_anthropic(req),
        "openai_responses" => complete_openai_responses(req),
        _ => Err(format!(
            "unknown protocol: {} (supports openai_chat, anthropic, openai_responses)",
            req.protocol
        )),
    }
}

pub fn stream(
    req: &ProviderRequest,
    out: &mut impl Write,
    stream_id: &str,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Value, String> {
    match req.protocol.as_str() {
        "openai_chat" => stream_openai_chat(req, out, stream_id, cancel),
        "anthropic" => stream_anthropic(req, out, stream_id, cancel),
        "openai_responses" => stream_openai_responses(req, out, stream_id, cancel),
        _ => Err(format!(
            "unknown protocol: {} (supports openai_chat, anthropic, openai_responses)",
            req.protocol
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_anthropic_response, parse_openai_chat, parse_openai_responses, stream,
        versioned_endpoint, ProviderRequest,
    };
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::AtomicBool;

    /// Serve one SSE reply on a local port, then close the connection, and return the base URL.
    fn serve_sse(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut seen = Vec::new();
            let mut chunk = [0u8; 4096];
            // Read the request head and its body so closing the socket does not reset the reply.
            while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = conn.read(&mut chunk).unwrap();
                if n == 0 {
                    return;
                }
                seen.extend_from_slice(&chunk[..n]);
            }
            let head_end = seen.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
            let head = String::from_utf8_lossy(&seen[..head_end]).to_lowercase();
            let length: usize = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
            let mut have = seen.len() - head_end;
            while have < length {
                let n = conn.read(&mut chunk).unwrap();
                if n == 0 {
                    break;
                }
                have += n;
            }
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{body}"
            );
            conn.write_all(reply.as_bytes()).unwrap();
        });
        format!("http://127.0.0.1:{port}")
    }

    fn streamed(protocol: &str, body: &'static str) -> Result<serde_json::Value, String> {
        let request = ProviderRequest {
            protocol: protocol.to_string(),
            model: "m".to_string(),
            messages: json!([{"role": "user", "content": "hi"}]),
            tools: None,
            settings: None,
            api_key: "k".to_string(),
            base_url: serve_sse(body),
            timeout_secs: Some(5.0),
            reduced: false,
            pinned_addrs: Vec::new(),
            proxy: None,
        };
        stream(&request, &mut Vec::new(), "id", &AtomicBool::new(false))
    }

    const ANTHROPIC_CUT: &str = "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":1}}}\n\n\
        data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\"}}\n\n\
        data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"half an ans\"}}\n\n";

    #[test]
    fn an_anthropic_stream_cut_before_its_end_is_an_error_not_an_answer() {
        let error = streamed("anthropic", ANTHROPIC_CUT).unwrap_err();
        assert!(crate::retry::is_retryable_failure(&error), "{error}");
    }

    #[test]
    fn an_anthropic_stream_that_ends_properly_is_an_answer() {
        let done = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\"}}\n\n\
            data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\n\
            data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n\
            data: {\"type\":\"message_stop\"}\n\n";
        assert_eq!(streamed("anthropic", done).unwrap()["text"], "ok");
    }

    #[test]
    fn an_anthropic_error_event_reports_its_cause_and_whether_to_retry() {
        let overloaded = "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n";
        let error = streamed("anthropic", overloaded).unwrap_err();
        assert!(error.contains("overloaded_error") && error.contains("Overloaded"));
        assert!(crate::retry::is_retryable_failure(&error), "{error}");

        let refused = "data: {\"type\":\"error\",\"error\":{\"type\":\"invalid_request_error\",\"message\":\"prompt is too long\"}}\n\n";
        let error = streamed("anthropic", refused).unwrap_err();
        assert!(error.contains("prompt is too long"));
        assert!(!crate::retry::is_retryable_failure(&error), "{error}");
    }

    #[test]
    fn an_openai_chat_error_chunk_reports_its_cause() {
        let body = "data: {\"error\":{\"type\":\"invalid_request_error\",\"code\":\"context_length_exceeded\",\"message\":\"too long\"}}\n\n";
        let error = streamed("openai_chat", body).unwrap_err();
        assert!(error.contains("too long"), "{error}");
        assert!(!crate::retry::is_retryable_failure(&error), "{error}");
    }

    #[test]
    fn a_responses_stream_without_a_closing_response_is_an_error() {
        let body = "event: response.output_text.delta\ndata: {\"delta\":\"half\"}\n\n";
        let error = streamed("openai_responses", body).unwrap_err();
        assert!(crate::retry::is_retryable_failure(&error), "{error}");
    }

    #[test]
    fn a_failed_response_is_an_error_with_the_providers_reason() {
        let body = "event: response.failed\ndata: {\"response\":{\"status\":\"failed\",\"error\":{\"code\":\"server_error\",\"message\":\"upstream broke\"},\"output\":[]}}\n\n";
        let error = streamed("openai_responses", body).unwrap_err();
        assert!(error.contains("upstream broke"), "{error}");
        assert!(crate::retry::is_retryable_failure(&error), "{error}");
    }

    #[test]
    fn a_responses_stream_with_its_closing_response_is_an_answer() {
        let body = "event: response.completed\ndata: {\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"fine\"}]}]}}\n\n";
        assert_eq!(streamed("openai_responses", body).unwrap()["text"], "fine");
    }

    #[test]
    fn versioned_endpoint_never_duplicates_a_version_segment() {
        // A bare host gains the version the protocol expects.
        assert_eq!(
            versioned_endpoint("https://api.example.test", "responses"),
            "https://api.example.test/v1/responses"
        );
        assert_eq!(
            versioned_endpoint("https://api.example.test/v1", "responses"),
            "https://api.example.test/v1/responses"
        );
        // A base URL that already names the endpoint is left whole, not spliced again. This is
        // the case that produced `…/v1/messages/v1/messages`.
        assert_eq!(
            versioned_endpoint("https://gw.example.test/v1/messages", "messages"),
            "https://gw.example.test/v1/messages"
        );
        assert_eq!(
            versioned_endpoint(
                "https://gw.example.test/v1/chat/completions",
                "chat/completions"
            ),
            "https://gw.example.test/v1/chat/completions"
        );
        // A different version segment is kept as given, not rewritten to /v1.
        assert_eq!(
            versioned_endpoint("https://gw.example.test/api/v2", "messages"),
            "https://gw.example.test/api/v2/messages"
        );
        // Trailing slashes never matter.
        assert_eq!(
            versioned_endpoint("https://api.example.test/v1/", "messages"),
            "https://api.example.test/v1/messages"
        );
        // Chat Completions resolves through the same function the other protocols use.
        assert_eq!(
            versioned_endpoint("https://gw.example.test", "chat/completions"),
            "https://gw.example.test/v1/chat/completions"
        );
    }

    #[test]
    fn openai_chat_parser_preserves_reasoning_usage_and_tools() {
        let parsed = parse_openai_chat(&json!({
            "choices": [{
                "message": {
                    "content": "Hello",
                    "reasoning_content": "thinking",
                    "tool_calls": [{
                        "id": "call_1",
                        "function": {"name": "lookup", "arguments": "{\"q\":\"delta\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "prompt_tokens_details": {"cached_tokens": 2}
            }
        }))
        .unwrap();

        assert_eq!(parsed["text"], "Hello");
        assert_eq!(parsed["reasoning"], "thinking");
        assert_eq!(parsed["finish_reason"], "tool_calls");
        assert_eq!(parsed["tool_calls"][0]["name"], "lookup");
        assert_eq!(parsed["tool_calls"][0]["arguments"]["q"], "delta");
        assert_eq!(parsed["usage"]["input"], 8);
        assert_eq!(parsed["usage"]["cache_read"], 2);
        assert_eq!(parsed["usage"]["output"], 5);
    }

    #[test]
    fn anthropic_parser_preserves_reasoning_usage_and_tools() {
        let parsed = parse_anthropic_response(&json!({
            "content": [
                {"type": "thinking", "thinking": "reason"},
                {"type": "text", "text": "Hello"},
                {"type": "tool_use", "id": "tool_1", "name": "lookup", "input": {"q": "delta"}}
            ],
            "stop_reason": "tool_use",
            "usage": {
                "input_tokens": 50,
                "output_tokens": 20,
                "cache_creation_input_tokens": 5,
                "cache_read_input_tokens": 3
            }
        }))
        .unwrap();

        assert_eq!(parsed["text"], "Hello");
        assert_eq!(parsed["reasoning"], "reason");
        assert_eq!(parsed["finish_reason"], "tool_calls");
        assert_eq!(parsed["tool_calls"][0]["arguments"]["q"], "delta");
        assert_eq!(parsed["usage"]["input"], 50);
        assert_eq!(parsed["usage"]["output"], 20);
        assert_eq!(parsed["usage"]["cache_read"], 3);
        assert_eq!(parsed["usage"]["cache_write"], 5);
    }

    #[test]
    fn openai_responses_parser_preserves_reasoning_usage_and_tools() {
        let parsed = parse_openai_responses(&json!({
            "output": [
                {"type": "message", "content": [{"type": "output_text", "text": "Hello"}]},
                {"type": "reasoning", "summary": [{"text": "reason"}]},
                {"type": "function_call", "id": "call_1", "name": "lookup", "arguments": {"q": "delta"}}
            ],
            "incomplete_details": null,
            "usage": {"input_tokens": 30, "output_tokens": 10}
        }))
        .unwrap();

        assert_eq!(parsed["text"], "Hello");
        assert_eq!(parsed["reasoning"], "reason");
        assert_eq!(parsed["finish_reason"], "tool_calls");
        assert_eq!(parsed["tool_calls"][0]["arguments"]["q"], "delta");
        assert_eq!(parsed["usage"]["input"], 30);
        assert_eq!(parsed["usage"]["output"], 10);
    }
}
