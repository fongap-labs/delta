import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import { defineTool } from '@fongap-labs/delta-plugin-shared'
import {
  CODING_TASK_KINDS, CODING_SCOPES, CODING_RISKS,
  type CodingTaskKind, type CodingScope, type CodingRisk,
} from './workflow-policy.js'
import { decideCodingWorkflow } from './workflow-route.js'
import { CONTINUE } from './builtin-task-signals.js'

interface UserTaskSources {
  latest: string
  inherited: string[]
  historyIncomplete: boolean
}

/**
 * Native user-authored task evidence only. A bare "continue" retains the last
 * substantive source; a follow-up amendment becomes the current source while
 * earlier sources remain bounded, provenance-labeled evidence. This is not a
 * persistent task store and does not resolve conflicting instructions.
 */
export class UserTaskContext {
  private current = new WeakMap<object, UserTaskSources>()

  capture(agent: object, text: string, continuation: boolean, routed: boolean): void {
    if (!routed) {
      this.current.delete(agent)
      return
    }
    const next = text.slice(-16_384)
    if (!next.trim()) return
    const prior = this.current.get(agent)
    if (!continuation || !prior) {
      this.current.set(agent, { latest: next, inherited: [], historyIncomplete: false })
    } else if (!CONTINUE.test(text.trim())) {
      // User amendments are evidence, not permission to disregard prior rules.
      const previous = [...prior.inherited, prior.latest]
      this.current.set(agent, {
        latest: next,
        inherited: previous.slice(-4),
        historyIncomplete: prior.historyIncomplete || previous.length > 4,
      })
    }
  }

  read(agent: unknown): string | undefined {
    return this.readSources(agent)?.current
  }

  readSources(agent: unknown): { current: string; inherited: string[]; historyIncomplete: boolean } | undefined {
    if (!agent || typeof agent !== 'object') return undefined
    const entry = this.current.get(agent)
    if (!entry) return undefined
    return { current: entry.latest, inherited: [...entry.inherited], historyIncomplete: entry.historyIncomplete }
  }

  remove(agent: object): void {
    this.current.delete(agent)
  }

  clear(): void {
    this.current = new WeakMap()
  }
}

export interface DirectConstraintConflict {
  subject: string
  inherited: string
  current: string
}

/**
 * Conservative literal contradiction signal: opposite explicit modal verbs
 * referring to exactly the same phrase in successive user messages.
 * Other linguistic/semantic conflicts must still be reviewed by the Agent.
 */
export function findDirectConstraintConflicts(
  current: string, inherited: readonly string[],
): DirectConstraintConflict[] {
  const parse = (message: string): Array<{ subject: string; quote: string; negative: boolean }> =>
    message.split(/[\uff0c,\u3002\uff1b;\uff01\uff1f!?\n]/u).flatMap(segment => {
      const text = segment.trim().replace(/^(?:\u7ee7\u7eed(?:\u63a8\u8fdb\u4efb\u52a1)?|\u73b0\u5728|\u6539\u4e3a)\s*[:\uff1a]?\s*/u, '')
      const match = /^(\u4e0d\u5141\u8bb8|\u7981\u6b62|\u4e0d\u5f97|\u4e0d\u8981|\u5141\u8bb8|\u53ef\u4ee5|\u5fc5\u987b)\s*(.{2,64})$/u.exec(text)
      if (!match) return []
      const subject = match[2]!.replace(/\s+/gu, '').trim()
      if (subject.length < 2) return []
      return [{ subject, quote: match[0], negative: /^(?:\u4e0d\u5141\u8bb8|\u7981\u6b62|\u4e0d\u5f97|\u4e0d\u8981)$/u.test(match[1]!) }]
    })
  const latestBySubject = new Map<string, { quote: string; negative: boolean }>()
  for (const message of inherited) {
    for (const rule of parse(message)) latestBySubject.set(rule.subject, rule)
  }
  const found: DirectConstraintConflict[] = []
  for (const rule of parse(current)) {
    const prior = latestBySubject.get(rule.subject)
    if (prior && prior.negative !== rule.negative) {
      found.push({ subject: rule.subject, inherited: prior.quote, current: rule.quote })
    }
  }
  return found.slice(0, 12)
}

export interface TaskSpecInput {
  goal: string
  taskKind: CodingTaskKind
  scope: CodingScope
  risk: CodingRisk
  userQuotes?: string[]
  proposedAcceptance: string[]
  unknowns?: string[]
  firstAction: string
}

export interface TaskSpec {
  goal: string
  taskKind: CodingTaskKind
  scope: CodingScope
  risk: CodingRisk
  userQuotes: string[]
  quoteEvidence: Array<{ quote: string; source: 'current' | 'inherited' }>
  sourceHistoryComplete: boolean
  potentialConflicts: DirectConstraintConflict[]
  readyForPlanning: boolean
  proposedAcceptance: string[]
  unknowns: string[]
  firstAction: string
  workflow: string
  verification: string[]
}

/** This is a validation contract, not an LLM, autonomous planner or permission grant. */
function statement(value: unknown, name: string, max = 320): string {
  if (typeof value !== 'string' || !value.trim() ||
      value.length > max || /[\u0000-\u001f\u007f]/.test(value)) {
    throw new Error(name + ' must be nonempty single-line text within ' + max + ' characters')
  }
  return value.trim()
}

function statements(value: unknown, name: string, min = 0): string[] {
  if (!Array.isArray(value) || value.length < min || value.length > 12) {
    throw new Error(name + ' must contain ' + min + '..12 entries')
  }
  const clean = value.map((item: unknown) => statement(item, name, 240))
  if (new Set(clean).size !== clean.length) throw new Error('duplicate ' + name + ' entry')
  return clean
}

/**
 * Only quotes found verbatim in the current user-authored task are called
 * explicit. Acceptance criteria are proposals until evidence verifies them.
 */
export function prepareTaskSpec(
  input: TaskSpecInput, sourceText: string, inheritedTexts: readonly string[] = [],
  isHistoryIncomplete = false,
): TaskSpec {
  if (!input || typeof input !== 'object' ||
      !CODING_TASK_KINDS.includes(input.taskKind) ||
      !CODING_SCOPES.includes(input.scope) ||
      !CODING_RISKS.includes(input.risk)) {
    throw new Error('taskKind, scope and risk must match a reviewed workflow category')
  }
  if (typeof sourceText !== 'string' || sourceText.length === 0) {
    throw new Error('native user task source is unavailable; do not infer explicit requirements')
  }
  const goal = statement(input.goal, 'goal')
  const userQuotes = statements(input.userQuotes ?? [], 'userQuotes')
  // Quotes must match actual user-authored messages; historical evidence is
  // labeled inherited and must be checked for overrides against the latest turn.
  const quoteEvidence = userQuotes.map(quote => {
    if (sourceText.includes(quote)) return { quote, source: 'current' as const }
    if (inheritedTexts.some(text => text.includes(quote))) {
      return { quote, source: 'inherited' as const }
    }
    throw new Error('userQuotes must appear verbatim in the current or inherited user task')
  })
  const potentialConflicts = findDirectConstraintConflicts(sourceText, inheritedTexts)
  const proposedAcceptance = statements(input.proposedAcceptance, 'proposedAcceptance', 1)
  const unknowns = statements(input.unknowns ?? [], 'unknowns')
  const firstAction = statement(input.firstAction, 'firstAction')
  const route = decideCodingWorkflow({
    taskKind: input.taskKind, scope: input.scope, risk: input.risk,
  })
  return {
    goal, taskKind: input.taskKind, scope: input.scope, risk: input.risk,
    userQuotes, quoteEvidence, sourceHistoryComplete: !isHistoryIncomplete,
    potentialConflicts, readyForPlanning: !isHistoryIncomplete && potentialConflicts.length === 0,
    proposedAcceptance, unknowns, firstAction,
    workflow: route.mode, verification: route.verification,
  }
}

/** Opt-in, once per substantial task. Never calls another model or writes a task store. */
export function createTaskSpecTool(context: UserTaskContext): ToolDefinition {
  return defineTool({
    name: 'code_task_spec',
    description: 'Validate task intent against native user messages with quote provenance. readyForPlanning=false signals incomplete inherited history or a detected literal constraint contradiction; reconcile before using a plan. This cannot detect all semantic conflicts or authorize unsafe overrides. Acceptance remains proposed, not verified. No extra model or task database.',
    parameters: {
      goal: { type: 'string', required: true, description: 'Concise proposed outcome, not a verified user quotation.' },
      taskKind: { type: 'string', required: true, enum: [...CODING_TASK_KINDS] },
      scope: { type: 'string', required: true, enum: [...CODING_SCOPES] },
      risk: { type: 'string', required: true, enum: [...CODING_RISKS] },
      userQuotes: { type: 'array', items: { type: 'string' }, description: '0..12 verbatim phrases from current or inherited native user task. Check newest instructions for conflicts and overrides.' },
      proposedAcceptance: { type: 'array', required: true, items: { type: 'string' }, description: '1..12 observable acceptance proposals, not claims of executed tests.' },
      unknowns: { type: 'array', items: { type: 'string' }, description: '0..12 unresolved facts to inspect, never invent missing values.' },
      firstAction: { type: 'string', required: true, description: 'First bounded, read-only investigation or safe next step.' },
    },
    output: {
      schema: { type: 'object', additionalProperties: false, properties: {
        goal: { type: 'string', required: true },
        taskKind: { type: 'string', required: true },
        scope: { type: 'string', required: true },
        risk: { type: 'string', required: true },
        userQuotes: { type: 'array', required: true, items: { type: 'string' } },
        sourceHistoryComplete: { type: 'boolean', required: true },
        readyForPlanning: { type: 'boolean', required: true },
        potentialConflicts: { type: 'array', required: true, items: { type: 'object', additionalProperties: false, properties: {
          subject: { type: 'string', required: true },
          inherited: { type: 'string', required: true },
          current: { type: 'string', required: true },
        } } },
        quoteEvidence: { type: 'array', required: true, items: { type: 'object', additionalProperties: false, properties: {
          quote: { type: 'string', required: true },
          source: { type: 'string', required: true },
        } } },
        proposedAcceptance: { type: 'array', required: true, items: { type: 'string' } },
        unknowns: { type: 'array', required: true, items: { type: 'string' } },
        firstAction: { type: 'string', required: true },
        workflow: { type: 'string', required: true },
        verification: { type: 'array', required: true, items: { type: 'string' } },
      } },
      render(_args, value) {
        return [{ type: 'text', text: JSON.stringify(value) }]
      },
    },
    async execute(args, exec) {
      const sources = context.readSources(exec.agent)
      if (!sources) throw new Error('no native user task is available; do not invent a task source')
      return prepareTaskSpec(args as unknown as TaskSpecInput,
        sources.current, sources.inherited, sources.historyIncomplete)
    },
  })
}
