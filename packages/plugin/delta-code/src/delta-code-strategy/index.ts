import type { DeltaCodeFeature } from '../delta-code-kernel/feature.js'
import { createWorkflowRouteTool } from './workflow-route.js'
import { createCodingCapabilitiesTool } from './capability-catalog.js'
import { createExperienceCandidateTool } from './experience-candidate.js'
import { createTaskSpecTool, type UserTaskContext } from './intent-contract.js'
import type { CodingWorkflowRule } from './workflow-policy.js'

export interface DeltaCodeStrategyBindings {
  /** Optional trusted policy injection, never hydrated from model/tool content. */
  workflowRules?: readonly CodingWorkflowRule[]
  canDelegate?: (agent: unknown, provider: string) => boolean
  taskContext: UserTaskContext
  registerSkills(): () => void
  registerRouter(): () => void
  registerPrompt(): () => void
}

export function deltaCodeStrategy(
  bindings: DeltaCodeStrategyBindings,
): DeltaCodeFeature<ReturnType<typeof createWorkflowRouteTool>> {
  return {
    id: 'delta-code-strategy',
    tools: [createWorkflowRouteTool(bindings.workflowRules, bindings.canDelegate), createCodingCapabilitiesTool(), createExperienceCandidateTool(), createTaskSpecTool(bindings.taskContext)],
    activate() {
      const releases: Array<() => void> = []
      try {
        releases.push(bindings.registerSkills())
        releases.push(bindings.registerRouter())
        releases.push(bindings.registerPrompt())
      } catch (error) {
        for (const release of releases.reverse()) release()
        throw error
      }
      return () => { for (const release of releases.reverse()) release() }
    },
  }
}
