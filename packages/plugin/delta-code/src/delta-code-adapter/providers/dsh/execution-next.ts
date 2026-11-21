import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import { defineTool } from '@fongap-labs/delta-plugin-shared'
import type { NativeResumeReader } from '../../../delta-code-quality/resume-policy.js'
import { decideExecutionTransition } from '../../../delta-code-quality/execution-convergence.js'
import { compactionReview, type NativeCompactionReader } from '../../../delta-code-quality/compaction-continuity.js'
import type { ExecutionStats } from '../../../delta-code-support/execution-stats.js'

/**
 * A read-only convergence advisory, never a second agent scheduler. The source
 * of task progress is DSH native sessionProjections, not model arguments.
 */
export function createExecutionNextTool(
  resume: NativeResumeReader, stats: ExecutionStats, compaction?: NativeCompactionReader,
): ToolDefinition {
  return defineTool({
    name: 'code_execution_next',
    description: 'Suggest the next safe coding action using actual native Todo projection and session-scoped observed Delta verification/error events. Does not replay writes, run commands, certify delivery, or bypass Host gates.',
    parameters: {},
    output: { schema: { type: 'object', additionalProperties: false, properties: {
      transition: { type: 'string', required: true },
      nextAction: { type: 'string', required: true },
      remaining: { type: 'array', required: true, items: { type: 'string' } },
      observation: { type: 'string', required: true },
      deliveryConfirmed: { type: 'boolean', required: true },
      contextReviewState: { type: 'string', required: true },
      contextSummarySeq: { type: 'integer', required: true },
      contextTodoRefreshed: { type: 'boolean', required: true },
      contextReviewAction: { type: 'string', required: true },
    } }, render(_args, result) {
      return [{ type: 'text', text: [
        'Execution transition: ' + result.transition,
        'Observed signal: ' + result.observation,
        'Next: ' + result.nextAction,
        ...result.remaining.slice(0, 8).map(item => 'Pending: ' + item),
        ...(result.contextReviewState !== 'none' ? [
          'Compaction context review: ' + result.contextReviewState,
          ...(result.contextReviewAction ? [result.contextReviewAction] : []),
        ] : []),
        'Advisory only; never an independent test result, Host authorization or delivery certificate.',
      ].join('\n') }]
    } },
    async execute(_args, exec) {
      let state
      try { state = resume.read(exec.agent) } catch {
        // A corrupt or missing Host projection must fail closed.
        state = undefined
      }
      let marker
      try { marker = compaction?.read(exec.agent) } catch { marker = undefined }
      const review = compactionReview(marker, state?.writeSeq ?? -1)
      const decision = decideExecutionTransition(state, stats.readLastObservation(exec.agent))
      return {
        ...decision,
        contextReviewState: review.state,
        contextSummarySeq: review.lastSummarySeq,
        contextTodoRefreshed: review.todoUpdatedAfterSummary,
        contextReviewAction: review.action,
      }
    },
  })
}
