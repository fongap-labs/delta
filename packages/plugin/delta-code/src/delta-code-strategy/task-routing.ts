/** Host-neutral coding route policy; no Cordis/DSH API dependencies. */
import { isExternalSkillOptOut } from './trusted-github-skills.js'
import { CONTINUE, OPT_OUT, CODE_OBJECT, CODE_ACTION, SKILL_DISCOVERY, SKILL_EVALUATION, VISUAL_PARITY, RUNTIME_FORENSICS, PERF_ISSUE, CI_FAILURE, DEBUG, REVIEW, PLAN, COMPLEX, SMALL } from './builtin-task-signals.js'
import { matchesDeclarativeRule, type DeclarativePatternRule } from './declarative-rules.js'
export type CodingRoute = 'code-ci-repair' | 'code-root-cause-debugging' | 'code-review-readiness'
  | 'code-writing-plans' | 'code-fast-change' | 'code-feature-delivery' | 'code-find-skills'
  | 'code-perf-issue' | 'code-runtime-forensics' | 'code-visual-parity' | 'code-skill-evaluation'
  | (string & {})

export function isCodingTaskContinuation(text: string): boolean {
  const lastLine = text.trim().slice(-2048).split(/\r?\n/)
    .map(line => line.trim()).filter(Boolean).at(-1) ?? ''
  if (CONTINUE.test(lastLine)) return true
  // A continued task can append a new constraint after explicit punctuation.
  // Do not classify arbitrary new coding requests as continuations.
  const amendment = lastLine.match(/^([^，,:：;；]{1,32})[，,:：;；]\s*(\S.{0,1023})$/u)
  return Boolean(amendment && CONTINUE.test(amendment[1]!.trim()))
}


/** Ordered, auditable rules; adding a route changes data rather than branching logic. */
export interface CodingRouteRule extends DeclarativePatternRule {
  readonly route: CodingRoute
  readonly requiresCodingIntent?: boolean
  readonly requiresExternalSkillConsent?: boolean
  readonly maxLengthExclusive?: number
}

/** Developer-reviewed policy input, never parsed from untrusted model/tool content. */
export const DEFAULT_CODING_ROUTE_RULES: readonly CodingRouteRule[] = [
  { route: 'code-ci-repair', anyOf: [CI_FAILURE] },
  { route: 'code-skill-evaluation', anyOf: [SKILL_EVALUATION], requiresCodingIntent: true },
  { route: 'code-visual-parity', anyOf: [VISUAL_PARITY], requiresCodingIntent: true },
  { route: 'code-runtime-forensics', anyOf: [RUNTIME_FORENSICS], requiresCodingIntent: true },
  { route: 'code-perf-issue', anyOf: [PERF_ISSUE], requiresCodingIntent: true },
  { route: 'code-find-skills', anyOf: [SKILL_DISCOVERY], requiresExternalSkillConsent: true },
  { route: 'code-review-readiness', anyOf: [REVIEW], requiresCodingIntent: true },
  { route: 'code-root-cause-debugging', anyOf: [DEBUG], requiresCodingIntent: true },
  { route: 'code-writing-plans', anyOf: [PLAN],
    noneOf: [/(?:\u5b9e\u73b0|\u5f00\u53d1|\u843d\u5730|implement|build)/i], requiresCodingIntent: true },
  { route: 'code-fast-change', anyOf: [SMALL], noneOf: [COMPLEX],
    maxLengthExclusive: 220, requiresCodingIntent: true },
]

/**
 * Route only user-authored coding intent, not arbitrary tool output or quoted text.
 * Short follow-ups inherit the current route. General discussion has no route.
 * The Host may supply a reviewed rule catalog; explicit opt-outs remain authoritative.
 */
export function routeCodingTask(
  text: string, previous?: CodingRoute,
  rules: readonly CodingRouteRule[] = DEFAULT_CODING_ROUTE_RULES,
): CodingRoute | undefined {
  const trimmed = text.trim()
  // Long logs often precede the actual user instruction. Favor the tail
  // when it contains a coding action, rather than routing on old CI errors.
  const head = trimmed.length > 4096 ? trimmed.slice(0, 2048) : ''
  const tail = trimmed.length > 4096 ? trimmed.slice(-2048) : trimmed
  const finalLine = tail.split(/\r?\n/).map(line => line.trim()).filter(Boolean).at(-1) || tail
  const tailIntent = CODE_ACTION.test(finalLine) &&
    (CODE_OBJECT.test(finalLine) || (PLAN.test(finalLine) && CODE_OBJECT.test(head)))
  const input = tailIntent ? finalLine : head ? head + '\n' + tail : tail
  if (!input || OPT_OUT.test(tail)) return undefined
  if (isCodingTaskContinuation(finalLine)) return previous
  const hasCodingIntent = CODE_OBJECT.test(input) && CODE_ACTION.test(input)
  const isExternalSkillsDenied = isExternalSkillOptOut(input)
  for (const rule of rules) {
    if (rule.requiresCodingIntent && !hasCodingIntent) continue
    // Opt-outs are always stronger than caller-supplied rule metadata.
    if (isExternalSkillsDenied &&
      (rule.requiresExternalSkillConsent || rule.route === 'code-find-skills')) continue
    if (rule.maxLengthExclusive !== undefined && input.length >= rule.maxLengthExclusive) continue
    if (matchesDeclarativeRule(rule, input)) return rule.route
  }
  return hasCodingIntent ? 'code-feature-delivery' : undefined
}
