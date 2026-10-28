"""SensitiveDataSanitizer — one recursive scrubbing policy, shared everywhere.

Audit rows, Run Event Ledger payloads, and any future log sinks must agree on what
"no secrets in persisted data" means. This module is that single definition:

- secret-shaped KEYS (token/password/api_key/…) are fully redacted, at any nesting
  depth, in dicts and lists;
- credential-bearing HTTP headers (Authorization, Cookie, …) are redacted by name,
  whatever dict they ride in;
- URL query parameters that commonly carry credentials are stripped inside string
  values (`https://host/x?token=t` → `https://host/x?token=[redacted]`);
- body-ish keys (body/content/html) are redacted wholesale: free text is where
  secrets hide without ever announcing themselves;
- free text that is not under a body key (a shell command, a tool result, an error message) is
  scrubbed for credentials that announce themselves: `Authorization:`/`Cookie:` headers,
  `--password x` style flags, `?token=x` query parameters and well-known token shapes
  (`sk-…`, `ghp_…`, `AKIA…`, JWTs). The Rust runtime applies the same policy
  (`crates/delta-core/src/redact.rs`); `crates/delta-core/tests/fixtures/redaction_golden.json`
  is the shared definition both sides are tested against.

Truncation and preview shaping are presentation concerns and stay with the callers
(e.g. audit's result previews); this module only decides WHAT must not persist.
"""

from __future__ import annotations

import re
from typing import Any
from urllib.parse import parse_qsl, quote, urlsplit, urlunsplit

SECRET_KEY_MARKERS = (
    "token",
    "secret",
    "password",
    "api_key",
    "apikey",
    "access_token",
    "bot_token",
    "app_token",
    "refresh_token",
    "credential",
    "private_key",
    "raw",
)

SENSITIVE_HEADERS = frozenset(
    {
        "authorization",
        "proxy-authorization",
        "cookie",
        "set-cookie",
        "x-api-key",
    }
)

BODY_KEYS = ("body", "content", "html")

_URL_CRED_PARAMS = frozenset(
    {
        "token",
        "access_token",
        "refresh_token",
        "api_key",
        "apikey",
        "key",
        "secret",
        "password",
        "credential",
        "sig",
        "signature",
        "auth",
    }
)

_REDACTED = "[redacted]"
_REDACTED_BODY = "[redacted body]"
_REDACTED_INPUT = "[redacted input]"


def is_secret_key(key: Any) -> bool:
    """True when a mapping key names something secret-shaped."""
    lk = str(key).lower()
    return (
        any(marker in lk for marker in SECRET_KEY_MARKERS)
        or lk in SENSITIVE_HEADERS
        # header dicts nest values under lowercase names; also catch
        # "headers.authorization"-style paths and list items like "headers[cookie]"
        or any(lk.endswith(h) or lk.endswith("[" + h + "]") for h in SENSITIVE_HEADERS)
    )


def is_body_key(key: Any) -> bool:
    lk = str(key).lower()
    return any(b == lk or lk.endswith("_" + b) for b in BODY_KEYS)


def redact_url_credentials(value: str) -> str:
    """Strip credential-bearing query parameters from an http(s) URL string.

    Only well-formed absolute http(s) URLs are rewritten; anything else passes
    through unchanged (deterministic — no guessing about non-URL strings).
    """
    try:
        parts = urlsplit(value)
    except ValueError:
        return value
    if parts.scheme not in ("http", "https") or not parts.netloc:
        return value
    try:
        pairs = parse_qsl(parts.query, keep_blank_values=True)
    except ValueError:
        return value
    if not pairs:
        return value
    scrubbed = [
        (k, _REDACTED if k.lower() in _URL_CRED_PARAMS else v) for k, v in pairs
    ]
    if scrubbed == pairs:
        return value
    query = "&".join(
        f"{quote(k, safe='')}={quote(v, safe='[]')}" for k, v in scrubbed
    )
    return urlunsplit(
        (parts.scheme, parts.netloc, parts.path, query, parts.fragment)
    )


# --- credentials inside free text ------------------------------------------------------------
#
# These patterns are kept free of look-around so the Rust `regex` crate can run the same text.

_TOKEN_SHAPES = re.compile(
    r"\bsk-(?:ant-|proj-)?[A-Za-z0-9_-]{20,}"
    r"|\bgh[pousr]_[A-Za-z0-9]{36,}"
    r"|\bgithub_pat_[A-Za-z0-9_]{40,}"
    r"|\bAKIA[0-9A-Z]{16}\b"
    r"|\bxox[baprs]-[A-Za-z0-9-]{10,}"
    r"|\bAIza[0-9A-Za-z_-]{35}"
    r"|\bnvapi-[A-Za-z0-9_-]{20,}"
    r"|\bhf_[A-Za-z0-9]{30,}"
    r"|\beyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}"
)
# `Authorization: Bearer abc`, `X-Api-Key: abc`; the scheme word is part of the secret.
_AUTH_HEADER = re.compile(
    r"(?i)\b(authorization|proxy-authorization|x-api-key|api-key)(\s*[:=]\s*)"
    r"((?:(?:bearer|basic|token)\s+)?[^\s\"'`;&|]+)"
)
# Cookie values hold spaces and semicolons; read to the end of the line or the closing quote.
_COOKIE_HEADER = re.compile(r"(?i)\b(cookie|set-cookie)(\s*:\s*)([^\r\n\"']+)")
# `--password hunter2`, `--token=abc`, `-secret \"a b\"`.
_SECRET_FLAG = re.compile(
    r"(?i)(--?(?:password|passwd|token|secret|api[-_]?key|access[-_]?key|credential)s?)(\s+|=)"
    r"(\"[^\"]*\"|'[^']*'|[^\s\"';&|-][^\s\"';&|]*)"
)
# `https://host/x?token=abc` and the same without a scheme (`curl 'host/x?token=abc'`).
_QUERY_CREDENTIAL = re.compile(
    r"(?i)([?&](?:token|access_token|refresh_token|api_key|apikey|key|secret|password|credential|sig|signature|auth)=)"
    r"([^&\s\"'#]+)"
)


def _keep_marker(match: re.Match[str], group: int) -> bool:
    return match.group(group).startswith("[redacted")


def redact_text(value: str) -> str:
    """Scrub credentials that announce themselves inside free text (a command line, a result).

    Idempotent: already redacted text passes through unchanged. Text without any of these shapes is
    returned as is.
    """
    if not value:
        return value
    out = _QUERY_CREDENTIAL.sub(
        lambda m: m.group(0) if _keep_marker(m, 2) else m.group(1) + _REDACTED, value
    )
    out = _AUTH_HEADER.sub(
        lambda m: m.group(0) if _keep_marker(m, 3) else m.group(1) + m.group(2) + _REDACTED, out
    )
    out = _COOKIE_HEADER.sub(
        lambda m: m.group(0) if _keep_marker(m, 3) else m.group(1) + m.group(2) + _REDACTED, out
    )
    out = _SECRET_FLAG.sub(
        lambda m: m.group(0) if _keep_marker(m, 3) else m.group(1) + m.group(2) + _REDACTED, out
    )
    return _TOKEN_SHAPES.sub(_REDACTED, out)


def sanitize_value(value: Any, *, typed_input_keys: frozenset[str] = frozenset()) -> Any:
    """Recursively apply the shared scrubbing policy to any JSON-ish value."""
    if isinstance(value, dict):
        out: dict[Any, Any] = {}
        for key, item in value.items():
            lk = str(key).lower()
            if is_secret_key(key):
                out[key] = _REDACTED
            elif lk in typed_input_keys:
                out[key] = _REDACTED_INPUT
            elif is_body_key(key):
                out[key] = _REDACTED_BODY
            else:
                out[key] = sanitize_value(item, typed_input_keys=typed_input_keys)
        return out
    if isinstance(value, (list, tuple)):
        sanitized = [sanitize_value(v, typed_input_keys=typed_input_keys) for v in value]
        return sanitized if isinstance(value, list) else type(value)(sanitized)
    if isinstance(value, str):
        return redact_text(redact_url_credentials(value))
    return value


def sanitize_payload(payload: Any) -> Any:
    """Scrub a ledger/log payload. Kept as a named entry point so call sites read
    honestly: this is the one shared SensitiveDataSanitizer."""
    return sanitize_value(payload)
