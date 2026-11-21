import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition, ToolRunContext } from '@deepseek-ai/dsh-tools'
import { registerTool } from '@fongap-labs/delta-plugin-shared'
import type { DeltaCodeRegistrar } from '../../../delta-code-kernel/feature.js'
import type { ExecutionStats } from '../../../delta-code-support/execution-stats.js'

/** Keep structured batch evidence internally, but propagate failed batches as
 * Host tool errors. A partially applied batch must never look like success. */
function withDshFailureResult(definition: ToolDefinition): ToolDefinition {
  if (definition.name !== 'code_multi_edit') return definition
  return { ...definition, async execute(args: any, exec: ToolRunContext) {
    const value = await definition.execute(args, exec)
    const batch = value as unknown as { results?: Array<{ ok?: boolean }>; rollback?: { failed?: number; skippedReason?: string } }
    if (batch.results?.some(result => result.ok === false) ||
        (batch.rollback?.failed ?? 0) > 0 || batch.rollback?.skippedReason) {
      throw new Error('MultiEdit did not complete successfully; inspect per-edit outcomes before retrying: ' + JSON.stringify(value))
    }
    return value
  } } as ToolDefinition
}

/** Only the DSH provider knows how to attach tools to the Host. */
export function dshToolRegistrar(
  ctx: Context, stats: ExecutionStats,
): DeltaCodeRegistrar<ToolDefinition> {
  return {
    register(definition) {
      const tool = stats.wrap(withDshFailureResult(definition))
      const result = registerTool(ctx, tool)
      if (!result.registered) {
        const error = result.error ?? new Error('Host tool registry rejected "' + tool.name + '"')
        ctx.logger?.warn?.('dsh-delta-code: tool "' + tool.name +
          '" was not registered: ' + error.message)
        throw error
      }
      if (typeof result.disposer === 'function') return result.disposer
    },
  }
}
