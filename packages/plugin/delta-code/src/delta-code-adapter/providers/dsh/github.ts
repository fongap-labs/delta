/** DSH-specific GitHub tool registration and optional local Origin discovery. */
import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition, ToolRunContext } from '@deepseek-ai/dsh-tools'
import { defineTool, runCommand, isSubprocessMounted } from '@fongap-labs/delta-plugin-shared'
import type { Settings } from '../../../types.js'
import { resolveRoot } from '../../../runtime.js'
import { resolveAuthorizedGitCwd } from './host-git-cwd.js'
import { hasCompleteGitEvidence } from '../../../delta-code-repository/git-execution.js'
import { isValidGitHubRepository, repositoryFromRemote } from '../../../delta-code-repository/github-policy.js'
import { createGitHubToolSpecs } from '../../../delta-code-repository/github-operations.js'
import type { DeltaGitHubExecution } from '../../../delta-code-repository/github-operations.js'
import type { DeltaGitHubTransport } from '../../contracts/github.js'

async function resolveRepository(ctx: Context, exec: ToolRunContext, settings: Settings, supplied?: string): Promise<string> {
  if (supplied) {
    if (!isValidGitHubRepository(supplied)) throw new Error('repository must be owner/name')
    return supplied
  }
  const root = await resolveRoot(ctx, exec, settings)
  const cwd = await resolveAuthorizedGitCwd(ctx, root, undefined, exec)
  const result = await runCommand(ctx, {
    command: settings.gitCommand,
    args: ['remote', 'get-url', 'origin'],
    cwd, signal: exec.signal, timeoutMs: 10_000, maxOutputBytes: 8 * 1024,
    // If the workspace belongs to a mounted Host FS, never call local Git
    // after the Host subprocess rejects or is unavailable.
    hostOnly: Boolean(ctx.get('fs')) || isSubprocessMounted(ctx),
  })
  if (!hasCompleteGitEvidence(result) || exec.signal.aborted) {
    throw new Error('GitHub origin inspection failed or returned incomplete Host evidence')
  }
  const repo = repositoryFromRemote(result.stdout)
  if (!repo) throw new Error('cannot detect a GitHub origin; supply repository=owner/name')
  return repo
}

const responseSchema = {
  type: 'object', additionalProperties: false, properties: {
    repository: { type: 'string', required: true },
    account: { type: 'string', required: true },
    result: { type: 'string', required: true },
  },
} as const

export function createGitHubTools(
  ctx: Context, settings: Settings, transport: DeltaGitHubTransport,
): ToolDefinition[] {
  const resolveRepo = (supplied: string | undefined, exec: DeltaGitHubExecution) =>
    resolveRepository(ctx, exec as ToolRunContext, settings, supplied)
  return createGitHubToolSpecs(settings, transport, resolveRepo).map(spec => defineTool({
    name: spec.name,
    description: spec.description,
    parameters: spec.parameters,
    output: {
      schema: responseSchema,
      render(_args, value) { return [{ type: 'text', text: value.result }] },
    },
    async execute(args, exec) { return spec.execute(args, exec) },
  }))
}
