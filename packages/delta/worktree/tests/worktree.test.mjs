import { test } from 'node:test'
import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { cp, mkdtemp, mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { DeltaWorktrees } from '../src/index.mjs'

const run = (cwd, ...args) => execFileSync('git', ['-C', cwd, ...args], { encoding: 'utf8', windowsHide: true })

test('real Git worktrees isolate tasks and protect uncommitted and ignored files', async () => {
  const temp = await mkdtemp(join(tmpdir(), 'delta worktree 中文 '))
  try {
    const repo = join(temp, 'project')
    const root = join(temp, 'managed')
    await mkdir(repo)
    run(repo, 'init', '-q')
    run(repo, 'config', 'user.name', 'Delta Test')
    run(repo, 'config', 'user.email', 'delta-test@example.invalid')
    run(repo, 'config', 'core.autocrlf', 'false')
    await writeFile(join(repo, 'tracked.txt'), 'original\n')
    await writeFile(join(repo, '.gitignore'), 'local.secret\n')
    run(repo, 'add', '.')
    run(repo, 'commit', '-qm', 'fixture')
    const service = await DeltaWorktrees.open(repo, root)
    const [one, two] = await Promise.all([service.create('one'), service.create('two')])
    assert.equal(await service.enter('one'), one)
    await writeFile(join(one, 'tracked.txt'), 'changed\n')
    assert.equal(await readFile(join(two, 'tracked.txt'), 'utf8'), 'original\n')
    assert.equal(await readFile(join(repo, 'tracked.txt'), 'utf8'), 'original\n')
    await assert.rejects(service.remove('one'), /contains changes/)
    await writeFile(join(one, 'tracked.txt'), 'original\n')
    await writeFile(join(one, 'local.secret'), 'keep')
    await assert.rejects(service.remove('one'), /contains changes/)
    assert.equal(await readFile(join(one, 'local.secret'), 'utf8'), 'keep')
    await rm(join(one, 'local.secret'))
    await service.remove('one')
    await service.remove('two')
    await assert.rejects(service.create('../escape'), /Invalid/)
    await assert.rejects(DeltaWorktrees.open(repo, join(repo, 'nested')), /outside/)
    await mkdir(join(repo, 'subdir'))
    await assert.rejects(DeltaWorktrees.open(join(repo, 'subdir'), join(repo, 'other')), /outside/)
  } finally { await rm(temp, { recursive: true, force: true }) }
})

test('refuses a primary checkout even when it lies under a managed root', async () => {
  const temp = await mkdtemp(join(tmpdir(), 'delta main '))
  try {
    const root = join(temp, 'managed')
    const repo = join(root, 'main')
    await mkdir(repo, { recursive: true })
    run(repo, 'init', '-q')
    const service = await DeltaWorktrees.open(repo, root)
    await assert.rejects(service.remove('main'), /main checkout/)
  } finally { await rm(temp, { recursive: true, force: true }) }
})

test('repairs a moved data directory and refuses to take over a copied worktree', async () => {
  const temp = await mkdtemp(join(tmpdir(), 'delta relocation 中文 '))
  try {
    const repo = join(temp, 'repo')
    const firstRoot = join(temp, 'first')
    const copyRoot = join(temp, 'copy')
    const movedRoot = join(temp, 'moved')
    await mkdir(repo)
    run(repo, 'init', '-q')
    run(repo, 'config', 'user.name', 'Delta Test')
    run(repo, 'config', 'user.email', 'delta@example.invalid')
    await writeFile(join(repo, 'file.txt'), 'original')
    run(repo, 'add', '.')
    run(repo, 'commit', '-qm', 'fixture')
    const first = await DeltaWorktrees.open(repo, firstRoot)
    const original = await first.create('task')
    await writeFile(join(original, 'uncommitted.txt'), 'preserve')
    await cp(firstRoot, copyRoot, { recursive: true })
    const copied = await DeltaWorktrees.open(repo, copyRoot)
    await assert.rejects(copied.enter('task'), /original worktree still exists/)
    assert.equal(await readFile(join(original, 'uncommitted.txt'), 'utf8'), 'preserve')
    await rename(firstRoot, movedRoot)
    const moved = await DeltaWorktrees.open(repo, movedRoot)
    const target = await moved.enter('task')
    assert.equal(target, join(movedRoot, 'task'))
    assert.equal(await readFile(join(target, 'uncommitted.txt'), 'utf8'), 'preserve')
    assert.match(run(repo, 'worktree', 'list', '--porcelain'), /moved[\\/]task/u)
    await assert.rejects(moved.remove('task'), /contains changes/)
  } finally { await rm(temp, { recursive: true, force: true }) }
})
