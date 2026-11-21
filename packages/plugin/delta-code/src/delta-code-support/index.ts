import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import type { DeltaCodeFeature } from '../delta-code-kernel/feature.js'
import { ExecutionStats, createExecutionStatsTool } from './execution-stats.js'

export function deltaCodeSupport(stats: ExecutionStats): DeltaCodeFeature<ToolDefinition> {
  return {
    id: 'delta-code-support',
    tools: [createExecutionStatsTool(stats)],
  }
}
