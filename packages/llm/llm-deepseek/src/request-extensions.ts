/** Prepare plugin-contributed request fields and commit their delivery after HTTP acceptance. */

import { errorChain, LlmError } from '@deepseek-ai/dsh-llm'
import { isApprovedDeepSeekRequestExtension, type DeepSeekLlmApiExtensionRequest, type PreparedDeepSeekLlmApiExtensions } from '@deepseek-ai/dsh-deepseek-llm-api-extensions'
import type { DeepSeekAdapterOptions } from './types.ts'
import type { WireRequest } from './wire-types.ts'

const APPROVED_MESSAGES_FIELDS: ReadonlySet<string> = new Set([
  'model', 'stream', 'messages', 'max_tokens', 'thinking', 'output_config', 'system', 'temperature', 'stop_sequences', 'tools',
] satisfies (keyof WireRequest)[])

/**
 * Merge contributions without replacing Messages fields. A rejected preparation or
 * a base-field collision reports REQUEST_EXTENSION with the cause chain appended to
 * the message. When the merged request fails to serialize, the payload is the
 * base request alone and acceptance is a no-op, so contributors resend their
 * unaccepted state on a later request. An acceptance failure is reported and
 * does not fail the request.
 * @param body - serialized Messages request before extension fields.
 * @param options - request identity, purpose, and cancellation.
 * @param prepare - contributor registry captured for this adapter.
 * @param onOmitted - receives the omitted field names and the serialization failure.
 * @param onUnaccepted - receives an acceptance failure after HTTP 2xx.
 * @returns HTTP payload and a commit to invoke only after a successful HTTP response.
 */
export async function prepareRequestExtensions(
  body: DeepSeekLlmApiExtensionRequest['body'],
  options: Omit<DeepSeekLlmApiExtensionRequest, 'body'>,
  prepare: DeepSeekAdapterOptions['prepareExtensions'],
  onOmitted: (fields: readonly string[], error: unknown) => void,
  onUnaccepted: (error: unknown) => void,
): Promise<{ payload: string; accept(): Promise<void> }> {
  if (Object.keys(body).some(field => !APPROVED_MESSAGES_FIELDS.has(field))) {
    throw new LlmError('Delta privacy policy rejects an unapproved Messages field', 'REQUEST_EXTENSION')
  }
  let extensions: PreparedDeepSeekLlmApiExtensions
  try {
    extensions = await prepare({ body, ...options })
  } catch (error) {
    throw new LlmError(`DeepSeek request extension preparation failed: ${errorChain(error)}`, 'REQUEST_EXTENSION', { cause: error })
  }
  const fields = Object.keys(extensions.fields)
  // Reject before serialization, even if an adapter bypasses the registry.
  if (fields.some(field => !isApprovedDeepSeekRequestExtension(field))) {
    throw new LlmError('Delta privacy policy rejects auxiliary request data', 'REQUEST_EXTENSION')
  }
  for (const field of fields) {
    if (Object.hasOwn(body, field)) {
      throw new LlmError(`DeepSeek request extension field ${JSON.stringify(field)} collides with the base request`, 'REQUEST_EXTENSION')
    }
  }
  let payload: string
  try {
    payload = JSON.stringify({ ...body, ...extensions.fields })
  } catch (error) {
    // A base request that cannot serialize throws here, before any field is reported as omitted.
    const base = JSON.stringify(body)
    onOmitted(fields, error)
    return { payload: base, accept: () => Promise.resolve() }
  }
  return {
    payload,
    async accept() {
      try {
        await extensions.accept()
      } catch (error) {
        onUnaccepted(error)
      }
    },
  }
}
