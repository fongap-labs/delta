import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import { lstat, mkdir, readFile, realpath } from 'node:fs/promises'
import { basename, dirname, isAbsolute, join, resolve, relative, sep } from 'node:path'

const execute = promisify(execFile)
const locks = new Map()
const validTask = /^[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}$/u

async function git(cwd, args) {
  const { stdout } = await execute('git', ['-C', cwd, ...args], {
    encoding: 'utf8', maxBuffer: 8 * 1024 * 1024, windowsHide: true,
    env: { ...process.env, GIT_TERMINAL_PROMPT: '0' },
  })
  return stdout
}

async function serial(key, action) {
  const prior = locks.get(key) ?? Promise.resolve()
  const next = prior.catch(() => {}).then(action)
  locks.set(key, next)
  try { return await next } finally { if (locks.get(key) === next) locks.delete(key) }
}

function taskPath(root, task) {
  if (!validTask.test(task)) throw new Error('Invalid worktree task identifier')
  return join(root, task)
}

async function canonicalDirectory(value) {
  if (!isAbsolute(value)) throw new Error('Worktree paths must be absolute')
  return realpath(value)
}

function comparablePath(value) {
  const normalized = resolve(value)
  return process.platform === 'win32' ? normalized.toLowerCase() : normalized
}

/** A Delta-managed checkout area. The caller owns user authorization and session coordination. */
export class DeltaWorktrees {
  static async open(repository, managedRoot) {
    if (!isAbsolute(managedRoot)) throw new Error('Worktree paths must be absolute')
    const cwd = await canonicalDirectory(repository)
    const checkout = await canonicalDirectory((await git(cwd, ['rev-parse', '--show-toplevel'])).trim())
    const common = await canonicalDirectory(resolve(cwd, (await git(cwd, ['rev-parse', '--git-common-dir'])).trim()))
    const proposed = relative(checkout, resolve(managedRoot))
    if (!proposed || (!proposed.startsWith(`..${sep}`) && proposed !== '..' && !isAbsolute(proposed))) {
      throw new Error('Worktree storage must be outside the repository checkout')
    }
    await mkdir(managedRoot, { recursive: true })
    const root = await canonicalDirectory(managedRoot)
    const within = relative(checkout, root)
    if (!within || (!within.startsWith(`..${sep}`) && within !== '..' && !isAbsolute(within))) {
      throw new Error('Worktree storage must be outside the repository checkout')
    }
    return new DeltaWorktrees(checkout, common, root)
  }

  constructor(repository, common, root) {
    this.repository = repository
    this.common = common
    this.root = root
  }

  /** Creates a detached checkout at the repository's current HEAD. */
  async create(task) {
    const target = taskPath(this.root, task)
    return serial(this.common, async () => {
      try { await lstat(target); throw new Error('Worktree target already exists') }
      catch (error) { if (error?.code !== 'ENOENT') throw error }
      await git(this.repository, ['worktree', 'add', '--detach', '--', target, 'HEAD'])
      return target
    })
  }

  /** Verify an existing managed path is a linked checkout of this repository. */
  async enter(task) {
    return serial(this.common, () => this.#enter(task))
  }

  async #enter(task) {
    const target = taskPath(this.root, task)
    const entry = await lstat(target)
    if (!entry.isDirectory() || entry.isSymbolicLink()) throw new Error('Worktree target is not a directory')
    const actual = await realpath(target)
    if (actual !== target) throw new Error('Worktree target resolved outside the managed path')
    const common = await canonicalDirectory(resolve(actual, (await git(actual, ['rev-parse', '--git-common-dir'])).trim()))
    if (common !== this.common) throw new Error('Worktree belongs to another repository')
    const gitDir = await canonicalDirectory(resolve(actual, (await git(actual, ['rev-parse', '--git-dir'])).trim()))
    if (gitDir === common) throw new Error('The main checkout cannot be removed as a worktree')
    const top = await canonicalDirectory((await git(actual, ['rev-parse', '--show-toplevel'])).trim())
    if (top !== actual) throw new Error('Path is not a worktree root')
    // Git stores a backlink from its administrative directory to the checkout.
    // A moved portable data directory leaves that backlink pointing at the old
    // location, even though commands inside the moved checkout still work.
    const backlink = (await readFile(join(gitDir, 'gitdir'), 'utf8')).trim()
    if (!backlink) throw new Error('Worktree has no Git backlink')
    const recorded = resolve(gitDir, backlink)
    const expected = join(actual, '.git')
    if (basename(recorded).toLowerCase() !== '.git') throw new Error('Invalid worktree Git backlink')
    if (comparablePath(recorded) !== comparablePath(expected)) {
      try {
        await lstat(dirname(recorded))
        throw new Error('The original worktree still exists; refusing to reattach a copied directory')
      } catch (error) {
        if (error?.code !== 'ENOENT') throw error
      }
      await git(this.repository, ['worktree', 'repair', '--', actual])
      const repaired = resolve(gitDir, (await readFile(join(gitDir, 'gitdir'), 'utf8')).trim())
      if (comparablePath(repaired) !== comparablePath(expected)) throw new Error('Git did not repair the worktree backlink')
    }
    return actual
  }

  /** Removes a clean linked checkout; any untracked or ignored files block removal. */
  async remove(task) {
    return serial(this.common, async () => {
      const target = await this.#enter(task)
      const status = await git(target, ['status', '--porcelain=v1', '--untracked-files=all', '--ignored=matching'])
      if (status.length) throw new Error('Worktree contains changes or untracked files')
      await git(this.repository, ['worktree', 'remove', '--', target])
    })
  }
}
