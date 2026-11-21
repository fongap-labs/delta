/** Bounded, session-local co-occurrence signals; not a causal measure of Skill quality. */
import { APPROVED_GITHUB_SKILLS } from './trusted-github-skills.js'

export interface SkillOutcomeCounts {
  readonly loads: number
  readonly verified: number
  readonly failed: number
}
type MutableCounts = { loads: number; verified: number; failed: number }

const APPROVED = new Set(APPROVED_GITHUB_SKILLS.map(skill => skill.name))
const MIN_OBSERVATIONS = 3

/** One instance belongs to one native Agent; nothing is persisted between sessions. */
export class SkillOutcomeFeedback {
  private readonly counts = new Map<string, MutableCounts>()
  private readonly active = new Set<string>()
  private didObserveTaskOutcome = false

  beginTask(): void {
    this.active.clear()
    this.didObserveTaskOutcome = false
  }

  noteLoaded(name: string, provider: string): void {
    if (provider !== 'delta-code-approved-github' || !APPROVED.has(name)) return
    if (this.active.has(name)) return
    this.active.add(name)
    const count = this.counts.get(name) ?? { loads: 0, verified: 0, failed: 0 }
    count.loads += 1
    this.counts.set(name, count)
  }

  /** Only a definite test result is usable; denials, timeouts and unknown outcomes are ignored. */
  observeVerification(isPassed: boolean): void {
    // Attribution is ambiguous when more than one approved Skill was loaded.
    if (this.didObserveTaskOutcome || this.active.size !== 1) return
    this.didObserveTaskOutcome = true
    for (const name of this.active) {
      const count = this.counts.get(name)
      if (!count) continue
      if (isPassed) count.verified += 1
      else count.failed += 1
    }
  }

  /** Weak signals only reorder already approved, equally applicable matches after enough evidence. */
  prioritize(matched: readonly string[]): string[] {
    return matched.filter(name => APPROVED.has(name)).map((name, index) => {
      const count = this.counts.get(name)
      const observations = count ? count.verified + count.failed : 0
      const score = observations >= MIN_OBSERVATIONS
        ? (count!.verified + 1) / (observations + 2)
        : null
      return { name, index, score }
    }).sort((a, b) =>
      a.score === null || b.score === null ? a.index - b.index :
        b.score - a.score || a.index - b.index
    ).map(item => item.name)
  }

  read(name: string): SkillOutcomeCounts {
    const count = this.counts.get(name)
    return count ? { ...count } : { loads: 0, verified: 0, failed: 0 }
  }
}
