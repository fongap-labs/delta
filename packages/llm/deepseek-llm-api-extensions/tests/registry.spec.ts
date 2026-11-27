import { afterEach, describe, expect, it, vi } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import DeepSeekLlmApiExtensionRegistry, { isApprovedDeepSeekRequestExtension } from '../src/index.ts'

declare module '@deepseek-ai/dsh-deepseek-llm-api-extensions/types' {
  interface DeepSeekLlmApiExtensionMap {
    test_alpha: { readonly value: string }
    test_beta: readonly number[]
  }
}

const contexts: Context[] = []
const SIGNAL = new AbortController().signal

afterEach(async () => {
  await Promise.all(contexts.splice(0).map(ctx => ctx.fiber.dispose()))
})

async function harness(): Promise<Context> {
  const ctx = new Context()
  contexts.push(ctx)
  await ctx.plugin(DeepSeekLlmApiExtensionRegistry)
  return ctx
}

describe('Delta model request extension policy', () => {
  it.each(['test_alpha', 'test_beta', 'dsh_session_log', 'dsh_plugin_packages', 'device_info', 'unknown_future_field'])('rejects %s before invoking its provider', async (field) => {
    const ctx = await harness()
    const prepare = vi.fn(() => ({ value: { value: 'fixture' } }))
    expect(isApprovedDeepSeekRequestExtension(field)).toBe(false)
    expect(() => ctx.deepseekLlmApiExtensions.register(field as 'test_alpha', { prepare })).toThrow('Delta privacy policy')
    expect(prepare).not.toHaveBeenCalled()
    const result = await ctx.deepseekLlmApiExtensions.prepare({ body: { messages: [] }, signal: SIGNAL })
    expect(Object.keys(result.fields)).toEqual([])
    await result.accept()
  })

  it('prepares an empty extension transaction for ordinary requests', async () => {
    const ctx = await harness()
    const prepared = await ctx.deepseekLlmApiExtensions.prepare({ body: { messages: [] }, signal: SIGNAL })
    expect(Object.keys(prepared.fields)).toEqual([])
    await prepared.accept()
    await prepared.accept()
  })

  it('rejects an already cancelled request', async () => {
    const ctx = await harness()
    const controller = new AbortController()
    controller.abort()
    await expect(ctx.deepseekLlmApiExtensions.prepare({ body: { messages: [] }, signal: controller.signal })).rejects.toThrow()
  })
})
