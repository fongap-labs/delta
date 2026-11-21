import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import { defineTool } from '@fongap-labs/delta-plugin-shared'
import {
  CODING_TASK_KINDS, CODING_SCOPES, CODING_RISKS, DEFAULT_CODING_WORKFLOW_RULES,
  REQUIRED_WORKFLOW_STOP_CONDITIONS, type CodingWorkflowRule,
  type CodingTaskKind, type CodingScope, type CodingRisk, type CodingWorkflowMode,
  type WorkflowStepCondition,
} from './workflow-policy.js'

export type { CodingTaskKind, CodingScope, CodingRisk, CodingWorkflowMode, CodingWorkflowRule } from './workflow-policy.js'

export interface CodingMilestone {
  objective: string
  acceptance: string
  dependsOn?: number[]
  writePaths?: string[]
}

export interface RouteInput {
  /** Model-proposed steps, not Host Todo completion evidence. */
  milestones?: CodingMilestone[]
  taskKind: CodingTaskKind
  scope: CodingScope
  risk: CodingRisk
  independentTasks?: number
  nativeSubagents?: boolean
  nativeSubagentProvider?: string
  taskPaths?: string[][]
  taskDependencies?: number[][]
}
export interface RouteResult {
  mode: CodingWorkflowMode
  skills: string[]
  parallelCandidate: boolean
  executionWaves: number[][]
  verification: string[]
  stopConditions: string[]
  milestones?: Array<{ objective: string; acceptance: string; dependsOn: number[]; writePaths: string[] }>
  nativeTodoDraft?: Array<{ content: string; status: 'pending' }>
  acceptanceHints?: Array<{ task: string; criterion: string }>
}

function safeTaskPath(raw: string): string {
  if (typeof raw !== 'string' || raw.length < 1 || raw.length > 240 ||
      raw.startsWith('/') || /^[a-zA-Z]:/.test(raw) || raw.includes('\\') ||
      raw.includes('*') || raw.includes('?') || raw.includes('[') ||
      /[\u0000-\u001f\u007f]/.test(raw) ||
      raw.split('/').some(part => !part || part === '.' || part === '..')) {
    throw new Error('taskPaths must contain safe, explicit repository-relative paths')
  }
  return raw
}

/** Validate all proposed milestones without executing or marking a native Todo completed. */
export function normalizeCodingMilestones(input: readonly CodingMilestone[]): Array<{
  objective: string; acceptance: string; dependsOn: number[]; writePaths: string[]
}> {
  if (!Array.isArray(input) || input.length < 1 || input.length > 20) {
    throw new Error('milestones must contain 1..20 bounded steps')
  }
  const unique = new Set<string>()
  return input.map((step, index) => {
    if (!step || typeof step !== 'object') throw new Error('invalid milestone')
    const clean = (value: unknown, field: string): string => {
      if (typeof value !== 'string' || !value.trim() ||
          value.length > 240 || /[\u0000-\u001f\u007f]/.test(value)) {
        throw new Error('invalid milestone ' + field)
      }
      return value.trim()
    }
    const objective = clean(step.objective, 'objective')
    const acceptance = clean(step.acceptance, 'acceptance')
    if (unique.has(objective)) throw new Error('duplicate milestone objective')
    unique.add(objective)
    const dependsOn = step.dependsOn ?? []
    if (!Array.isArray(dependsOn) || dependsOn.length > index ||
        dependsOn.some(i => !Number.isSafeInteger(i) || i < 0 || i >= index) ||
        new Set(dependsOn).size !== dependsOn.length) {
      throw new Error('milestone dependencies must reference unique earlier indexes')
    }
    const writePaths = step.writePaths ?? []
    if (!Array.isArray(writePaths) || writePaths.length > 32 ||
        writePaths.some(path => typeof path !== 'string')) {
      throw new Error('milestone writePaths must be bounded explicit relative paths')
    }
    const canonical = writePaths.map(path => safeTaskPath(path).normalize('NFC').toLowerCase())
    if (new Set(canonical).size !== canonical.length) {
      throw new Error('duplicate milestone write path')
    }
    return { objective, acceptance, dependsOn: [...dependsOn], writePaths: [...writePaths] }
  })
}

/**
 * A lossless, read-only handoff to DSH native todo_write and the delivery-gate
 * acceptance vocabulary. No Host state is written; checks require fresh execution.
 */
export function draftNativeTodoHandoff(
  milestones: readonly { objective: string; acceptance: string }[],
): {
  nativeTodoDraft: Array<{ content: string; status: 'pending' }>
  acceptanceHints: Array<{ task: string; criterion: string }>
} {
  const tasks = milestones.map(item => {
    // DSH native Todo limits task content to 200 chars. Never silently truncate
    // an objective: delivery-gate acceptance requires exact content matching.
    if (item.objective.length > 200) {
      throw new Error('milestone objective exceeds native Todo 200-character limit')
    }
    return item.objective
  })
  return {
    nativeTodoDraft: tasks.map(content => ({ content, status: 'pending' as const })),
    acceptanceHints: milestones.map(item => ({ task: item.objective, criterion: item.acceptance })),
  }
}

function overlaps(left: string, right: string): boolean {
  return left === right || left.startsWith(right + '/') || right.startsWith(left + '/')
}

/** Pure dependency-wave planning: never dispatches agents or touches a worktree. */
export function planCodingWaves(
  count: number, paths?: string[][], dependencies?: number[][], canParallelize = false,
): number[][] {
  if (!Number.isSafeInteger(count) || count < 1 || count > 100) {
    throw new Error('independentTasks must be an integer from 1 to 100')
  }
  if (paths !== undefined &&
      (!Array.isArray(paths) || paths.length !== count ||
       paths.some(list => !Array.isArray(list) || list.length < 1 || list.length > 32))) {
    throw new Error('taskPaths must provide 1..32 explicit write paths for every task')
  }
  // Declared paths are advisory, not Host filesystem identities. Conservative
  // NFC + case folding prevents false-safe parallel writes on case-insensitive
  // worktrees and canonically equivalent Unicode paths across platforms.
  const writeSets = paths?.map(list => {
    const normalized = list.map(raw => safeTaskPath(raw).normalize('NFC').toLowerCase())
    if (new Set(normalized).size !== normalized.length) {
      throw new Error('duplicate task write path')
    }
    return normalized
  })
  if (dependencies !== undefined &&
      (!Array.isArray(dependencies) || dependencies.length !== count ||
       dependencies.some(list => !Array.isArray(list) || list.length > count))) {
    throw new Error('taskDependencies must cover every task')
  }
  const prerequisites = Array.from({ length: count }, (_, index) => {
    const items = dependencies?.[index] ?? []
    if (items.some(dependency => !Number.isSafeInteger(dependency) ||
        dependency < 0 || dependency >= index) || new Set(items).size !== items.length) {
      throw new Error('taskDependencies must reference distinct, earlier task indexes')
    }
    return items
  })
  const waves: number[][] = []
  const assigned: number[] = []
  for (let index = 0; index < count; index++) {
    const earliest = prerequisites[index]!.reduce((wave, dependency) =>
      Math.max(wave, assigned[dependency]! + 1), 0)
    let waveIndex = canParallelize && writeSets ? earliest : Math.max(earliest, waves.length)
    while (waves[waveIndex]?.some(other =>
      writeSets && writeSets[index]!.some(path =>
        writeSets[other]!.some(existing => overlaps(path, existing))))) {
      waveIndex++
    }
    if (!waves[waveIndex]) waves[waveIndex] = []
    waves[waveIndex]!.push(index)
    assigned[index] = waveIndex
  }
  return waves
}

/**
 * Data selects the workflow; this function only enforces safety invariants and
 * computes a bounded dependency-wave plan. It never dispatches agents.
 */
export function decideCodingWorkflow(
  input: RouteInput,
  rules: readonly CodingWorkflowRule[] = DEFAULT_CODING_WORKFLOW_RULES,
): RouteResult {
  if (!CODING_TASK_KINDS.includes(input.taskKind) ||
      !CODING_SCOPES.includes(input.scope) ||
      !CODING_RISKS.includes(input.risk)) {
    throw new Error('invalid taskKind, scope or risk')
  }
  const milestones = input.milestones === undefined ? undefined : normalizeCodingMilestones(input.milestones)
  if (milestones && (input.taskPaths !== undefined || input.taskDependencies !== undefined ||
      (input.independentTasks !== undefined && input.independentTasks !== milestones.length))) {
    throw new Error('milestones are exclusive with separate taskPaths/taskDependencies and inconsistent counts')
  }
  const independentTasks = milestones?.length ?? input.independentTasks ?? 1
  if (!Number.isSafeInteger(independentTasks) || independentTasks < 1 || independentTasks > 100) {
    throw new Error('independentTasks must be an integer from 1 to 100')
  }
  const isHighImpact = input.risk === 'high' || input.scope === 'large'
  const isPlanned = isHighImpact || input.scope === 'medium'
  const profile = rules.find(rule =>
    rule.taskKinds.includes(input.taskKind) &&
    (rule.shouldPlan === undefined || rule.shouldPlan === isPlanned))
  if (!profile) throw new Error('no reviewed coding workflow policy matches the task')

  // A catalog cannot permit unsafe parallel execution, even if canParallelize
  // was mistakenly enabled for a diagnostic or read-only workflow.
  const isParallelPermitted = profile.mode === 'planned' &&
    profile.canParallelize === true && isPlanned &&
    (input.taskKind === 'feature' || input.taskKind === 'refactor') &&
    independentTasks >= 2 && input.nativeSubagents === true
  const plannedPaths = milestones && milestones.every(item => item.writePaths.length > 0)
    ? milestones.map(item => item.writePaths) : input.taskPaths
  const executionWaves = planCodingWaves(independentTasks,
    plannedPaths, milestones?.map(item => item.dependsOn) ?? input.taskDependencies, isParallelPermitted)
  const canParallelize = executionWaves.some(wave => wave.length > 1)
  const conditions: Record<WorkflowStepCondition, boolean> = {
    planned: isPlanned,
    'high-impact': isHighImpact,
    parallel: canParallelize,
    sequential: !canParallelize,
  }
  const skills = profile.steps
    .filter(step => (step.when === undefined || conditions[step.when]) &&
      (canParallelize || (step.skill !== 'code-parallel-work' &&
        step.skill !== 'code-subagent-delivery')))
    .map(step => step.skill)

  return {
    mode: profile.mode,
    skills,
    parallelCandidate: canParallelize,
    executionWaves,
    verification: [...profile.verification],
    stopConditions: [...REQUIRED_WORKFLOW_STOP_CONDITIONS],
    ...(milestones ? { milestones, ...draftNativeTodoHandoff(milestones) } : {}),
  }
}

export function createWorkflowRouteTool(
  rules: readonly CodingWorkflowRule[] = DEFAULT_CODING_WORKFLOW_RULES,
  canDelegate?: (agent: unknown, provider: string) => boolean,
): ToolDefinition {
  return defineTool({
    name: 'code_workflow_route',
    description: 'Choose a deterministic coding workflow. Optional milestones produce pending native Todo and exact acceptance hints for the existing Host and delivery gate; advisory only, does not call native todo_write, execute commands or mark tasks complete. Tiny changes can load code-fast-change directly.',
    parameters: {
      taskKind: { type: 'string', required: true, enum: [...CODING_TASK_KINDS] },
      scope: { type: 'string', required: true, enum: [...CODING_SCOPES] },
      risk: { type: 'string', required: true, enum: [...CODING_RISKS] },
      independentTasks: { type: 'integer', description: 'Number of independently reviewable work items, 1..100; default 1.' },
      nativeSubagents: { type: 'boolean', description: 'Request native delegation; a true value is not capability evidence. The Host adapter must confirm it.' },
      nativeSubagentProvider: { type: 'string', description: 'Exact registered DSH subagent provider name, required to consider native parallel waves.' },
      taskPaths: { type: 'array', description: 'Explicit repository-relative write sets for each task, required for parallel execution.',
        items: { type: 'array', items: { type: 'string' } } },
      taskDependencies: { type: 'array', description: 'Earlier zero-based prerequisite indexes for each task.',
        items: { type: 'array', items: { type: 'integer' } } },
      milestones: { type: 'array', description: 'Optional 1..20 model-proposed task steps with observable acceptance. No status/permission claims; exclusive with taskPaths/taskDependencies.',
        items: { type: 'object', additionalProperties: false, properties: {
          objective: { type: 'string', required: true },
          acceptance: { type: 'string', required: true },
          dependsOn: { type: 'array', items: { type: 'integer' } },
          writePaths: { type: 'array', items: { type: 'string' } },
        } } },
    },
    output: { schema: { type: 'object', additionalProperties: false, properties: {
      mode: { type: 'string', required: true },
      skills: { type: 'array', required: true, items: { type: 'string' } },
      parallelCandidate: { type: 'boolean', required: true },
      executionWaves: { type: 'array', required: true, items: { type: 'array', items: { type: 'integer' } } },
      verification: { type: 'array', required: true, items: { type: 'string' } },
      stopConditions: { type: 'array', required: true, items: { type: 'string' } },
      nativeTodoDraft: { type: 'array', items: { type: 'object', additionalProperties: false, properties: {
        content: { type: 'string', required: true },
        status: { type: 'string', required: true },
      } } },
      acceptanceHints: { type: 'array', items: { type: 'object', additionalProperties: false, properties: {
        task: { type: 'string', required: true },
        criterion: { type: 'string', required: true },
      } } },
      milestones: { type: 'array', items: { type: 'object', additionalProperties: false, properties: {
        objective: { type: 'string', required: true },
        acceptance: { type: 'string', required: true },
        dependsOn: { type: 'array', required: true, items: { type: 'integer' } },
        writePaths: { type: 'array', required: true, items: { type: 'string' } },
      } } },
    } }, render(_args, value) {
      return [{ type: 'text', text: [
        'Coding route: ' + value.mode,
        'Load in order (only when relevant): ' + value.skills.join(' -> '),
        'Parallel candidate: ' + (value.parallelCandidate ? 'yes (disjoint write sets)' : 'no'),
        ...value.executionWaves.map((wave, index) => 'Execution wave ' + (index + 1) + ': ' +
          wave.map(task => '#' + task).join(', ')),
        ...value.milestones?.map((item, index) => 'Proposed milestone #' + index + ': ' + item.objective +
          ' | acceptance: ' + item.acceptance + ' | prerequisites: ' + item.dependsOn.join(', ')) ?? [],
        ...(value.nativeTodoDraft ? [
          'Native Todo handoff draft: write pending tasks through authorized DSH todo_write, then verify native state. Never treat this draft as completed work.',
          ...value.nativeTodoDraft.map((item, index) => 'Todo #' + index + ': ' + item.content + ' [pending]'),
          'Acceptance hints map by exact Todo content; delivery gate still requires real executed checks.',
        ] : []),
        ...value.verification.map(line => 'Verify: ' + line),
        ...value.stopConditions.map(line => 'Stop: ' + line),
      ].join('\n') }]
    } },
    execute: async (args, exec) => {
      const input = args as unknown as RouteInput
      // Model-authored input cannot assert native Host availability. The
      // adapter validates the exact live Agent and named Host provider.
      const isVerified = input.nativeSubagents === true &&
        typeof input.nativeSubagentProvider === 'string' &&
        canDelegate?.(exec.agent, input.nativeSubagentProvider) === true
      return decideCodingWorkflow({ ...input, nativeSubagents: isVerified }, rules)
    },
  })
}
