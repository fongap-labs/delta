/** Bound and redact diagnostic text before it reaches local UI or logs. */
const LIMIT = 1200

/** Secret-bearing values and machine identifiers are not needed for recovery guidance. */
export function redactDiagnostic(value: string): string {
  return value
    .replace(/\b(?:Bearer|Basic)\s+[A-Za-z0-9._~+\/-]+=*/giu, '[credential redacted]')
    .replace(/\b(?:sk|ds|ghp|gho|ghu|ghs|ghr)-[A-Za-z0-9_-]{12,}\b/giu, '[credential redacted]')
    .replace(/\beyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b/gu, '[token redacted]')
    .replace(/((?:["']?(?:api_?key|access_?token|refresh_?token|id_?token|token|password|secret|authorization|cookie|session_?id|user_?id|account_?id|device_?id|install_?id|machine_?id|request_?id|trace_?id|span_?id|client_?id|client_?secret|serial_?number)["']?)\s*[:=]\s*)["']?[^"'\s,;&}]+/giu, '$1[redacted]')
    .replace(/\b(?:https?:\/\/)[^\s"'<>]+/giu, '[URL redacted]')
    .replace(/\b[A-Z]:\\(?:[^\\\n ]+\\)*[^\s\n]+/gu, '[local path redacted]')
    .replace(/\/(?:Users|home|root|private\/var|var\/folders|tmp|Volumes)\/[^\s'"\n]+/gu, '[local path redacted]')
    .replace(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/giu, '[email redacted]')
    .replace(/\b(?:\d{1,3}\.){3}\d{1,3}\b/gu, '[IP redacted]')
    .slice(-LIMIT)
}
