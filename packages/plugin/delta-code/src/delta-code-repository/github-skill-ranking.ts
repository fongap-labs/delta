/** Reviewable, non-security ranking of GitHub Skill search candidates.
 * Popularity and account age are weak, gameable signals: never grant trust from a score.
 */
export interface SkillRankingWeights {
  readonly stars: number
  readonly forks: number
  readonly maintainerTenure: number
  readonly repositoryAge: number
  readonly recentMaintenance: number
}

export const DEFAULT_SKILL_RANKING_WEIGHTS: Readonly<SkillRankingWeights> = Object.freeze({
  stars: 10, forks: 5, maintainerTenure: 10, repositoryAge: 5, recentMaintenance: 20,
})

export interface SkillRankingEvidence {
  readonly stars?: unknown
  readonly forks?: unknown
  readonly ownerCreatedAt?: unknown
  readonly repositoryCreatedAt?: unknown
  readonly lastPushAt?: unknown
  readonly archived?: unknown
  readonly disabled?: unknown
}

function count(value: unknown): number | null {
  return Number.isSafeInteger(value) && (value as number) >= 0 ? value as number : null
}

function ageDays(value: unknown, now: Date): number | null {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z$/.test(value)) return null
  const parsed = Date.parse(value)
  if (!Number.isFinite(parsed) || parsed > now.getTime()) return null
  return Math.floor((now.getTime() - parsed) / 86_400_000)
}

function weightedLog(value: number, cap: number, weight: number): number {
  return Math.round(weight * Math.min(1, Math.log1p(value) / Math.log1p(cap)))
}

function weightedAge(days: number, capDays: number, weight: number): number {
  return Math.round(weight * Math.min(1, days / capDays))
}

function weightedFreshness(days: number, weight: number): number {
  // Recent work is only a maintenance signal: no age threshold grants safety.
  return Math.round(weight * Math.max(0, 1 - days / 730))
}

export function rankSkillEvidence(
  evidence: SkillRankingEvidence,
  now: Date = new Date(),
  weights: SkillRankingWeights = DEFAULT_SKILL_RANKING_WEIGHTS,
): {
  readonly score: number
  readonly maximum: number
  readonly completeness: 'complete' | 'partial' | 'unavailable'
  readonly components: Readonly<Record<keyof SkillRankingWeights, number | null>>
  readonly inputs: {
    readonly stars: number | null; readonly forks: number | null
    readonly ownerCreatedAt: string | null; readonly repositoryCreatedAt: string | null
    readonly lastPushAt: string | null; readonly archived: boolean | null; readonly disabled: boolean | null
  }
  readonly warning: string
} {
  const values = Object.values(weights)
  if (values.some(v => !Number.isFinite(v) || v < 0 || v > 100)) {
    throw new Error('invalid Skill ranking weights')
  }
  const maximum = values.reduce((sum, v) => sum + v, 0)
  if (maximum <= 0 || maximum > 100) throw new Error('invalid Skill ranking weight budget')
  if (!Number.isFinite(now.getTime())) throw new Error('invalid ranking reference date')

  const stars = count(evidence.stars)
  const forks = count(evidence.forks)
  const ownerAge = ageDays(evidence.ownerCreatedAt, now)
  const repositoryAge = ageDays(evidence.repositoryCreatedAt, now)
  const pushedAge = ageDays(evidence.lastPushAt, now)
  const archived = typeof evidence.archived === 'boolean' ? evidence.archived : null
  const disabled = typeof evidence.disabled === 'boolean' ? evidence.disabled : null
  const components = {
    stars: stars === null ? null : weightedLog(stars, 10_000, weights.stars),
    forks: forks === null ? null : weightedLog(forks, 2_000, weights.forks),
    maintainerTenure: ownerAge === null ? null : weightedAge(ownerAge, 1_825, weights.maintainerTenure),
    repositoryAge: repositoryAge === null ? null : weightedAge(repositoryAge, 1_095, weights.repositoryAge),
    recentMaintenance: pushedAge === null ? null : weightedFreshness(pushedAge, weights.recentMaintenance),
  }
  const known = Object.values(components).filter((v): v is number => v !== null).length
  const raw = Object.values(components).reduce<number>((sum, v) => sum + (v ?? 0), 0)
  return {
    score: archived === true || disabled === true ? 0 : raw,
    maximum,
    completeness: known === 0 ? 'unavailable' : known === Object.keys(components).length ? 'complete' : 'partial',
    components,
    inputs: {
      stars, forks, ownerCreatedAt: ownerAge === null ? null : evidence.ownerCreatedAt as string,
      repositoryCreatedAt: repositoryAge === null ? null : evidence.repositoryCreatedAt as string,
      lastPushAt: pushedAge === null ? null : evidence.lastPushAt as string, archived, disabled,
    },
    warning: 'Advisory discovery ranking only; stars and maintainer history can be manipulated. Not a safety, license, identity, or Skill trust approval.',
  }
}
