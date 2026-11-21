/** Reviewed workflow descriptions. No Host SDK, subprocess, model call or agent runtime. */
export const CODING_TASK_KINDS = ['feature', 'bugfix', 'refactor', 'review', 'ci', 'research'] as const
export const CODING_SCOPES = ['small', 'medium', 'large'] as const
export const CODING_RISKS = ['low', 'medium', 'high'] as const

export type CodingTaskKind = (typeof CODING_TASK_KINDS)[number]
export type CodingScope = (typeof CODING_SCOPES)[number]
export type CodingRisk = (typeof CODING_RISKS)[number]
export type CodingWorkflowMode = 'fast' | 'diagnose' | 'planned' | 'review' | 'ci-repair' | 'research'
export type WorkflowStepCondition = 'planned' | 'high-impact' | 'parallel' | 'sequential'

export interface WorkflowSkillStep {
  readonly skill: string
  /** Context gates are evaluated by Delta, never by a model or remote Skill. */
  readonly when?: WorkflowStepCondition
}

export interface CodingWorkflowRule {
  readonly id: string
  readonly taskKinds: readonly CodingTaskKind[]
  readonly shouldPlan?: boolean
  readonly mode: CodingWorkflowMode
  readonly steps: readonly WorkflowSkillStep[]
  /** Only reviewed, safe workflows may opt into native Host parallel candidates. */
  readonly canParallelize?: boolean
  readonly verification: readonly string[]
}

/** Mandatory independent safety conditions are not replaceable by a workflow catalog. */
export const REQUIRED_WORKFLOW_STOP_CONDITIONS: readonly string[] = [
  'Host permissions or credentials unavailable: stop, do not silently fall back or escalate.',
  'Destructive or irreversible action: require explicit authorization.',
  'Tests fail, required CI is red/pending, or evidence is missing: do not claim verified completion.',
  'Parallel tasks require verified native subagents and disjoint declared write paths.',
]

export const STANDARD_WORKFLOW_VERIFICATION: readonly string[] = [
  'Inspect changed paths and relevant source/test diffs.',
  'Run focused behavior tests and applicable project checks; report exact commands and exit codes.',
  'For a PR, verify required checks against its current HEAD SHA.',
]

export const RESEARCH_WORKFLOW_VERIFICATION: readonly string[] = [
  'Cite the source files and facts supporting each conclusion; distinguish hypotheses.',
]

/**
 * Ordered first-match profiles. Editing this catalog changes workflow
 * composition without touching the routing engine or Host adapter.
 * Profiles are trusted code-reviewed data, not runtime JSON or model output.
 */
export const DEFAULT_CODING_WORKFLOW_RULES: readonly CodingWorkflowRule[] = [
  {
    id: 'ci-repair',
    taskKinds: ['ci'],
    mode: 'ci-repair',
    steps: [{ skill: 'code-ci-repair' }, { skill: 'code-verification-gate' }],
    verification: STANDARD_WORKFLOW_VERIFICATION,
  },
  {
    id: 'read-only-review',
    taskKinds: ['review'],
    mode: 'review',
    steps: [{ skill: 'code-review-readiness' }, { skill: 'code-quality-strength', when: 'high-impact' }, { skill: 'code-verification-gate' }],
    verification: STANDARD_WORKFLOW_VERIFICATION,
  },
  {
    id: 'research',
    taskKinds: ['research'],
    mode: 'research',
    steps: [{ skill: 'code-context-discipline' }, { skill: 'code-brainstorming' }],
    verification: RESEARCH_WORKFLOW_VERIFICATION,
  },
  {
    id: 'root-cause-before-edit',
    taskKinds: ['bugfix'],
    mode: 'diagnose',
    steps: [
      { skill: 'code-root-cause-debugging' },
      { skill: 'code-writing-plans', when: 'planned' },
      { skill: 'code-executing-plans', when: 'planned' },
      { skill: 'code-test-driven-development' },
      { skill: 'code-quality-strength', when: 'high-impact' },
      { skill: 'code-diagnostic-readiness', when: 'high-impact' },
      { skill: 'code-request-review', when: 'high-impact' },
      { skill: 'code-verification-gate' },
      { skill: 'code-finish-branch' },
    ],
    verification: STANDARD_WORKFLOW_VERIFICATION,
  },
  {
    id: 'small-change',
    taskKinds: ['feature', 'refactor'],
    shouldPlan: false,
    mode: 'fast',
    steps: [{ skill: 'code-fast-change' }, { skill: 'code-verification-gate' }],
    verification: STANDARD_WORKFLOW_VERIFICATION,
  },
  {
    id: 'planned-delivery',
    taskKinds: ['feature', 'refactor'],
    shouldPlan: true,
    mode: 'planned',
    canParallelize: true,
    steps: [
      { skill: 'code-brainstorming', when: 'high-impact' },
      { skill: 'code-writing-plans' },
      { skill: 'code-parallel-work', when: 'parallel' },
      { skill: 'code-subagent-delivery', when: 'parallel' },
      { skill: 'code-executing-plans', when: 'sequential' },
      { skill: 'code-test-driven-development' },
      { skill: 'code-quality-strength', when: 'high-impact' },
      { skill: 'code-diagnostic-readiness', when: 'high-impact' },
      { skill: 'code-request-review', when: 'high-impact' },
      { skill: 'code-review-readiness', when: 'high-impact' },
      { skill: 'code-verification-gate' },
      { skill: 'code-finish-branch' },
    ],
    verification: STANDARD_WORKFLOW_VERIFICATION,
  },
]
