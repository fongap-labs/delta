/** Bound and redact diagnostic details before showing them in Delta UI or console. */
const LIMIT = 1200

/** Secret-bearing values are not needed to explain a startup failure. */
export function redactDiagnostic(value: string): string {
  return value
    .replace(/\b(?:Bearer|Basic)\s+[A-Za-z0-9._~+\/-]+=*/giu, '[credential redacted]')
    .replace(/\b(?:sk|ds|ghp|gho|ghu|ghs|ghr)-[A-Za-z0-9_-]{12,}\b/giu, '[credential redacted]')
    .replace(/\beyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b/gu, '[token redacted]')
    .replace(/((?:["']?(?:api_?key|access_?token|refresh_?token|password|secret|authorization|session_?id|user_?id|device_?id|install_?id|machine_?id)["']?)\s*[:=]\s*)["']?[^"'\s,;&}]+/giu, '$1[redacted]')
    .replace(/\b(?:https?:\/\/)[^\s"'<>]+/giu, '[URL redacted]')
    .replace(/\b[A-Z]:\\(?:[^\\\n ]+\\)*[^\s\n]+/giu, '[local path redacted]')
    .replace(/\/(?:Users|home|root)\/[^\s'"\n]+/gu, '[local path redacted]')
    .replace(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/giu, '[email redacted]')
    .slice(-LIMIT)
}
