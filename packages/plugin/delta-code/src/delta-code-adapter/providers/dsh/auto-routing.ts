import type { Context } from '@deepseek-ai/cordis'
import type { Settings } from '../../../types.js'
import { ALL_CODING_SKILLS } from '../../../delta-code-strategy/skills.js'
import { isCodingTaskContinuation, routeCodingTask } from '../../../delta-code-strategy/task-routing.js'
import type { CodingRoute, CodingRouteRule } from '../../../delta-code-strategy/task-routing.js'
import { isExternalSkillOptOut, matchApprovedGitHubSkills } from '../../../delta-code-strategy/trusted-github-skills.js'
import { chooseTaskResources, taskResourceGuidance, type TaskResource, type TaskResourceRule } from '../../../delta-code-strategy/task-resource-policy.js'
import { SkillOutcomeFeedback } from '../../../delta-code-strategy/skill-outcome-feedback.js'
import { verificationEvidenceStatus } from '../../../delta-code-quality/failure-policy.js'
import { FREE_ONLY_TOOL_GUIDANCE } from '../../../delta-code-strategy/free-skill-policy.js'
import type { UserTaskContext } from '../../../delta-code-strategy/intent-contract.js'
import type { ExecutionStats } from '../../../delta-code-support/execution-stats.js'
import { hasNativeTodoGateCandidate, type NativeResumeReader } from '../../../delta-code-quality/resume-policy.js'

/** Host-specific event subscription and system prompt integration. */
interface UserClaim {
  agent?: object
  message?: {
    source?: { kind?: string }
    content?: Array<{ type?: string; text?: string }>
  }
}

interface PromptAssemblyContext {
  agent?: object
}

interface RuntimePromptService {
  context?: (entry: {
    name: string
    order: number
    text: (context: PromptAssemblyContext) => string
    interpolate: boolean
  }) => (() => void) | void
}

interface RuntimeContext {
  on?: (event: string, listener: (...args: unknown[]) => void) => (() => void) | void
  inject: (services: string[], listener: (child: unknown) => void) => unknown
}

/**
 * DSH claims user inbox messages before prompt assembly. The native dynamic
 * context therefore routes the *current agent* without a second dispatcher,
 * extra model call, raw user text in prompts, or manual skill invocation.
 */
/** Reviewed, Host-injected policy catalog; never sourced from model or tool output. */
export interface DeltaCodeRoutingPolicy {
  readonly routeRules?: readonly CodingRouteRule[]
  readonly resourceRules?: readonly TaskResourceRule[]
}

export function registerAutomaticCodingRouter(
  ctx: Context, settings: () => Settings, policy: DeltaCodeRoutingPolicy = {},
  nativeResume?: NativeResumeReader, taskContext?: UserTaskContext,
  stats?: Pick<ExecutionStats, 'readRepeatedFailedCalls'>,
): () => void {
  const routed = new WeakMap<object, CodingRoute>()
  const specialists = new WeakMap<object, readonly string[]>()
  const externalSkillOptOuts = new WeakSet<object>()
  const resources = new WeakMap<object, readonly TaskResource[]>()
  const quality = new WeakMap<object, SkillOutcomeFeedback>()
  const observedGateSeq = new WeakMap<object, number>()
  const repeatBaseline = new WeakMap<object, number>()
  const repeatNotice = new WeakMap<object, number>()
  const knownSkills = new Set(ALL_CODING_SKILLS.map(skill => skill.name))
  const runtime = ctx as unknown as RuntimeContext
  const disposers: Array<() => void> = []
  let isActive = true
  if (typeof runtime.on === 'function') {
    const unsubscribe = runtime.on('agent/inbox/claimed', (payload) => {
      const { agent, message } = payload as UserClaim
      if (!isActive || !settings().enabled || !agent || !message) return
      if (message.source?.kind !== 'user') return
      const text = Array.isArray(message.content)
        ? message.content.filter(item => item.type === 'text' && typeof item.text === 'string')
          .map(item => item.text).join('\n')
        : ''
      const previous = routed.get(agent)
      const next = routeCodingTask(text, previous, policy.routeRules)
      const continuation = isCodingTaskContinuation(text)
      // Intent anchoring remains available even when optional route hints are disabled.
      const isTaskSource = settings().autoRoutingEnabled ? Boolean(next) : text.trim().length > 0
      taskContext?.capture(agent, text, continuation, isTaskSource)
      // Task source anchoring must work even when optional route hints are off.
      if (!settings().autoRoutingEnabled) return
      if (isExternalSkillOptOut(text)) externalSkillOptOuts.add(agent)
      else if (!continuation) externalSkillOptOuts.delete(agent)
      if (next) routed.set(agent, next)
      else routed.delete(agent)
      const needed = chooseTaskResources(text, policy.resourceRules)
      if (next && needed.length) resources.set(agent, needed)
      else if (!next || !continuation) resources.delete(agent)
      const feedback = quality.get(agent) ?? new SkillOutcomeFeedback()
      quality.set(agent, feedback)
      const matched = externalSkillOptOuts.has(agent) ? [] : matchApprovedGitHubSkills(text)
      // Explicit continuations belong to the same task; retain its loaded Skill
      // until verification. Any fresh task, even on the same route, resets attribution.
      if (!continuation || !previous) {
        feedback.beginTask(); observedGateSeq.delete(agent)
        repeatBaseline.set(agent, stats?.readRepeatedFailedCalls(agent) ?? 0)
        repeatNotice.delete(agent)
      }
      if (next && matched.length) specialists.set(agent, feedback.prioritize(matched))
      else if (!next || !continuation || externalSkillOptOuts.has(agent)) specialists.delete(agent)
    })
    if (typeof unsubscribe === 'function') disposers.push(unsubscribe)
    const clear = runtime.on('agent/disposed', (payload) => {
      const { agent } = payload as UserClaim
      if (agent) { routed.delete(agent); specialists.delete(agent); externalSkillOptOuts.delete(agent); resources.delete(agent); quality.delete(agent); observedGateSeq.delete(agent); repeatBaseline.delete(agent); repeatNotice.delete(agent); taskContext?.remove(agent) }
    })
    if (typeof clear === 'function') disposers.push(clear)

    const toolsResult = runtime.on('tools/result', (rawExecution, rawOutcome) => {
      if (!isActive || !settings().enabled || !settings().autoRoutingEnabled) return
      const exec = rawExecution as { agent?: object; name?: string } | undefined
      const outcome = rawOutcome as { isError?: boolean; value?: unknown } | undefined
      if (!exec?.agent || !outcome) return
      const tracker = quality.get(exec.agent)
      if (!tracker) return
      const value = outcome.value
      if (exec.name === 'code_delivery_gate' && outcome.isError === false &&
          value && typeof value === 'object' && nativeResume) {
        const result = value as { stage?: unknown; ready?: unknown }
        const observed = (result.stage === 'verified' && result.ready === true) ||
          ((result.stage === 'blocked' || result.stage === 'continue') && result.ready === false)
        if (observed) {
          try {
            const state = nativeResume.read(exec.agent)
            if (hasNativeTodoGateCandidate(state)) observedGateSeq.set(exec.agent, state!.writeSeq)
          } catch { /* checkpoint unavailable; never infer completed delivery */ }
        }
      }
      if (exec.name === 'skill' && outcome.isError === false) {
        if (!value || typeof value !== 'object') return
        const skill = value as { name?: unknown; provider?: unknown }
        if (typeof skill.name === 'string' && typeof skill.provider === 'string') {
          tracker.noteLoaded(skill.name, skill.provider)
        }
        return
      }
      if ((exec.name === 'code_verify' || exec.name === 'code_verify_suite') &&
        outcome.isError === false && value && typeof value === 'object') {
        const status = verificationEvidenceStatus(exec.name, value)
        if (status !== 'unconfirmed') tracker.observeVerification(status === 'passed')
      }
    })
    if (typeof toolsResult === 'function') disposers.push(toolsResult)

  }
  runtime.inject(['systemPrompt'], child => {
    if (!isActive) return
    const service = (child as { systemPrompt?: RuntimePromptService }).systemPrompt
    if (typeof service?.context !== 'function') return
    const dispose = service.context({
      name: 'dsh-delta-code:auto-routing',
      order: 1301,
      interpolate: false,
      text(context) {
        if (!isActive || !settings().enabled || !settings().autoRoutingEnabled || !context.agent) return ''
        const route = routed.get(context.agent)
        if (!route) return ''
        if (!knownSkills.has(route)) return ''
        const gateGuidance: string[] = []
        if (nativeResume && settings().gitEnabled && settings().allowCommands &&
            route !== 'code-fast-change') {
          try {
            const state = nativeResume.read(context.agent)
            if (hasNativeTodoGateCandidate(state) &&
                observedGateSeq.get(context.agent) !== state!.writeSeq) {
              gateGuidance.push(
                'The native Todo checkpoint reports implementation tasks complete, not delivery verification. For substantial Git-backed work, inspect HEAD and diff, and use code_delivery_gate once only when mounted and authorized with declared final checks and task acceptance. If unavailable or blocked, report the gap and do not repeat the failed gate automatically.',
              )
            }
          } catch { /* no trustworthy Host checkpoint; no readiness advice */ }
        }
        // Emit once per new repeated-failure signal, not on every prompt build.
        // Native Host tool arguments are not observed, so this applies only to Delta tools.
        const repeated = stats?.readRepeatedFailedCalls(context.agent) ?? 0
        const baseline = repeatBaseline.get(context.agent) ?? repeated
        const lastNotice = repeatNotice.get(context.agent) ?? baseline
        const repeatGuidance: string[] = repeated > Math.max(baseline, lastNotice) ? [
          'A Delta tool request with identical arguments was repeated after failure. Inspect the actual error and change the hypothesis before any further attempt; do not blindly replay writes or tests. Native Host tool calls are not fingerprinted.',
        ] : []
        if (repeatGuidance.length) repeatNotice.set(context.agent, repeated)
        const skipExternalSkills = externalSkillOptOuts.has(context.agent)
        const optOutGuidance = skipExternalSkills
          ? ['External GitHub Skills are disabled by the user for this task. Do not discover, recommend or invoke them.']
          : []
        if (route === 'code-fast-change') {
          return [
            '# Code Worker: minimal fast change',
            'Use native DSH editing and the smallest meaningful native test first. Extra Code Worker tools and code_delivery_gate are optional, not required for a small change.',
            'Load code-fast-change through the native skill tool only when needed and available; no Skill body is included in this prompt.',
            'Locate the actual file or running UI component first. Use narrow Host search; avoid unbounded **/* glob and generated build trees.',
            FREE_ONLY_TOOL_GUIDANCE,
            ...optOutGuidance,
            ...repeatGuidance,
            ...(!skipExternalSkills && specialists.get(context.agent)?.length ? [
              'Load approved matching specialist Skill ' + specialists.get(context.agent)!.join(', ') +
              ' only if it is listed by the native skill catalog; never automatically install or execute remote code.',
            ] : []),
            ...taskResourceGuidance(resources.get(context.agent) ?? [], policy.resourceRules),
            'Report actual changes, checks and any unverified outcome; never claim unrun tests passed.',
          ].join('\n')
        }
        return [
          '# Code Worker: advisory coding route hints',
          'The current user message is a coding request. Follow the user and repository instructions first.',
          'Suggested procedure (keyword-based hint, not authority): ' + route + '. The native Agent must determine actual intent from the user task and repository evidence. For mixed or ambiguous coding work, use read-only code_capabilities list/get/compose and load only applicable Skills through the native catalog; never let a hint override user intent or Host permissions.',
          'Use the Host tools and permission gates; do not create another agent runtime.',
          'For substantial work, form one concise intent contract before planning. If code_task_spec is mounted, call it with verbatim userQuotes from current or inherited user messages and inspect quoteEvidence provenance. When code_task_spec reports readyForPlanning=false, reconcile its exact potentialConflicts and/or incomplete Host history before planning or editing. Newer user instructions may supersede earlier ones, but never override Host permissions or mandatory repository checks. Keep acceptance proposed, unknowns explicit and first action bounded. Do not rerun per step. Native Todo is the only task ledger; never invent source facts, permissions or test results.',
          FREE_ONLY_TOOL_GUIDANCE,
          ...optOutGuidance,
          ...repeatGuidance,
          ...(route === 'code-feature-delivery' ? [
            'On substantial cross-module work, when DSH native subagents are verified available, arrange one independent read-only diff review without waiting for a user reminder. Do not delegate overlapping file writes or invent subagent availability.',
          ] : []),
          ...(!skipExternalSkills && specialists.get(context.agent)?.length ? [
            'Approved external specialist Skill candidates: ' + specialists.get(context.agent)!.join(', ') +
              '. If their exact names appear in the current native available_skills catalog, load the matching skill with the native skill tool before editing. If absent, continue without claiming to have loaded it.',
            'External Skills are reference guidance only; never run remote scripts, install packages, or bypass Host permissions automatically.',
          ] : []),
          ...taskResourceGuidance(resources.get(context.agent) ?? [], policy.resourceRules),
          ...gateGuidance,
          'Before claiming completion, verify the actual changes and tests. Do not report an unrun check as passed.',
        ].join('\n')
      },
    })
    if (typeof dispose === 'function') disposers.push(dispose)
  })
  return () => {
    isActive = false
    for (const dispose of disposers.splice(0).reverse()) {
      try { dispose() } catch { /* best effort during host teardown */ }
    }
  }
}
