/** Deterministic serialization of credential headers for credential-scoped caches. @module dsh-llm-deepseek/auth-headers */

/**
 * Serialize credential headers so caches scoped by credential digest the same
 * credential to the same value whatever the header order. The result is hash
 * input only: never persist or log it.
 * @param headers - credential headers of one request.
 * @returns a stable string representation of the headers.
 */
export function serializeAuthHeaders(headers: Readonly<Record<string, string>>): string {
  return JSON.stringify(Object.entries(headers).sort(([left], [right]) => left.localeCompare(right)))
}

const APPROVED_AUTH_HEADERS: ReadonlySet<string> = new Set(['authorization', 'x-api-key', 'x-dsh-auth-token'])

/** Only credential headers explicitly required by the provider may cross the network boundary. */
export function approvedDeepSeekAuthHeaders(headers: Readonly<Record<string, string>>): Record<string, string> {
  const result: Record<string, string> = {}
  for (const [name, value] of Object.entries(headers)) {
    const lower = name.toLowerCase()
    if (!APPROVED_AUTH_HEADERS.has(lower) || Object.hasOwn(result, lower) || /[\r\n]/u.test(value)) {
      throw new Error('Delta privacy policy rejects an unapproved authentication header')
    }
    result[lower] = value
  }
  return result
}
