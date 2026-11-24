import { execFileSync } from 'node:child_process'
import { mkdtemp, mkdir, realpath, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Context } from '@deepseek-ai/cordis'
import { brandString } from '@deepseek-ai/dsh-brand'
import { mountAgentLoopTestDependencies, mountAgentLoopTestHarness } from '@deepseek-ai/dsh-agent-loop-testkit'
import LocalFileSystem from '@deepseek-ai/dsh-fs-local'
import WorkingDirectory from '@deepseek-ai/dsh-working-directory'
import CommandRuntime from '@deepseek-ai/dsh-commands'
import { expect, it } from 'vitest'
import * as worktree from '../../../delta/worktree/src/session.mjs'

it('routes user commands to the real session directory service and records the move', async () => {
  const root = await realpath(await mkdtemp(join(tmpdir(), 'delta-session-')))
  const ctx = new Context()
  try {
    const repo = join(root, 'repo')
    await mkdir(repo)
    const git = (...args: string[]) => execFileSync('git', ['-C', repo, ...args], { windowsHide: true })
    git('init', '-q')
    git('config', 'user.name', 'Delta Test')
    git('config', 'user.email', 'delta@example.invalid')
    await writeFile(join(repo, 'a'), 'a')
    git('add', '.')
    git('commit', '-qm', 'initial')
    await mountAgentLoopTestDependencies(ctx, { systemPrompt: { includeRuntimeContext: false } })
    await ctx.plugin(LocalFileSystem, { cwd: repo })
    await ctx.plugin(WorkingDirectory, {})
    await ctx.plugin(CommandRuntime)
    const harness = await mountAgentLoopTestHarness(ctx)
    const agent = await harness.create(brandString('delta-worktree-session'), {}, { cwd: repo })
    const other = await harness.create(brandString('delta-worktree-other'), {}, { cwd: repo })
    await ctx.plugin(worktree, { root: join(root, 'data') })
    const signal = new AbortController().signal
    const created = await ctx.commands.execute(agent, '/worktree create task', [], signal)
    expect(created?.result.kind).toBe('success')
    const moved = ctx.workingDirectory.get(agent.session)
    expect(moved).not.toBe(repo)
    expect(ctx.workingDirectory.get(other.session)).toBe(repo)
    expect(agent.session.snapshotEvents()).toContainEqual(expect.objectContaining({ type: 'workspace-root/change', data: { cwd: moved } }))
    expect((await ctx.commands.execute(agent, '/worktree leave', [], signal))?.result.kind).toBe('success')
    expect(ctx.workingDirectory.get(agent.session)).toBe(repo)
    expect((await ctx.commands.execute(agent, '/worktree enter task', [], signal))?.result.kind).toBe('success')
    expect(ctx.workingDirectory.get(agent.session)).toBe(moved)
  } finally {
    await ctx.fiber.dispose()
    await rm(root, { recursive: true, force: true })
  }
})
