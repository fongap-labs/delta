import { expect, it, vi } from 'vitest'
import { brandString } from '@deepseek-ai/dsh-brand'
import { applyAgentForgeTool } from '../src/agent-tool.mjs'
import zhLocale from '../locale/zh.json' with { type: 'json' }
import { assertSupportedJsonSchema, projectToolSchema } from '@deepseek-ai/dsh-tools'
import type { Agent } from '@deepseek-ai/dsh-agent'
import type { ToolCallId } from '@deepseek-ai/dsh-llm'
import type { ToolDefinition } from '@deepseek-ai/dsh-tools'

const result = {
  provider: 'github' as const,
  project: 'owner/repo',
  number: 12,
  title: 'Update parser',
  state: 'open',
  sourceBranch: 'feature/parser',
  targetBranch: 'main',
  headSha: 'abc123',
  description: 'Please review the parser changes.',
}

function setup(outcome: 'allowed-once' | 'rejected' | 'cancelled' | 'unavailable' = 'allowed-once') {
  let tool: ToolDefinition | undefined
  const approval = vi.fn(async () => outcome)
  const resolve = vi.fn(async () => ({ value: 'private-token', source: 'test' as const }))
  const readChange = vi.fn(async () => result)
  const ctx = {
    tools: { register(value: ToolDefinition) { tool = value; return () => {} } },
    approval: { request: approval },
    credentials: { resolve },
  }
  applyAgentForgeTool(ctx, {
    github: { client: { readChange } as never, tokenRef: 'DELTA_GITHUB_TOKEN' },
    gitlab: { client: { readChange } as never },
  })
  const getTool = () => {
    if (tool === undefined) throw new Error('Forge agent tool was not registered')
    return tool
  }
  const execute = (args: unknown = { provider: 'github', project: 'owner/repo', number: 12 }) => getTool().execute(
    args,
    {
      callId: brandString<ToolCallId>('forge-call'),
      name: 'forge_read_change_request',
      arguments: args,
      agent: {} as Agent,
      signal: new AbortController().signal,
      token: Symbol('tool-token') as never,
      rootCallId: brandString<ToolCallId>('forge-root'),
      deferContext() {},
      concludeTurn() {},
    },
  )
  return { execute, approval, resolve, readChange, getTool }
}

it('requires a fresh localized one-shot approval before resolving credentials or reading Forge', async () => {
  const fixture = setup()
  expect(() => { assertSupportedJsonSchema(fixture.getTool().output.schema) }).not.toThrow()
  expect(projectToolSchema(fixture.getTool(), true)).toMatchObject({ name: 'forge_read_change_request' })
  const value = await fixture.execute()
  expect(value).toEqual({ status: 'success', result })
  expect(fixture.approval).toHaveBeenCalledWith(expect.objectContaining({
    toolName: 'forge_read_change_request',
    reason: 'The agent wants to make one read-only request to a configured Forge service.',
    displayReason: {
      en: 'Allow a read-only request for GitHub pull request owner/repo #12?',
      zh: zhLocale.approval.github.replace('{project}', 'owner/repo').replace('{number}', '12'),
    },
  }))
  expect(fixture.resolve).toHaveBeenCalledOnce()
  expect(fixture.readChange).toHaveBeenCalledWith('owner/repo', 12, expect.objectContaining({ token: 'private-token' }))
  const rendered = fixture.getTool().output.render({}, value)
  expect(rendered[0]?.text).toContain('UNTRUSTED EXTERNAL FORGE DATA')
  expect(rendered[0]?.text).toContain('Update parser')
  expect(rendered[0]?.text).not.toContain('private-token')
})

it.each([
  ['rejected', 'denied'],
  ['cancelled', 'cancelled'],
  ['unavailable', 'unavailable'],
] as const)('fails closed when approval is %s', async (outcome, status) => {
  const fixture = setup(outcome)
  await expect(fixture.execute()).resolves.toEqual({ status })
  expect(fixture.resolve).not.toHaveBeenCalled()
  expect(fixture.readChange).not.toHaveBeenCalled()
})

it('rejects malformed requests before asking for approval', async () => {
  const fixture = setup()
  await expect(fixture.execute({ provider: 'github', project: '../repo', number: 12 })).rejects.toThrow(/Invalid repository/)
  expect(fixture.approval).not.toHaveBeenCalled()
  expect(fixture.resolve).not.toHaveBeenCalled()
  expect(fixture.readChange).not.toHaveBeenCalled()
})

it('does not disclose transport or credential failures to the model', async () => {
  const fixture = setup()
  fixture.resolve.mockRejectedValueOnce(new Error('private-token leaked'))
  await expect(fixture.execute()).resolves.toEqual({ status: 'error' })
  const rendered = fixture.getTool().output.render({}, { status: 'error' })
  expect(rendered[0]?.text).toBe('Forge request error. Do not retry unless the user approves another request.')
  expect(rendered[0]?.text).not.toContain('private-token')
  expect(fixture.readChange).not.toHaveBeenCalled()
})
