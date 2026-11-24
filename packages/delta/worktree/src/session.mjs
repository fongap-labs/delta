import { createHash } from 'node:crypto'
import { join, resolve } from 'node:path'
import { realpath } from 'node:fs/promises'
import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import { DeltaWorktrees } from './index.mjs'

const execute = promisify(execFile)

/** Human-command adapter. Session moves use the Host's durable working-directory service. */
export class WorktreeSessionController {
  #pending = new WeakMap()
  constructor(workingDirectory, root) {
    this.workingDirectory = workingDirectory
    this.root = root
  }

  run(agent, operation, task, signal) {
    if (!['create', 'enter', 'leave'].includes(operation)) throw new Error('Invalid worktree operation')
    const previous = this.#pending.get(agent.session) ?? Promise.resolve()
    const current = previous.catch(() => {}).then(async () => {
      signal.throwIfAborted()
      if (operation === 'leave') {
        const original = agent.session.header.cwd
        if (!original) throw new Error('This session has no original project directory')
        return this.workingDirectory.set(agent, original, signal)
      }
      const cwd = this.workingDirectory.get(agent.session)
      const { stdout } = await execute('git', ['-C', cwd, 'rev-parse', '--git-common-dir'], {
        encoding: 'utf8', windowsHide: true, env: { ...process.env, GIT_TERMINAL_PROMPT: '0' },
      })
      const common = await realpath(resolve(cwd, stdout.trim()))
      signal.throwIfAborted()
      const identity = process.platform === 'win32' ? common.toLowerCase() : common
      const root = join(this.root, createHash('sha256').update(identity).digest('hex'))
      const worktrees = await DeltaWorktrees.open(cwd, root)
      const target = operation === 'create' ? await worktrees.create(task) : await worktrees.enter(task)
      // A cancelled creation leaves its checkout on disk so the user can recover it.
      signal.throwIfAborted()
      return this.workingDirectory.set(agent, target, signal)
    })
    this.#pending.set(agent.session, current.then(() => {}, () => {}))
    return current
  }
}

export const name = 'delta-worktree'
export const inject = ['commands', 'workingDirectory']

/** Register an explicitly invoked human command. The plugin is opt-in in product composition. */
export function apply(ctx, config) {
  if (!config?.root) throw new Error('delta-worktree requires a managed root')
  const controller = new WorktreeSessionController(ctx.workingDirectory, config.root)
  const active = new Set()
  ctx.effect(function* () {
    yield async () => { await Promise.allSettled(active) }
    yield ctx.commands.register({
      definitionId: '@delta/worktree',
      name: 'worktree',
      description: 'Create or switch an isolated Git worktree',
      input: { hint: 'create <task> | enter <task> | leave' },
      handler(invocation) {
        const words = invocation.rawInput.trim().split(/\s+/u)
        const [operation, task] = words
        if (!['create', 'enter', 'leave'].includes(operation) || (operation === 'leave' ? words.length !== 1 : words.length !== 2)) {
          return { kind: 'error', text: 'Usage: /worktree create <task> | enter <task> | leave' }
        }
        const run = controller.run(invocation.agent, operation, task, invocation.signal)
          .then(path => ({ kind: 'success', text: `Working directory: ${path}` }))
        active.add(run)
        void run.then(() => active.delete(run), () => active.delete(run))
        return run
      },
    })
  }, 'delta worktree command')
}
