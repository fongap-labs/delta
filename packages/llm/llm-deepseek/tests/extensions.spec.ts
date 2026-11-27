/** Messages request contributions settle with the HTTP request that carries them. */
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it, vi, type Mock } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import LlmRuntime from '@deepseek-ai/dsh-llm'
import DeepSeekLlmApiExtensionRegistry from '@deepseek-ai/dsh-deepseek-llm-api-extensions'
import * as DeepSeek from '@deepseek-ai/dsh-llm-deepseek-api-key'
import DeepSeekAuth from '@deepseek-ai/dsh-deepseek-auth'
import { adapter, assemble, options, sse, textEvents } from './helpers.ts'

declare module '@deepseek-ai/dsh-deepseek-llm-api-extensions' {
  interface DeepSeekLlmApiExtensionMap {
    dsh_messages_test: { value: string }
  }
}

const cleanup: (() => Promise<unknown>)[] = []
afterEach(async () => {
  while (cleanup.length) await cleanup.pop()!()
  vi.unstubAllEnvs()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

async function boot() {
  const home = await mkdtemp(join(tmpdir(), 'dsh-messages-extensions-'))
  cleanup.push(() => rm(home, { recursive: true, force: true }))
  vi.stubEnv('DSH_HOME', home)
  vi.stubEnv('DEEPSEEK_API_KEY', 'test-key')
  const ctx = new Context()
  cleanup.push(() => ctx.fiber.dispose())
  await ctx.plugin(LlmRuntime)
  await ctx.plugin(DeepSeekLlmApiExtensionRegistry)
  await ctx.plugin(DeepSeekAuth)
  await ctx.plugin(DeepSeek, { baseURL: 'https://messages.example.test/root' })
  return ctx
}

function sentBody(fetch: Mock<typeof globalThis.fetch>): Record<string, unknown> {
  const body = fetch.mock.calls[0]?.[1]?.body
  if (typeof body !== 'string') throw new Error('Expected a serialized Messages request')
  return JSON.parse(body) as Record<string, unknown>
}

describe('Messages request extensions', () => {
  it.each(['dsh_session_log', 'dsh_plugin_packages', 'dsh_messages_test', 'device_info', 'unknown_future_field'])('rejects forbidden request extension %s at registration and HTTP serialization', async (field) => {
    const ctx = await boot()
    const contribution = { value: 'synthetic privacy fixture' }
    const prepare = vi.fn(() => ({ value: contribution }))
    expect(() => ctx.deepseekLlmApiExtensions.register(field as 'dsh_messages_test', { prepare }))
      .toThrow(/privacy policy/)
    expect(prepare).not.toHaveBeenCalled()
    const accept = vi.fn(async () => {})
    const llm = adapter({ baseURL: 'https://messages.example.test' }, {
      prepareExtensions: async () => ({ fields: { [field]: contribution }, accept }),
    })
    const fetch = vi.fn<typeof globalThis.fetch>()
    vi.stubGlobal('fetch', fetch)
    await expect(assemble(llm.stream(options()))).rejects.toMatchObject({
      code: 'REQUEST_EXTENSION',
      message: 'Delta privacy policy rejects auxiliary request data',
    })
    expect(fetch).not.toHaveBeenCalled()
    expect(accept).not.toHaveBeenCalled()
  })

  it('preserves the native model request and streams without any extensions', async () => {
    const ctx = await boot()
    const fetch = vi.fn<typeof globalThis.fetch>().mockResolvedValue(new Response(sse(textEvents)))
    vi.stubGlobal('fetch', fetch)
    const result = await assemble(ctx.llm.stream(options()))
    expect(result.assembler.finish.kind).toBe('stop')
    expect(sentBody(fetch)).toMatchObject({ messages: [{ role: 'user', content: [{ type: 'text', text: 'hello' }] }] })
    expect(sentBody(fetch)).not.toHaveProperty('dsh_messages_test')
    expect(sentBody(fetch)).not.toHaveProperty('dsh_session_log')
    expect(sentBody(fetch)).not.toHaveProperty('dsh_plugin_packages')
  })
})
