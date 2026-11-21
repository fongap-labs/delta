/** Pure, Host-independent matcher used by ordered coding and resource policy catalogs. */
export interface DeclarativePatternRule {
  /** At least one expression must match. An empty list matches by default. */
  readonly anyOf?: readonly RegExp[]
  /** Every expression must match. */
  readonly allOf?: readonly RegExp[]
  /** None of these expressions may match. */
  readonly noneOf?: readonly RegExp[]
}

/**
 * Never mutate a supplied RegExp's lastIndex. Global/sticky expressions must
 * not make policy selection depend on how many messages were seen previously.
 */
function matches(pattern: RegExp, text: string): boolean {
  if (!(pattern instanceof RegExp)) throw new Error('policy patterns must be regular expressions')
  return new RegExp(pattern.source, pattern.flags.replace(/[gy]/g, '')).test(text)
}

export function matchesDeclarativeRule(rule: DeclarativePatternRule, text: string): boolean {
  // A malformed empty rule must never silently match every task.
  if (!rule.anyOf?.length && !rule.allOf?.length) return false
  return (rule.anyOf?.length ? rule.anyOf.some(pattern => matches(pattern, text)) : true) &&
    (rule.allOf?.every(pattern => matches(pattern, text)) ?? true) &&
    !(rule.noneOf?.some(pattern => matches(pattern, text)) ?? false)
}
