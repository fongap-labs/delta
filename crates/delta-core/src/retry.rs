//! Retry Policy Decision Authority (ADR-039).
//!
//! Whether a provider failure is worth retrying is a Rust-authoritative
//! **decision**, consumed by the runtime's retry loop: the loop owns the
//! execution (backoff delay math, the budget), this module owns the verdict.
//!
//! The verdict is layered. The endpoint's own signal outranks the status code,
//! and a request-local failure family outranks both — see
//! [`is_retryable_failure`].

// Protocol-incompatible markers in the error body — a server that says any of these
// about a param we sent speaks a dialect we mis-assumed (NOT a transport blip).
const PROTOCOL_MARKERS: &[&str] = &[
    "is not supported",
    "not supported",
    "unknown parameter",
    "unexpected parameter",
    "unexpected field",
    "does not support",
    "invalid parameter",
    "unsupported parameter",
    "not recognized",
    "'stream_options'",
    "'max_tokens'",
    "'parallel_tool_calls'",
    "'reasoning_effort'",
];

/// Decide whether a provider failure is worth retrying, from the error the transport produced.
///
/// This is the single authority the runtime consults (ADR-039). It reads, in order:
///
/// 1. **The gateway's own verdict.** `provider::http_error` appends
///    [`crate::provider::NO_RETRY_TAG`] when the endpoint answers `x-should-retry: false`.
///    `fongap-labs/ai-gateway` sends exactly that header on every terminal error except 429 and
///    503 (`protocol/http.ts::shouldNotRetryHeaders`), so a tagged error is one the endpoint has
///    already weighed and refused.
/// 2. **The failure family.** When the body carries a `failure_kinds` payload, the kinds say
///    whether retrying the same request can help. `client` means the upstream answered with a
///    request-local 4xx — a body it will not accept, however often it is sent.
/// 3. **The status code**, as before, for endpoints that offer no signal at all.
///
/// The order matters. A gateway that wraps a rejected request in a 502 used to look like a
/// transient upstream fault here, so the runtime resent the identical body four times and, on a
/// gateway that retries internally, multiplied that into fifteen pointless upstream attempts.
pub fn is_retryable_failure(message: &str) -> bool {
    // 1. The endpoint said not to.
    if message.contains(crate::provider::NO_RETRY_TAG) {
        return false;
    }
    // 2. The failure families say every attempt was non-retryable.
    if failures_block_retry(message) {
        return false;
    }
    // 3. Fall back to the status code for endpoints that say nothing.
    is_retryable_text(message)
}

/// True when a gateway `failure_kinds` payload names only non-retryable families.
fn failures_block_retry(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    let Some(start) = lower.find("\"failure_kinds\"") else {
        return false;
    };
    let payload = &lower[start..];
    let mentioned = payload
        .split('"')
        .skip(1)
        .step_by(2)
        .any(|key| crate::provider::NON_RETRYABLE_KINDS.contains(&key));
    let retryable = [
        "rate_limit",
        "server",
        "network",
        "timeout",
        "stream_interrupted",
    ]
    .iter()
    .any(|kind| payload.contains(kind));
    mentioned && !retryable
}

/// Status-code and text matching, for an error that carries no endpoint verdict.
///
/// Kept separate from [`is_retryable_failure`] so the two policies can be reasoned about —
/// and tested — independently.
pub fn is_retryable_text(message: &str) -> bool {
    // A reply with a status is judged by the status alone, so a body that merely mentions a
    // number ("HTTP 400: ... 500 tokens") is not mistaken for a server failure.
    if let Some(rest) = message.strip_prefix("HTTP ") {
        let code: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(code) = code.parse::<u16>() {
            return matches!(
                code,
                408 | 425 | 429 | 500 | 502 | 503 | 504 | 520..=524 | 529
            );
        }
    }
    let lower = message.to_lowercase();
    lower.contains("429")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("504")
        || lower.contains("connection")
        || lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("transport")
}

/// The marker set that means "a parameter this provider does not accept", which supports the
/// runtime's one-shot reduced re-send rather than a blind retry.
pub fn looks_protocol_rejection(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    PROTOCOL_MARKERS.iter().any(|m| lower.contains(m))
        // A gateway that reports a request-local failure family is telling us the body, not the
        // path, is what it objects to.
        || lower.contains("\"client\"")
        || lower.contains("all attempted nodes failed")
        || lower.contains(crate::provider::NO_RETRY_TAG)
}

/// A provider that cannot take tool definitions says so about its `tools` / `functions` field.
/// Only that reading counts: a generic 400 is not evidence that the model lacks tool calling.
pub fn looks_tools_rejection(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    let names_tools = lower.contains("tool") || lower.contains("function");
    let refuses = [
        "not support",
        "unsupported",
        "does not support",
        "not enabled",
        "not available",
        "unknown parameter",
        "unrecognized",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    names_tools && refuses
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_flagging_no_retry_outranks_the_status_code() {
        // The 502 wraps a rejected request, and the gateway said so. The old rule looked only at
        // the status code and retried this four times.
        let message = format!(
            "HTTP 502: {{\"error\":{{\"message\":\"All attempted nodes failed for model \\\"Max\\\".\"}}}} {}",
            crate::provider::NO_RETRY_TAG
        );
        assert!(!is_retryable_failure(&message));
    }

    #[test]
    fn failure_kinds_of_client_only_is_not_retried() {
        let message = "HTTP 502: {\"error\":{\"message\":\"All attempted nodes failed.\",\"details\":{\"failure_kinds\":{\"client\":3}}}}";
        assert!(!is_retryable_failure(message));
    }

    #[test]
    fn failure_kinds_with_a_retryable_family_stays_retryable() {
        let message =
            "HTTP 502: {\"error\":{\"details\":{\"failure_kinds\":{\"server\":2,\"network\":1}}}}";
        assert!(is_retryable_failure(message));
    }

    #[test]
    fn a_plain_502_with_no_signal_is_still_retried() {
        assert!(is_retryable_failure("HTTP 502: bad gateway"));
    }

    #[test]
    fn an_auth_failure_is_never_retried() {
        assert!(!is_retryable_failure("HTTP 401: invalid api key"));
    }

    #[test]
    fn a_reply_with_a_status_is_judged_by_the_status_alone() {
        // The body mentions 500 as a token count, not a status — not a server failure.
        assert!(!is_retryable_text("HTTP 400: ... 500 tokens"));
        assert!(is_retryable_text("HTTP 503: service unavailable"));
        assert!(is_retryable_text("HTTP 429: rate limit"));
        assert!(is_retryable_text("connection refused"));
        assert!(!is_retryable_text("something weird happened"));
    }

    #[test]
    fn protocol_rejection_markers_drive_the_reduced_resend() {
        assert!(looks_protocol_rejection(
            "HTTP 400: 'reasoning_effort' is not supported"
        ));
        assert!(looks_protocol_rejection(
            "HTTP 502: All attempted nodes failed for model \"Max\"."
        ));
        assert!(looks_protocol_rejection(
            "HTTP 400: unknown parameter: stream_options"
        ));
        // A genuine transient fault must not be mistaken for a parameter problem, or the runtime
        // would strip parameters it did not need to strip.
        assert!(!looks_protocol_rejection("HTTP 503: service unavailable"));
        assert!(!looks_protocol_rejection("connection refused"));
    }

    #[test]
    fn a_refusal_of_tool_definitions_is_recognised_but_a_generic_400_is_not() {
        assert!(looks_tools_rejection(
            "HTTP 400: model does not support tools"
        ));
        assert!(looks_tools_rejection(
            "{\"error\":\"Function calling is not enabled for this model\"}"
        ));
        // A gateway failure is not a verdict on the model's tool support.
        assert!(!looks_tools_rejection(
            "Error: HTTP 504: All attempted nodes failed for model \"Air\"."
        ));
        assert!(!looks_tools_rejection("HTTP 400: invalid temperature"));
    }
}
