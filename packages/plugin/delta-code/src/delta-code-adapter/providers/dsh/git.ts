import { relative } from 'node:path'
import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import { defineTool, runCommand, isSubprocessMounted, resolveInside, isPathInside } from '@fongap-labs/delta-plugin-shared'
import type { Settings } from '../../../types.js'
import { TOOL_PREFIX, resolveRoot } from '../../../runtime.js'
import { resolveAuthorizedGitCwd } from './host-git-cwd.js'
import { requireHostFs } from './host-fs.js'
import { lookupDshHostFs } from './host-adapter.js'
import { assertGitExit } from '../../../delta-code-repository/git-execution.js'
import { parseGitStatus } from '../../../delta-code-repository/git-status-policy.js'
export { parseGitStatus } from '../../../delta-code-repository/git-status-policy.js'


/**
 * The public diff path is workspace-root-relative, not a Git pathspec.
 * Derive a literal pathspec within the selected checkout. For a mounted Host
 * FS, resolve actual targets through the Host and reject cross-repo aliases.
 */
async function resolveDiffPath(
  ctx: Context, root: string, requestedCwd: string | undefined,
  path: string, signal: AbortSignal,
): Promise<string> {
  if (!path || path.startsWith('/') || path.includes('\\') || path.includes('\0') ||
      /^[a-zA-Z]:/.test(path) ||
      path.split('/').some(segment => !segment || segment === '.' || segment === '..')) {
    throw new Error('git diff requires a safe workspace-relative path')
  }
  const fs = requireHostFs(ctx, ['resolve', 'contains'], 'diff path containment methods required')
  if (fs) {
    if (requestedCwd && (requestedCwd.startsWith('/') || requestedCwd.includes('\\') ||
        /^[a-zA-Z]:/.test(requestedCwd) ||
        requestedCwd.split('/').some(segment => !segment || segment === '.' || segment === '..'))) {
      throw new Error('git diff cwd must be a workspace-relative directory')
    }
    const rootTarget = await fs.resolve(requestedCwd ?? root, { cwd: root, signal })
    const target = await fs.resolve(path, { cwd: root, signal })
    if (!fs.contains(rootTarget, target)) throw new Error('git diff path escapes selected repository')
    if (signal.aborted) throw new Error('git diff cancelled during path authorization')
    const prefix = requestedCwd ? requestedCwd + '/' : ''
    if (prefix && !path.startsWith(prefix)) {
      throw new Error('git diff path must be relative to the selected repository')
    }
    const relativePath = prefix ? path.slice(prefix.length) : path
    return ':(literal)' + relativePath
  }
  const localCwd = requestedCwd ? resolveInside(root, requestedCwd) : root
  const localPath = resolveInside(root, path)
  if (!isPathInside(localCwd, localPath) || localPath === localCwd) {
    throw new Error('git diff path escapes selected repository')
  }
  const relativePath = relative(localCwd, localPath)
  return ':(literal)' + relativePath
}

export function createGitStatusTool(ctx: Context, settings: Settings): ToolDefinition {
  return defineTool({
    name: `${TOOL_PREFIX}git_status`,
    description:
      'Run `git status --porcelain=v1 --branch` in the workspace and return a parsed summary of branch, staged, unstaged and untracked entries.',
    parameters: {
      cwd: { type: 'string', description: 'Optional subdirectory inside the workspace to run git in.' },
    },
    output: {
      schema: {
        type: 'object',
        additionalProperties: false,
        properties: {
          // Every property below is always produced by `parseGitStatus`, so the
          // declared contract marks them required instead of optional.
          branch: { type: 'string', required: true },
          ahead: { type: 'integer', required: true },
          behind: { type: 'integer', required: true },
          staged: { type: 'array', required: true, items: { type: 'string' } },
          unstaged: { type: 'array', required: true, items: { type: 'string' } },
          untracked: { type: 'array', required: true, items: { type: 'string' } },
          clean: { type: 'boolean', required: true },
        },
      },
      render(_args, value) {
        const head = value.branch ? `On branch ${value.branch}.` : 'No branch.'
        const counts = [
          `${value.staged.length} staged`,
          `${value.unstaged.length} unstaged`,
          `${value.untracked.length} untracked`,
        ].join(', ')
        const body = [...value.staged, ...value.unstaged, ...value.untracked].slice(0, 30)
        const lines = [
          head,
          counts,
          value.clean ? 'Working tree clean.' : body.map((file) => `  ${file}`).join('\n'),
        ]
        return [{ type: 'text', text: lines.filter(Boolean).join('\n') }]
      },
    },
    async execute(args, exec) {
      const root = await resolveRoot(ctx, exec, settings)
      const cwd = await resolveAuthorizedGitCwd(ctx, root, args.cwd, exec)
      const result = await runCommand(ctx, {
        command: settings.gitCommand,
        args: ['status', '--porcelain=v1', '--branch', '-z'],
        cwd,
        signal: exec.signal,
        timeoutMs: 30_000,
        // A denied or failing DSH Host subprocess may never silently retry as Node.
        hostOnly: Boolean(lookupDshHostFs(ctx)) || isSubprocessMounted(ctx),
      })
      assertGitExit(result, 'code_git_status')
      if (result.lossy) throw new Error('code_git_status: output truncated; refusing partial status')
      return parseGitStatus(result.stdout)
    },
  })
}

export function createGitDiffTool(ctx: Context, settings: Settings): ToolDefinition {
  return defineTool({
    name: `${TOOL_PREFIX}git_diff`,
    description:
      'Return a `git diff` for the workspace. Set `staged` to true for `--cached`, and `path` to limit to one file.',
    parameters: {
      cwd: { type: 'string', description: 'Optional subdirectory inside the workspace.' },
      path: { type: 'string', description: 'Optional path to diff, relative to the workspace.' },
      staged: { type: 'boolean', description: 'Diff staged (cached) changes.' },
    },
    output: {
      schema: {
        type: 'object',
        additionalProperties: false,
        properties: {
          diff: { type: 'string', required: true, description: 'The raw diff text.' },
          truncated: { type: 'boolean', required: true, description: 'Whether output was capped.' },
        },
      },
      render(_args, value) {
        const head = value.truncated ? 'git diff (truncated):' : 'git diff:'
        return [{ type: 'text', text: `${head}\n${value.diff}` }]
      },
    },
    async execute(args, exec) {
      const root = await resolveRoot(ctx, exec, settings)
      const cwd = await resolveAuthorizedGitCwd(ctx, root, args.cwd, exec)
      const gitArgs = ['diff', '--no-color']
      if (args.staged) gitArgs.push('--cached')
      if (args.path) {
        const pathspec = await resolveDiffPath(ctx, root, args.cwd, args.path, exec.signal)
        gitArgs.push('--', pathspec)
      }
      const result = await runCommand(ctx, {
        command: settings.gitCommand,
        args: gitArgs,
        cwd,
        signal: exec.signal,
        timeoutMs: 30_000,
        maxOutputBytes: 256 * 1024,
        hostOnly: Boolean(lookupDshHostFs(ctx)) || isSubprocessMounted(ctx),
      })
      assertGitExit(result, 'code_git_diff')
      return {
        diff: result.stdout,
        truncated: result.lossy,
      }
    },
  })
}
