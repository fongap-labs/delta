import type { DeltaCodeTool } from '../delta-code-adapter/contracts/tools.js'
import type { DeltaCodeFeature } from '../delta-code-kernel/feature.js'

/** Remote GitHub operations can be provided independently of local subprocess. */
export function deltaCodeRepositoryRemote<T extends DeltaCodeTool>(
  tools: readonly T[],
): DeltaCodeFeature<T> {
  return { id: 'delta-code-repository-remote', tools }
}

/** Local Git, worktree and workspace operations depend on host command capabilities. */
export function deltaCodeRepositoryCommands<T extends DeltaCodeTool>(
  tools: readonly T[],
): DeltaCodeFeature<T> {
  return { id: 'delta-code-repository-commands', tools }
}

export function deltaCodeRepository<T extends DeltaCodeTool>(
  commandTools: readonly T[], remoteTools: readonly T[],
): DeltaCodeFeature<T> {
  return { id: 'delta-code-repository', tools: [...commandTools, ...remoteTools] }
}
