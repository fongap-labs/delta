import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import { defineTool } from '@fongap-labs/delta-plugin-shared'
import { ALL_CODING_SKILLS } from './skills.js'
import { planDeclaredCapabilities, type DeclaredCapabilityPlan } from './capability-plan.js'

/**
 * Reviewed, host-neutral scenarios. The native Agent decides which scenario
 * matches the task semantically; no keywords, external content or model output
 * may edit this directory or grant execution permission.
 */
export interface CodingCapability {
  readonly id: string
  readonly title: string
  readonly intent: string
  readonly accepts: readonly string[]
  readonly produces: readonly string[]
  /** Optional related capabilities, not implicit tool or Skill invocations. */
  readonly uses: readonly string[]
  /** Mandatory capability dependencies, unlike optional related `uses`. */
  readonly requires?: readonly string[]
  readonly skills: readonly string[]
}

export const CODING_CAPABILITIES: readonly CodingCapability[] = [
  {
    id: 'small-change', title: 'Focused change',
    intent: 'Make one bounded change with a clear observable result.',
    accepts: ['User goal', 'Relevant existing implementation'],
    produces: ['Minimal diff', 'Focused verification evidence'],
    uses: [], skills: ['code-fast-change'],
  },
  {
    id: 'feature-delivery', title: 'Feature implementation',
    intent: 'Implement a substantial feature or refactor within existing architecture.',
    accepts: ['User acceptance criteria', 'Project constraints', 'Existing source and tests'],
    produces: ['Implementation', 'Regression evidence', 'PR when authorized'],
    uses: ['quality-review', 'diagnostic-readiness'],
    skills: ['code-feature-delivery'],
  },
  {
    id: 'defect-repair', title: 'Root-cause repair',
    intent: 'Find a reproducible cause and verify a targeted fix.',
    accepts: ['Observed failure', 'Logs or reproduction', 'Relevant source'],
    produces: ['Root cause', 'Minimal fix', 'Regression evidence'],
    uses: ['diagnostic-readiness', 'quality-review'],
    skills: ['code-root-cause-debugging'],
  },
  {
    id: 'ci-repair', title: 'CI failure investigation',
    intent: 'Inspect the actual failing job and restore genuine required checks.',
    accepts: ['Failed commit or PR', 'GitHub Actions job evidence'],
    produces: ['Confirmed cause', 'Reviewed fix', 'Current SHA CI evidence'],
    uses: ['diagnostic-readiness'],
    skills: ['code-ci-repair'],
  },
  {
    id: 'code-review', title: 'Read-only change review',
    intent: 'Assess correctness, safety and gaps without silently modifying code.',
    accepts: ['Diff or PR', 'Intended behavior', 'Available check results'],
    produces: ['Actionable findings', 'Known verification gaps'],
    uses: ['quality-review'], skills: ['code-review-readiness'],
  },
  {
    id: 'quality-review', title: 'Test strength and maintainability',
    intent: 'Evaluate whether checks actually detect relevant behavior regressions.',
    accepts: ['Changed code', 'Existing tests and coverage evidence'],
    produces: ['Missed behavior cases', 'Optional verified mutation/complexity evidence'],
    uses: [], skills: ['code-quality-strength'],
  },
  {
    id: 'diagnostic-readiness', title: 'Future failure diagnosability',
    intent: 'Check whether recurrence could be located using real, safe diagnostic evidence.',
    accepts: ['Failure modes', 'Existing logs or traces', 'Current error handling'],
    produces: ['Specific diagnostic gaps', 'Read-back evidence when changed'],
    uses: [], skills: ['code-diagnostic-readiness'],
  },
  {
    id: 'verified-experience', title: 'Verified experience reuse',
    intent: 'Suggest a reusable lesson only after a real fix and its limits are established.',
    accepts: ['Completed task', 'Source revision', 'Repro and verification evidence'],
    produces: ['Reviewable candidate with provenance', 'No automatic Skill activation'],
    uses: ['quality-review'], skills: ['code-experience-review'],
  },
  {
    id: 'visual-parity', title: 'Visual implementation review',
    intent: 'Compare a rendered interface to an actual reference when Host UI tools exist.',
    accepts: ['Reference', 'Running target UI', 'Matching viewport and state'],
    produces: ['Observed visual differences', 'Real recheck or explicit unverified state'],
    uses: [], skills: ['code-visual-parity'],
  },
]

/** Refuse broken declarations before publishing any capabilities to a model. */
export function validateCodingCapabilities(
  catalog: readonly CodingCapability[] = CODING_CAPABILITIES,
  knownSkills: ReadonlySet<string> = new Set(ALL_CODING_SKILLS.map(skill => skill.name)),
): void {
  if (!Array.isArray(catalog) || catalog.length < 1 || catalog.length > 64) {
    throw new Error('invalid capability catalog size')
  }
  const validText = (value: unknown): value is string =>
    typeof value === 'string' && value.trim().length > 0 &&
    value.length <= 512 && !/[\u0000-\u001f\u007f]/.test(value)
  const validList = (value: unknown, canBeEmpty = false): value is readonly string[] =>
    Array.isArray(value) && value.length >= (canBeEmpty ? 0 : 1) &&
    value.length <= 16 && value.every(validText)
  const byId = new Map<string, CodingCapability>()
  for (const item of catalog) {
    if (!item || typeof item !== 'object' || typeof item.id !== 'string' ||
      !/^[a-z][a-z0-9-]{0,63}$/.test(item.id) || byId.has(item.id)) {
      throw new Error('invalid or duplicate capability id')
    }
    if (!validText(item.title) || !validText(item.intent) ||
      !validList(item.accepts) || !validList(item.produces) ||
      !validList(item.skills) || !validList(item.uses, true) ||
      (item.requires !== undefined && !validList(item.requires, true))) {
      throw new Error('incomplete capability: ' + item.id)
    }
    if (new Set(item.skills).size !== item.skills.length ||
      item.skills.some((skill: string) => !knownSkills.has(skill))) {
      throw new Error('unknown or duplicate capability Skill: ' + item.id)
    }
    if (new Set(item.uses).size !== item.uses.length ||
        (item.requires !== undefined && new Set(item.requires).size !== item.requires.length)) {
      throw new Error('duplicate capability reference: ' + item.id)
    }
    byId.set(item.id, item)
  }
  const visited = new Set<string>()
  const visiting = new Set<string>()
  const visit = (id: string): void => {
    if (visiting.has(id)) throw new Error('cyclic capability reference: ' + id)
    if (visited.has(id)) return
    const item = byId.get(id)
    if (!item) throw new Error('unknown capability reference: ' + id)
    visiting.add(id)
    // Catalog reference cycles remain invalid for stable guidance; only the
    // explicitly declared requires edges participate in execution planning.
    for (const dependency of item.uses) visit(dependency)
    for (const dependency of item.requires ?? []) visit(dependency)
    visiting.delete(id)
    visited.add(id)
  }
  for (const id of byId.keys()) visit(id)
}

/** JSON-safe snapshot. Readonly catalog arrays are copied before returning a tool value. */
export interface CodingCapabilitySnapshot {
  id: string
  title: string
  intent: string
  accepts: string[]
  produces: string[]
  uses: string[]
  skills: string[]
}

export type CapabilityQuery =
  | { action: 'list' }
  | { action: 'get'; id: string }
  | { action: 'compose'; ids: readonly string[] }
  | { action: 'plan'; ids: readonly string[]; completed?: readonly string[]; active?: readonly string[] }

/**
 * Lists and composes existing guidance only. Related "uses" are suggestions,
 * not implicit loads, dispatches, side effects, or permission elevation.
 */
export function queryCodingCapabilities(
  query: CapabilityQuery,
  catalog: readonly CodingCapability[] = CODING_CAPABILITIES,
): { action: string; capabilities: CodingCapabilitySnapshot[]; skills: string[]; plan?: DeclaredCapabilityPlan } {
  validateCodingCapabilities(catalog)
  if (query.action === 'list') {
    return {
      action: 'list', skills: [],
      capabilities: catalog.map(item => ({
        id: item.id, title: item.title, intent: item.intent,
        accepts: [], produces: [], uses: [], skills: [],
      })),
    }
  }
  const ids = query.action === 'get' ? [query.id] : query.ids
  if (!Array.isArray(ids) || ids.length < 1 || ids.length > 8 ||
    ids.some(id => typeof id !== 'string') || new Set(ids).size !== ids.length) {
    throw new Error('select 1..8 distinct capability ids')
  }
  const selected = ids.map(id => {
    const found = catalog.find(item => item.id === id)
    if (!found) throw new Error('unknown capability: ' + id)
    return found
  })
  const plan = query.action === 'plan'
    ? planDeclaredCapabilities(catalog, ids, query.completed, query.active)
    : undefined
  return {
    action: query.action,
    capabilities: selected.map(item => ({
      id: item.id, title: item.title, intent: item.intent,
      accepts: [...item.accepts], produces: [...item.produces],
      uses: [...item.uses], skills: [...item.skills],
    })),
    skills: plan ? [...plan.skills] : [...new Set(selected.flatMap(item => item.skills))],
    ...(plan ? { plan } : {}),
  }
}

export function createCodingCapabilitiesTool(): ToolDefinition {
  validateCodingCapabilities()
  return defineTool({
    name: 'code_capabilities',
    description: 'Read-only reviewed Code Worker capability directory. Let the native Agent select task scenarios by meaning. list/get/compose inspect guidance; plan topologically orders only declared required dependencies using Host-verified checkpoints. Optional uses never trigger tools. No Skill loads, dispatch, writes or permission grants.',
    parameters: {
      action: { type: 'string', required: true, enum: ['list', 'get', 'compose', 'plan'] },
      id: { type: 'string', description: 'Capability id for get.' },
      ids: { type: 'array', items: { type: 'string' }, description: '1..8 unique capability ids for compose or plan.' },
      completed: { type: 'array', items: { type: 'string' }, description: 'Only verified Host-completed capability ids; never infer completion.' },
      active: { type: 'array', items: { type: 'string' }, description: 'Host-reported active capability ids, excluded from duplicate dispatch.' },
    },
    output: {
      schema: { type: 'object', additionalProperties: false, properties: {
        action: { type: 'string', required: true },
        capabilities: { type: 'array', required: true, items: {
          type: 'object', additionalProperties: false, properties: {
            id: { type: 'string', required: true },
            title: { type: 'string', required: true },
            intent: { type: 'string', required: true },
            accepts: { type: 'array', required: true, items: { type: 'string' } },
            produces: { type: 'array', required: true, items: { type: 'string' } },
            uses: { type: 'array', required: true, items: { type: 'string' } },
            skills: { type: 'array', required: true, items: { type: 'string' } },
          },
        } },
        skills: { type: 'array', required: true, items: { type: 'string' } },
        plan: { type: 'object', additionalProperties: false, properties: {
          waves: { type: 'array', required: true, items: { type: 'array', items: { type: 'string' } } },
          ready: { type: 'array', required: true, items: { type: 'string' } },
          active: { type: 'array', required: true, items: { type: 'string' } },
          blocked: { type: 'array', required: true, items: { type: 'string' } },
          required: { type: 'array', required: true, items: { type: 'string' } },
          suggested: { type: 'array', required: true, items: { type: 'string' } },
          skills: { type: 'array', required: true, items: { type: 'string' } },
        } },
      } },
      render(_args, value) {
        return [{ type: 'text', text: JSON.stringify(value) }]
      },
    },
    execute: async args => {
      const input = args as { action: string; id?: string; ids?: string[]; completed?: string[]; active?: string[] }
      if (input.action === 'list') return queryCodingCapabilities({ action: 'list' })
      if (input.action === 'get') return queryCodingCapabilities({ action: 'get', id: input.id ?? '' })
      if (input.action === 'compose') return queryCodingCapabilities({ action: 'compose', ids: input.ids ?? [] })
      if (input.action === 'plan') return queryCodingCapabilities({ action: 'plan', ids: input.ids ?? [], completed: input.completed, active: input.active })
      throw new Error('unsupported capability action')
    },
  })
}
