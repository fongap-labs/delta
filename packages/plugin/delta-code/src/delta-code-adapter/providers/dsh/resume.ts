/**
 * Native DSH Todo recovery. This is a host-only projection of todo/write
 * events, not a competing task store or a background agent scheduler.
 * Unlike DSH's UI-facing todos projection, it survives turn/start.
 */
import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition, ToolRunContext } from '@deepseek-ai/dsh-tools'
import { defineTool, runCommand } from '@fongap-labs/delta-plugin-shared'
import type { Settings } from '../../../types.js'
import { resolveRoot } from '../../../runtime.js'
import { resolveAuthorizedGitCwd } from './host-git-cwd.js'
import { remainingDeliveryTasks } from '../../../delta-code-quality/delivery-policy.js'
import { parseGitStatus } from '../../../delta-code-repository/git.js'
import { retryReadOnly, classifyFailure } from '../../../delta-code-quality/failure-policy.js'

import {
  EMPTY, HEAD_PATTERN, foldNativeResume, parseResumeState, result, matchesNativeTodoPlan,
  type ResumeState, type NativeResumeReader,
} from '../../../delta-code-quality/resume-policy.js'
type ProjectionHandle = { stateOf(session: unknown): ResumeState; dispose(): Promise<void> }
type ProjectionRegistry = { register(definition: unknown): Promise<ProjectionHandle> }

/**
 * Register a HOST-ONLY derived projection through the native DSH registry.
 * The installed Host owns replay/checkpoints; no disk, file, or alternate log
 * is read from this plugin. A missing/older Host service fails closed.
 */
export function registerNativeResume(ctx: Context): NativeResumeReader {
  let isActive = true
  let handle: ProjectionHandle | undefined
  // An unregistered DSH handle remains readable; revoke its use immediately
  // when the Host service is withdrawn or a replacement registration starts.
  let registrationEpoch = 0
  const disposeHandle = (old: ProjectionHandle | undefined): void => {
    if (!old) return
    void Promise.resolve().then(() => old.dispose()).catch(() => {
      ctx.logger?.warn?.('dsh-delta-code: native resume projection disposal failed')
    })
  }
  const clearHandle = (): void => {
    const old = handle
    handle = undefined
    disposeHandle(old)
  }
  const definition = {
    key: 'deltaCodeNativeResume',
    stateVersion: 1,
    stateSchema: { parse: parseResumeState },
    init: () => ({ ...EMPTY }),
    apply: foldNativeResume,
  }
  const injected = ctx.inject(['sessionProjections'], (child) => {
    if (!isActive) return
    const registry = (child as unknown as { sessionProjections?: ProjectionRegistry }).sessionProjections
    if (typeof registry?.register !== 'function') return
    const epoch = ++registrationEpoch
    clearHandle()
    let isScopeActive = true
    try {
      // The Host's registration is awaitable and may complete after a service
      // withdrawal. Only the currently mounted epoch may publish a reader.
      void Promise.resolve(registry.register(definition)).then(registered => {
        if (!isActive || !isScopeActive || epoch !== registrationEpoch) {
          disposeHandle(registered)
          return
        }
        handle = registered
      }).catch(() => {
        ctx.logger?.warn?.('dsh-delta-code: native resume projection registration failed')
      })
    } catch {
      ctx.logger?.warn?.('dsh-delta-code: native resume projection registration failed')
    }
    return () => {
      isScopeActive = false
      if (epoch !== registrationEpoch) return
      registrationEpoch++
      clearHandle()
    }
  })
  return {
    read(agent) {
      if (!isActive || !handle || !agent || typeof agent !== 'object') return undefined
      const session = (agent as { session?: unknown }).session
      if (!session) return undefined
      return parseResumeState(handle.stateOf(session))
    },
    dispose() {
      if (!isActive) return
      isActive = false
      registrationEpoch++
      clearHandle()
      const disposeInjection = (injected as { dispose?: () => Promise<void> } | null)?.dispose
      if (typeof disposeInjection === 'function') {
        void Promise.resolve().then(() => disposeInjection.call(injected)).catch(() => {
          ctx.logger?.warn?.('dsh-delta-code: native resume injection disposal failed')
        })
      }
    },
  }
}

async function hostGit(ctx: Context, settings: Settings, cwd: string,
  exec: ToolRunContext, args: string[]): Promise<string> {
  return retryReadOnly(async () => {
    const output = await runCommand(ctx, {
      command: settings.gitCommand, args, cwd, signal: exec.signal,
      timeoutMs: 30_000, maxOutputBytes: 64 * 1024, hostOnly: true,
    })
    if (output.exitCode !== 0 || output.signal || output.timedOut || output.lossy) {
      // Only a KNOWN failed process exit with a transient response is safe to
      // retry. Never turn an unknown exit into a synthetic HTTP 503.
      const transient = Number.isSafeInteger(output.exitCode) &&
        output.exitCode !== null && output.exitCode > 0 &&
        !output.timedOut && !output.signal && !output.lossy &&
        classifyFailure(output.stderr) === 'transient'
      // Preserve the upstream explicit no-retry decision without including raw
      // stderr, which could contain Host paths or secrets.
      const nonRetryable = /(?:["']?retryable["']?\s*[:=]\s*false\b|\bnon[- ]retryable\b)/i.test(output.stderr)
      throw new Error(transient
        ? 'Host Git temporarily unavailable' + (nonRetryable ? '; retryable=false' : '')
        : 'DSH Host Git inspection failed or returned incomplete data')
    }
    return output.stdout
  }, exec.signal)
}

/** Explicit, read-only resume decision. Never replays edits or trusts Todo alone. */
export function createNativeResumeTool(
  ctx: Context, settings: Settings, reader: NativeResumeReader,
): ToolDefinition {
  return defineTool({
    name: 'code_resume',
    description: 'Recover native todo_write snapshot and verify its optional exact planned tasks against real Host state, then reconcile Git HEAD/status. No writes or auto replay. Supply expectedHead when the previous checkpoint recorded one.',
    parameters: {
      expectedHead: { type: 'string', description: 'Optional checkpoint HEAD SHA; when supplied, a mismatch blocks continuation.' },
      expectedSeq: { type: 'integer', description: 'Exact native todo/write sequence from the previous checkpoint; reject stale continuations when supplied.' },
      expectedTasks: { type: 'array', items: { type: 'string' }, description: 'Optional exact native Todo content list from a prior code_workflow_route nativeTodoDraft. Mismatch blocks resuming; no task is written.' },
      cwd: { type: 'string', description: 'Optional workspace-relative repository/worktree path.' },
    },
    output: { schema: { type: 'object', additionalProperties: false, properties: {
      stage: { type: 'string', required: true },
      head: { type: 'string', required: true },
      sourceSeq: { type: 'integer', required: true },
      nextAction: { type: 'string', required: true },
      tasks: { type: 'array', required: true, items: { type: 'object', additionalProperties: false, properties: {
        content: { type: 'string', required: true },
        status: { type: 'string', required: true },
      } } },
      remaining: { type: 'array', required: true, items: { type: 'string' } },
      changed: { type: 'array', required: true, items: { type: 'string' } },
      blockers: { type: 'array', required: true, items: { type: 'string' } },
    } }, render(_args, output) {
      return [{ type: 'text', text: [
        'Native Code Worker resume: ' + output.stage,
        'Last Todo event: ' + (output.sourceSeq >= 0 ? output.sourceSeq : '(none)'),
        'HEAD: ' + (output.head || '(not checked)'),
        'Next: ' + output.nextAction,
        ...output.remaining.slice(0, 8).map(task => 'Unfinished: ' + task),
        ...output.changed.slice(0, 8).map(file => 'Git change: ' + file),
        ...output.blockers.map(reason => 'Blocked: ' + reason),
        'Read-only evidence only. Reconcile prior completed steps before editing; no automatic replay, merge or release.',
      ].join('\n') }]
    } },
    async execute(args, exec) {
      if (!exec.agent) return result('blocked', 'Run in the owning DSH agent Session; do not use another session history.',
        undefined, '', [], ['owning agent session is absent'])
      if (exec.signal.aborted) throw new Error('native resume cancelled')
      let state: ResumeState | undefined
      try { state = reader.read(exec.agent) } catch {
        return result('blocked', 'Inspect DSH native session projection restore failure.',
          undefined, '', [], ['native Todo projection failed to load'])
      }
      if (!state) return result('blocked', 'Mount the DSH sessionProjections service; do not synthesize task history.',
        undefined, '', [], ['native Todo projection is unavailable'])
      if (state.invalid) return result('blocked', 'Inspect malformed native Todo history; do not replay stale operations.',
        undefined, '', [], ['native Todo event is malformed'])
      // Check pinned identity even when the Host has no Todo snapshot or has
      // written an empty list. Otherwise a stale checkpoint silently looks empty.
      if (args.expectedSeq !== undefined &&
          (!Number.isSafeInteger(args.expectedSeq) || args.expectedSeq < 0 || args.expectedSeq !== state.writeSeq)) {
        return result('blocked', 'Native Todo checkpoint sequence differs; reload tasks before continuing.',
          state, '', [], ['native Todo sequence differs from expected checkpoint'])
      }
      if (args.expectedHead !== undefined && (typeof args.expectedHead !== 'string' ||
          !HEAD_PATTERN.test(args.expectedHead))) {
        return result('blocked', 'Supply a valid 40/64-character previous Git HEAD or omit the baseline.',
          state, '', [], ['invalid expectedHead'])
      }
      if (args.expectedTasks !== undefined && !matchesNativeTodoPlan(state, args.expectedTasks)) {
        return result('blocked', 'Native Todo does not match the proposed plan; reconcile task ordering and content before resuming.',
          state, '', [], ['native Todo does not match expected plan'])
      }
      if (state.tasks === null || state.tasks.length === 0) {
        return result('empty', 'No current native Todo checkpoint exists; establish a new task plan.', state)
      }
      const pending = remainingDeliveryTasks(state.tasks)
      // A completed native Todo is a progress claim, not Git or test evidence.
      // Always validate its baseline and worktree before recommending next steps.
      const todosComplete = pending.length === 0
      if (!settings.gitEnabled) return result('blocked', 'Enable authorized Git inspection for task reconciliation.',
        state, '', [], ['Git tool is disabled'])
      if (exec.signal.aborted) throw new Error('native resume cancelled')
      let head = ''
      try {
        const root = await resolveRoot(ctx, exec, settings)
        const cwd = await resolveAuthorizedGitCwd(ctx, root, args.cwd, exec)
        head = (await hostGit(ctx, settings, cwd, exec, ['rev-parse', '--verify', 'HEAD'])).trim()
        if (!HEAD_PATTERN.test(head)) throw new Error('Host returned invalid Git HEAD')
        if (args.expectedHead && head !== args.expectedHead) {
          return result('blocked', 'Git HEAD changed; inspect commits and refresh plan before any task operation.',
            state, head, [], ['Git HEAD differs from previous checkpoint'])
        }
        const before = await hostGit(ctx, settings, cwd, exec, ['status', '--porcelain=v1', '--branch', '-z'])
        // A clean first status is not enough: another writer can change the
        // worktree while the Host inspection is in progress. Re-read status
        // before accepting a resume recommendation; no local Git fallback.
        const after = await hostGit(ctx, settings, cwd, exec, ['status', '--porcelain=v1', '--branch', '-z'])
        const afterHead = (await hostGit(ctx, settings, cwd, exec, ['rev-parse', '--verify', 'HEAD'])).trim()
        if (head !== afterHead) {
          return result('blocked', 'HEAD changed during read; reconcile repository and retry.',
            state, afterHead, [], ['Git HEAD changed while inspecting'])
        }
        if (before !== after) {
          return result('blocked', 'Git status changed during inspection; refresh the repository state.',
            state, head, [], ['Git status changed while inspecting'])
        }
        const current = reader.read(exec.agent)
        if (!current || current.invalid || current.writeSeq !== state.writeSeq ||
            JSON.stringify(current.tasks) !== JSON.stringify(state.tasks)) {
          return result('blocked', 'Native task history changed during inspection; reload it.',
            state, head, [], ['native Todo changed during Git inspection'])
        }
        const git = parseGitStatus(before)
        const changed = [...git.staged, ...git.unstaged, ...git.untracked]
        if (changed.length > 0) {
          return result('reconcile', todosComplete
            ? 'Native Todos claim completion but Git has pending changes; inspect the diff and run the delivery gate.'
            : 'Inspect existing staged/unstaged/untracked work before continuing: ' + pending[0],
            state, head, changed)
        }
        if (!args.expectedHead) return result('reconcile', todosComplete
          ? 'Native Todos claim completion but no Git baseline was supplied; inspect commits and run the delivery gate.'
          : 'No previous Git baseline was supplied; inspect commits and validate completed Todo steps before continuing: ' + pending[0],
          state, head)
        if (todosComplete) return result('reconcile',
          'Native Todos claim completion and Git is stable; obtain fresh delivery-gate evidence before reporting completion.',
          state, head)
        return result('resume', 'Revalidate the first incomplete Todo step, then continue: ' + pending[0], state, head)
      } catch {
        if (exec.signal.aborted) throw new Error('native resume cancelled')
        return result('blocked', 'Inspect Host Git permissions, repository path and command output; never fall back to local Git.',
          state, head, [], ['DSH Host Git inspection was denied or incomplete'])
      }
    },
  })
}
