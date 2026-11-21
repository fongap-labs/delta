/** Host-independent delivery state, acceptance and result policy. */
import type { VerificationScript } from './verify-presets.js'

const STATUS = new Set(['pending', 'in_progress', 'completed'])
// The gate itself is not an implementation task. Legacy plans sometimes put
// it in the final native Todo, which otherwise makes verification recursive.
export function isTerminalGateTodo(tasks: DeliveryTask[], remaining: string[]): boolean {
  if (remaining.length !== 1 || tasks.length < 2) return false
  const final = tasks[tasks.length - 1]!
  if (final.status === 'completed' || final.content.trim() !== remaining[0]) return false
  if (!tasks.slice(0, -1).every(task => task.status === 'completed')) return false
  return /(?:\u4ea4\u4ed8.{0,16}(?:\u9a8c\u8bc1|\u95e8\u7981)|(?:\u6700\u7ec8|\u6700\u540e).{0,20}(?:\u9a8c\u8bc1|\u9a8c\u6536|\u95e8\u7981)|(?:\u9a8c\u8bc1|\u9a8c\u6536).{0,12}\u95e8\u7981|(?:delivery|verification|acceptance).{0,16}(?:gate|check)|(?:gate|check).{0,16}(?:delivery|verification))/i.test(final.content)
}

export interface DeliveryTask {
  content: string
  status: 'pending' | 'in_progress' | 'completed'
}

/**
 * Pure progress assessment. The actual task list remains owned by DSH native
 * todo_write and the Host session, not by a second plugin-side task store.
 */
export function remainingDeliveryTasks(tasks: DeliveryTask[]): string[] {
  if (!Array.isArray(tasks) || tasks.length < 1 || tasks.length > 32) {
    throw new Error('delivery gate requires 1..32 tasks')
  }
  const seen = new Set<string>()
  return tasks.map(task => {
    if (!task || typeof task.content !== 'string' || task.content.trim().length === 0 ||
        task.content.length > 200 || !STATUS.has(task.status)) {
      throw new Error('delivery task content/status is invalid')
    }
    const content = task.content.trim()
    if (seen.has(content)) throw new Error('duplicate delivery task')
    seen.add(content)
    return task.status === 'completed' ? undefined : content
  }).filter((task): task is string => task !== undefined)
}

export interface GateCheck {
  script: string
  exitCode: number
  passed: boolean
}

export interface TaskAcceptance {
  task: string
  criterion: string
  checks: VerificationScript[]
}

export interface TaskEvidence {
  task: string
  criterion: string
  checks: GateCheck[]
}

/** Maps every native Todo task to a declared criterion and fresh allowed checks. */
export function validateTaskAcceptance(
  tasks: DeliveryTask[], scripts: VerificationScript[], raw: unknown,
): TaskAcceptance[] {
  if (!Array.isArray(raw) || raw.length !== tasks.length) {
    throw new Error('task acceptance must cover every native Todo item exactly once')
  }
  return raw.map((entry: unknown, index): TaskAcceptance => {
    if (!entry || typeof entry !== 'object') throw new Error('invalid task acceptance entry')
    const item = entry as Partial<TaskAcceptance>
    const task = tasks[index]!.content.trim()
    if (item.task !== task) throw new Error('task acceptance order/content differs from native Todo')
    if (typeof item.criterion !== 'string' || item.criterion.trim().length < 3 ||
        item.criterion.length > 240) throw new Error('task acceptance criterion is missing or invalid')
    if (!Array.isArray(item.checks) || item.checks.length < 1 || item.checks.length > scripts.length ||
        new Set(item.checks).size !== item.checks.length ||
        item.checks.some(check => !scripts.includes(check))) {
      throw new Error('task acceptance checks must be distinct executed final scripts')
    }
    return { task, criterion: item.criterion.trim(), checks: [...item.checks] }
  })
}

export interface GateResult {
  stage: 'continue' | 'blocked' | 'verified'
  ready: boolean
  head: string
  nextAction: string
  remaining: string[]
  blockers: string[]
  changes: string[]
  checks: GateCheck[]
  evidence: string[]
  taskEvidence: TaskEvidence[]
}

/** Delivery claims cannot be constructed from completed Todos alone. */
export function hasCompleteDeliveryEvidence(
  remaining: readonly string[], blockers: readonly string[],
  checks: readonly GateCheck[], evidence: readonly string[],
  taskEvidence: readonly TaskEvidence[],
): boolean {
  if (remaining.length || blockers.length || !checks.length || !taskEvidence.length || !evidence.length) return false
  if (checks.some(check => !check.passed || check.exitCode !== 0)) return false
  if (new Set(checks.map(check => check.script)).size !== checks.length) return false
  return taskEvidence.every(item =>
    Boolean(item.task.trim() && item.criterion.trim()) && item.checks.length > 0 &&
    item.checks.every(check => check.passed && check.exitCode === 0 &&
      checks.some(actual => actual.script === check.script && actual.passed && actual.exitCode === check.exitCode)))
}

/** A missing certificate must be blocked, never modeled as a verified delivery. */
export function gate(
  stage: GateResult['stage'], nextAction: string, remaining: string[] = [],
  head = '', blockers: string[] = [], changes: string[] = [],
  checks: GateCheck[] = [], evidence: string[] = [], taskEvidence: TaskEvidence[] = [],
): GateResult {
  const verified = stage === 'verified' && hasCompleteDeliveryEvidence(
    remaining, blockers, checks, evidence, taskEvidence,
  )
  if (stage === 'verified' && !verified) {
    return {
      stage: 'blocked', ready: false, head,
      nextAction: 'Final delivery requires fresh, complete checks and mapped task acceptance evidence.',
      remaining, blockers: [...blockers, 'final delivery evidence incomplete'],
      changes, checks, evidence, taskEvidence,
    }
  }
  return { stage, ready: verified, head, nextAction, remaining, blockers, changes, checks, evidence, taskEvidence }
}
