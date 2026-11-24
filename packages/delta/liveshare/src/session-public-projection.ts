/** Explicit allowlist for the text a read-only LiveShare viewer may receive. */

import type { SessionHistoryRecord } from '@deepseek-ai/dsh-api-session-controller/types'

/** One sanitized message that is safe to forward to a link holder. */
export interface PublicSessionMessage {
  readonly role: 'user' | 'assistant'
  readonly text: string
}

/** Project only appended user-authored and assistant final-message text. */
export function projectPublicSessionMessage(
  record: SessionHistoryRecord,
  maxTextBytes: number,
): PublicSessionMessage | undefined {
  const event = record.event
  if (event.surfaceOp !== 'append') return undefined
  const data = asObject(event.data)
  if (event.type === 'user/message') {
    if (asObject(data.source).kind !== 'user') return undefined
    return textMessage('user', data.content, maxTextBytes)
  }
  if (event.type === 'assistant/message') {
    return textMessage('assistant', asObject(data.message).content, maxTextBytes)
  }
  return undefined
}

function textMessage(
  role: PublicSessionMessage['role'],
  input: unknown,
  maxTextBytes: number,
): PublicSessionMessage | undefined {
  if (!Array.isArray(input)) return undefined
  const text = input.flatMap(block => {
    const item = asObject(block)
    return item.type === 'text' && typeof item.text === 'string' ? [item.text] : []
  }).join('')
  if (text.length === 0 || new TextEncoder().encode(text).byteLength > maxTextBytes) return undefined
  return { role, text }
}

function asObject(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : {}
}
