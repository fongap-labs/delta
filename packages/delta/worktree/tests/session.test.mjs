import { test } from 'node:test'
import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdtemp, mkdir, realpath, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { WorktreeSessionController, apply } from '../src/session.mjs'

const git = (cwd, ...args) => execFileSync('git', ['-C', cwd, ...args], { encoding: 'utf8', windowsHide: true })

test('user command changes only its own session and can return to its original project', async () => {
  const temp = await realpath(await mkdtemp(join(tmpdir(), 'delta session 中文 ')))
  try {
    const repo = join(temp, 'repo')
    await mkdir(repo)
    git(repo, 'init', '-q')
    git(repo, 'config', 'user.name', 'Delta Test')
    git(repo, 'config', 'user.email', 'delta@example.invalid')
    await writeFile(join(repo, 'a'), 'a')
    git(repo, 'add', '.')
    git(repo, 'commit', '-qm', 'initial')
    const session = { header: { cwd: repo }, cwd: repo }
    const other = { header: { cwd: repo }, cwd: repo }
    const wd = { get: s => s.cwd, set: async (a, path, signal) => { signal.throwIfAborted(); a.session.cwd = path; return path } }
    const ctx = { workingDirectory: wd, commands: { register: definition => { ctx.command = definition; return () => {} } }, effect(fn) { const iterator = fn(); iterator.next(); iterator.next() } }
    apply(ctx, { root: join(temp, 'data') })
    const invoke = rawInput => ctx.command.handler({ agent: { session }, rawInput, signal: new AbortController().signal })
    const result = await invoke('create task1')
    assert.equal(result.kind, 'success')
    assert.notEqual(session.cwd, repo)
    assert.equal(other.cwd, repo)
    const target = session.cwd
    assert.equal((await invoke('leave')).kind, 'success')
    assert.equal(session.cwd, repo)
    await invoke('enter task1')
    assert.equal(session.cwd, target)
    assert.equal((await invoke('remove task1')).kind, 'error')
    const aborted = AbortSignal.abort(new Error('cancelled'))
    await assert.rejects(new WorktreeSessionController(wd, join(temp, 'data')).run({ session }, 'leave', undefined, aborted), /cancelled/)
    assert.equal(session.cwd, target)
  } finally { await rm(temp, { recursive: true, force: true }) }
})
