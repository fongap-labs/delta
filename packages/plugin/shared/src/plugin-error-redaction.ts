/** Redact caller-supplied secrets before writing diagnostic text. */
export function redactPluginMessage(message: string, secrets: readonly string[] = []): string {
  if (typeof message !== 'string') throw new TypeError('plugin message must be a string')
  let result = message
  for (const secret of [...new Set(secrets)].filter(Boolean).sort((a, b) => b.length - a.length)) {
    result = result.split(secret).join('[REDACTED]')
  }
  return result.replace(/\bBearer\s+[^\s,;]+/gi, 'Bearer [REDACTED]').slice(0, 4096)
}

/** Do not include arbitrary error.message or stack content in default logs. */
export function describePluginError(error: unknown): string {
  return error instanceof Error ? error.name || 'Error' : 'UnknownError'
}
