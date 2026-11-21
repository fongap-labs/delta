import type { DeltaCodeTool } from '../delta-code-adapter/contracts/tools.js'
import type { DeltaCodeFeature } from '../delta-code-kernel/feature.js'

/** Host-independent Coding composition. Tool construction is owned by adapters. */
export function deltaCodeCodingWithoutCommands<T extends DeltaCodeTool>(
  tools: readonly T[],
): DeltaCodeFeature<T> {
  return { id: 'delta-code-coding-core', tools }
}

export function deltaCodeCodingCommands<T extends DeltaCodeTool>(
  tools: readonly T[],
): DeltaCodeFeature<T> {
  return { id: 'delta-code-coding-commands', tools }
}

/** Optional combined factory for consumers that need the complete feature. */
export function deltaCodeCoding<T extends DeltaCodeTool>(
  coreTools: readonly T[], commandTools: readonly T[],
): DeltaCodeFeature<T> {
  return { id: 'delta-code-coding', tools: [...coreTools, ...commandTools] }
}
