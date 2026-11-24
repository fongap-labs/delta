import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Context } from '@deepseek-ai/cordis'
import { brandString } from '@deepseek-ai/dsh-brand'
import { credentialRef } from '@deepseek-ai/dsh-credentials'
import { LocalCredentialProvider } from '@deepseek-ai/dsh-credentials-local'
import { mountAgentLoopTestDependencies, mountAgentLoopTestHarness } from '@deepseek-ai/dsh-agent-loop-testkit'
import CommandRuntime from '@deepseek-ai/dsh-commands'
import { afterEach, expect, it, vi } from 'vitest'
import * as forge from '../src/command.mjs'

afterEach(() => { vi.unstubAllGlobals() })

it('resolves private credentials per request while command history omits inputs and secrets', async () => {
  const root = await mkdtemp(join(tmpdir(), 'delta-forge-'))
  const ctx = new Context()
  const fetchMock = vi.fn(async () => Response.json({ title: 'Example', state: 'open', head: { ref: 'feature', sha: 'abc' }, base: { ref: 'main' } }))
  vi.stubGlobal('fetch', fetchMock)
  try {
    await mountAgentLoopTestDependencies(ctx, { systemPrompt: { includeRuntimeContext: false } })
    await ctx.plugin(LocalCredentialProvider, { path: join(root, '.credentials.yaml'), watch: false })
    await ctx.credentials.set(credentialRef('DELTA_GITHUB_TOKEN'), 'private-token')
    await ctx.plugin(CommandRuntime)
    const harness = await mountAgentLoopTestHarness(ctx)
    const agent = await harness.create(brandString('delta-forge-session'))
    await ctx.plugin(forge, {})
    const signal = new AbortController().signal
    const reply = await ctx.commands.execute(agent, '/forge github owner/repo 12', [], signal)
    expect(reply?.result.kind).toBe('success')
    expect(fetchMock.mock.calls[0]?.[1]?.headers).toMatchObject({ authorization: 'Bearer private-token' })
    const bad = await ctx.commands.execute(agent, '/forge github owner/repo accidentally-pasted-secret', [], signal)
    expect(bad?.result.kind).toBe('error')
    const events = agent.session.snapshotEvents().filter(event => event.type === 'command/run' || event.type === 'command/done')
    expect(events.filter(event => event.type === 'command/run')).toHaveLength(2)
    expect(events.filter(event => event.type === 'command/run').every(event => !('args' in event.data))).toBe(true)
    expect(JSON.stringify(events)).not.toContain('private-token')
    expect(JSON.stringify(events)).not.toContain('accidentally-pasted-secret')
  } finally {
    await ctx.fiber.dispose()
    await rm(root, { recursive: true, force: true })
  }
})
