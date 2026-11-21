import { lstat, realpath } from 'node:fs/promises'
import { dirname, join, normalize } from 'node:path'
import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import { defineTool, runCommand, isPathInside, isSubprocessAvailable } from '@fongap-labs/delta-plugin-shared'
import type { Settings } from '../../../types.js'
import { resolveRoot } from '../../../runtime.js'
import { resolveAuthorizedGitCwd } from './host-git-cwd.js'
import { parseGitStatus } from '../../../delta-code-repository/git-status-policy.js'
import { hasCompleteGitEvidence } from '../../../delta-code-repository/git-execution.js'
import { requireHostFs } from './host-fs.js'
import type { HostFs } from './host-fs.js'
import { validateWorktreeName } from '../../../delta-code-repository/worktree-policy.js'

/** Required Host capability set for worktrees. */
type WorktreeHostFs = HostFs<'resolve' | 'contains' | 'stat' | 'lstat' | 'processPath'>

/**
 * Host FS exclusively resolves and checks worktree targets. Existing parents
 * must be real directories, never symlinks; missing parents are created by
 * the authorized Host Git process, not by Node in the plugin process.
 */
async function resolveHostWorktreeTarget(
  fs: WorktreeHostFs, root: string, task: string, signal: AbortSignal,
  shouldExist = false,
): Promise<string> {
  signal.throwIfAborted()
  const rootTarget = await fs.resolve(root, { cwd: root, signal })
  // Paths passed to Host FS are relative to its own execution world.
  // Do not format them with Node's platform-dependent path.join.
  const workspace = '.dsh-delta-code'
  const storage = '.dsh-delta-code/worktrees'
  let storageTarget: unknown
  for (const path of [workspace, storage]) {
    signal.throwIfAborted()
    const metadata = await fs.lstat(path, { cwd: root }, signal)
    if (metadata && metadata.type !== 'directory') {
      throw new Error('worktree storage has non-directory or symlinked parent')
    }
    const target = await fs.resolve(path, { cwd: root, signal })
    if (!fs.contains(rootTarget, target)) throw new Error('worktree storage escapes Host workspace')
    if (path === storage) storageTarget = target
  }
  const targetPath = storage + '/' + task
  signal.throwIfAborted()
  const existing = await fs.lstat(targetPath, { cwd: root }, signal)
  if (shouldExist && (!existing || existing.type !== 'directory')) {
    throw new Error('managed worktree directory does not exist or is not a real directory')
  }
  if (!shouldExist && existing) throw new Error('worktree directory already exists')
  const target = await fs.resolve(targetPath, { cwd: root, signal })
  if (!fs.contains(rootTarget, target) || !fs.contains(storageTarget, target)) {
    throw new Error('worktree target escapes Host workspace')
  }
  signal.throwIfAborted()
  const processPath = fs.processPath(target)
  if (typeof processPath !== 'string' || !processPath || processPath.includes('\0')) {
    throw new Error('invalid Host worktree process path')
  }
  return processPath
}

function sameHostPath(left: string, right: string): boolean {
  const one = normalize(left)
  const two = normalize(right)
  return process.platform === 'win32' ? one.toLowerCase() === two.toLowerCase() : one === two
}

function registeredTaskWorktree(output: string, target: string, branch: string): boolean {
  const sections = output.split(/\r?\n\r?\n/).filter(Boolean)
  return sections.some(section => {
    const lines = section.split(/\r?\n/)
    const registeredPath = lines.find(line => line.startsWith('worktree '))?.slice(9)
    return registeredPath && sameHostPath(registeredPath, target) &&
      lines.includes('branch refs/heads/' + branch)
  })
}

/** Host-Git only: refuse to remove non-managed, dirty or detached worktrees. */
async function removeManagedWorktree(
  ctx: Context, settings: Settings, _root: string, cwd: string,
  target: string, task: string, signal: AbortSignal, shouldDeleteMergedBranch: boolean,
): Promise<string> {
  const branch = 'code-worker/' + task
  const list = await runCommand(ctx, {
    command: settings.gitCommand, args: ['worktree', 'list', '--porcelain'],
    cwd, timeoutMs: 15_000, maxOutputBytes: 32 * 1024, signal, hostOnly: true,
  })
  if (!hasCompleteGitEvidence(list) ||
      !registeredTaskWorktree(list.stdout, target, branch)) {
    throw new Error('managed worktree is not registered at the expected branch and Host path')
  }
  const status = await runCommand(ctx, {
    command: settings.gitCommand, args: ['status', '--porcelain=v1', '--branch', '-z'],
    cwd: target, timeoutMs: 15_000, maxOutputBytes: 64 * 1024, signal, hostOnly: true,
  })
  if (!hasCompleteGitEvidence(status)) {
    throw new Error('Host Git worktree status unavailable; refusing removal')
  }
  const parsed = parseGitStatus(status.stdout)
  if (!parsed.clean) throw new Error('managed worktree has uncommitted changes; removal refused')
  signal.throwIfAborted()
  // No --force or directory deletion; Git refuses unsafe worktree removal.
  const removed = await runCommand(ctx, {
    command: settings.gitCommand, args: ['worktree', 'remove', target],
    cwd, timeoutMs: 45_000, maxOutputBytes: 8 * 1024, signal, hostOnly: true,
  })
  if (!hasCompleteGitEvidence(removed)) {
    throw new Error('worktree removal failed or outcome is uncertain; inspect Git state before retry')
  }
  if (!shouldDeleteMergedBranch) return 'Removed managed worktree for ' + task + '; branch retained: ' + branch
  const deleted = await runCommand(ctx, {
    command: settings.gitCommand, args: ['branch', '-d', branch],
    cwd, timeoutMs: 15_000, maxOutputBytes: 8 * 1024, signal, hostOnly: true,
  })
  if (!hasCompleteGitEvidence(deleted)) {
    return 'Removed managed worktree for ' + task + '; branch retained (not merged or deletion unconfirmed): ' + branch
  }
  return 'Removed managed worktree and safely deleted merged branch: ' + branch
}

/** The host must explicitly opt in before any new worktree or branch is written. */
export function createWorktreeTool(ctx: Context, settings: Settings): ToolDefinition {
  return defineTool({
    name: 'code_git_worktree',
    description: 'List, create or safely remove an isolated managed worktree. Mutations require execution.allowWorktreeWrites; removal refuses dirty or unregistered targets and never forces deletion.',
    parameters: {
      action: { type: 'string', enum: ['list', 'create', 'remove'], required: true },
      task: { type: 'string', description: 'New task slug, e.g. fix-issue-123 (create only).' },
      base: { type: 'string', description: 'Existing base ref; defaults to HEAD (create only).' },
      deleteMergedBranch: { type: 'boolean', description: 'Remove only an already-merged task branch using git branch -d (remove only); default false.' },
    },
    output: { schema: { type: 'object', additionalProperties: false, properties: {
      action: { type: 'string', required: true }, output: { type: 'string', required: true },
    } }, render(_args, value) { return [{ type: 'text', text: value.output }] } },
    async execute(args, exec) {
      if (!isSubprocessAvailable(ctx)) throw new Error('host subprocess service is required for git worktrees')
      const root = await resolveRoot(ctx, exec, settings)
      const processCwd = await resolveAuthorizedGitCwd(ctx, root, undefined, exec)
      if (args.action === 'list') {
        const result = await runCommand(ctx, { command: settings.gitCommand, args: ['worktree', 'list', '--porcelain'],
          cwd: processCwd, timeoutMs: 15_000, maxOutputBytes: 32 * 1024, signal: exec.signal, hostOnly: true })
        if (!hasCompleteGitEvidence(result)) throw new Error('git worktree list failed')
        return { action: 'list', output: result.stdout }
      }
      if (!settings.allowWorktreeWrites) throw new Error('worktree mutation is disabled; set execution.allowWorktreeWrites=true')
      const task = args.task ?? ''
      const base = args.base ?? 'HEAD'
      validateWorktreeName(task, base)
      if (args.action !== 'create' && args.action !== 'remove') throw new Error('invalid worktree action')
      if (args.action === 'remove' && args.base !== undefined) {
        throw new Error('remove action does not accept a base ref')
      }
      if (args.action === 'create' && args.deleteMergedBranch !== undefined) {
        throw new Error('deleteMergedBranch only applies to removal')
      }
      const fs = requireHostFs(ctx, ['resolve', 'contains', 'stat', 'lstat', 'processPath'], 'refusing local worktree fallback')
      let target: string
      if (fs) {
        target = await resolveHostWorktreeTarget(fs, root, task, exec.signal,
          args.action === 'remove')
      } else {
        const canonicalRoot = await realpath(root)
        const dir = join(canonicalRoot, '.dsh-delta-code', 'worktrees')
        // Only the legacy local workspace uses Node filesystem checks.
        let ancestor = dir
        while (true) {
          try {
            const canonicalAncestor = await realpath(ancestor)
            if (!isPathInside(canonicalRoot, canonicalAncestor)) {
              throw new Error('worktree storage escapes workspace')
            }
            break
          } catch (error) {
            if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error
            const parent = dirname(ancestor)
            if (parent === ancestor) throw new Error('cannot resolve worktree storage inside workspace')
            ancestor = parent
          }
        }
        target = join(dir, task)
        let targetInfo
        try { targetInfo = await lstat(target) } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error
        }
        if (args.action === 'create' && targetInfo) throw new Error('worktree directory already exists')
        if (args.action === 'remove' && (!targetInfo || !targetInfo.isDirectory() ||
            targetInfo.isSymbolicLink() || !sameHostPath(await realpath(target), target))) {
          throw new Error('managed worktree target is absent or unsafe')
        }
      }
      exec.signal.throwIfAborted()
      if (args.action === 'remove') {
        const output = await removeManagedWorktree(ctx, settings, root, processCwd, target,
          task, exec.signal, args.deleteMergedBranch === true)
        return { action: 'remove', output }
      }
      const branch = 'code-worker/' + task
      const result = await runCommand(ctx, {
        command: settings.gitCommand, args: ['worktree', 'add', '-b', branch, target, base],
        cwd: processCwd, signal: exec.signal, timeoutMs: 45_000, maxOutputBytes: 8 * 1024, hostOnly: true,
      })
      if (!hasCompleteGitEvidence(result)) {
        throw new Error('git worktree add failed or returned incomplete evidence: ' + result.stderr.slice(0, 1000))
      }
      return { action: 'create', output: 'Created ' + branch + ' in ' + target }
    },
  })
}
