/** DSH-independent Todo event folding, checkpoint validation and resume result policy. */
import { remainingDeliveryTasks, type DeliveryTask } from './delivery-policy.js'

export type ResumeState = { tasks: DeliveryTask[] | null; writeSeq: number; invalid: boolean }
export type SessionEvent = { type: string; seq: number; data?: unknown }
export const EMPTY: ResumeState = { tasks: null, writeSeq: -1, invalid: false }
export const HEAD_PATTERN = /^(?:[a-f0-9]{40}|[a-f0-9]{64})$/

function parseTasks(raw: unknown): DeliveryTask[] | null {
  if (!Array.isArray(raw) || raw.length > 32) return null
  if (raw.length === 0) return []
  try {
    const tasks = raw.map(value => ({
      content: (value as DeliveryTask).content,
      status: (value as DeliveryTask).status,
    }))
    remainingDeliveryTasks(tasks)
    return tasks.map(task => ({ content: task.content.trim(), status: task.status }))
  } catch { return null }
}

/** Native event fold; never clears on turn/start and never trusts stale events. */
export function foldNativeResume(state: ResumeState, event: SessionEvent): ResumeState {
  if (event.type !== 'todo/write') return state
  // A reordered, duplicated or malformed Todo event must not replace a newer
  // checkpoint with old task assertions. Fail closed until a newer valid event.
  if (!Number.isSafeInteger(event.seq) || event.seq < 0 || event.seq <= state.writeSeq) {
    return { tasks: null, writeSeq: state.writeSeq, invalid: true }
  }
  const payload = event.data as { todos?: unknown } | null | undefined
  const tasks = parseTasks(payload?.todos)
  return { tasks, writeSeq: event.seq, invalid: tasks === null }
}

export function parseResumeState(value: unknown): ResumeState {
  if (!value || typeof value !== 'object') throw new Error('invalid native resume projection')
  const state = value as ResumeState
  if (!Number.isSafeInteger(state.writeSeq) || state.writeSeq < -1 ||
      typeof state.invalid !== 'boolean') throw new Error('invalid native resume checkpoint')
  if (state.tasks === null) {
    if (state.writeSeq >= 0 && !state.invalid) throw new Error('missing native Todo snapshot')
  } else if (parseTasks(state.tasks) === null || state.invalid) {
    throw new Error('invalid native Todo checkpoint')
  }
  return state
}

/** Check that a model-supplied plan actually reached the native Host Todo. */
export function matchesNativeTodoPlan(state: ResumeState, expectedTasks: unknown): boolean {
  if (state.invalid || !Array.isArray(state.tasks) || !Array.isArray(expectedTasks) ||
      expectedTasks.length < 1 || expectedTasks.length > 32 ||
      state.tasks.length !== expectedTasks.length) return false
  const expected = expectedTasks as unknown[]
  return expected.every((item, index) =>
    typeof item === 'string' && item.length > 0 && item.length <= 200 &&
    item === state.tasks![index]!.content)
}

export interface NativeResumeReader {
  read(agent: unknown): ResumeState | undefined
  dispose(): void
}

export type ResumeStage = 'blocked' | 'empty' | 'complete' | 'reconcile' | 'resume'
export interface ResumeResult {
  stage: ResumeStage
  head: string
  sourceSeq: number
  nextAction: string
  tasks: DeliveryTask[]
  remaining: string[]
  changed: string[]
  blockers: string[]
}

export function result(stage: ResumeStage, nextAction: string, state?: ResumeState,
  head = '', changed: string[] = [], blockers: string[] = []): ResumeResult {
  return { stage, head, sourceSeq: state?.writeSeq ?? -1, nextAction,
    tasks: state?.tasks ?? [], remaining: state?.tasks?.length ? remainingDeliveryTasks(state.tasks) : [],
    changed, blockers }
}

/** Advisory only: completed native Todos are candidates for final verification, not proof. */
export function hasNativeTodoGateCandidate(state: ResumeState | undefined): boolean {
  if (!state || state.invalid || !Number.isSafeInteger(state.writeSeq) ||
      state.writeSeq < 0 || !Array.isArray(state.tasks) || state.tasks.length === 0) return false
  try {
    return remainingDeliveryTasks(state.tasks).length === 0
  } catch {
    return false
  }
}
