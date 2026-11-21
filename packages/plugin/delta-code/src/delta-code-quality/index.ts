import type { DeltaCodeTool } from '../delta-code-adapter/contracts/tools.js'
import type { DeltaCodeFeature } from '../delta-code-kernel/feature.js'

/** Host-independent Quality composition; the DSH adapter owns tool creation. */
export function deltaCodeQuality<T extends DeltaCodeTool>(
  tools: readonly T[],
): DeltaCodeFeature<T> {
  return { id: 'delta-code-quality', tools }
}
