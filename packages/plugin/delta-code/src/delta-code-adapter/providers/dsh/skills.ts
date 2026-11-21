import type { Context } from '@deepseek-ai/cordis'
import { ALL_CODING_SKILLS } from '../../../delta-code-strategy/skills.js'

/** The Host's provider contract: metadata is discoverable, bodies load only on get(). */
interface SkillLookupOptions {
  readonly signal?: AbortSignal
}

interface SkillCandidate {
  readonly name: string
  readonly description: string
  readonly source: 'runtime'
  readonly provider: string
  readonly rank: number
  readonly invocation: { readonly modelInvocable: boolean; readonly userInvocable: boolean }
  readonly locator: string
}

interface SkillProvider {
  readonly name: string
  list(options?: SkillLookupOptions): Promise<readonly SkillCandidate[]>
  get(candidate: SkillCandidate, options?: SkillLookupOptions): Promise<(SkillCandidate & { content: string }) | undefined>
}

interface SkillProviderControl {
  readonly signal: AbortSignal
  readonly invalidate: () => void
}

interface SkillRegistry {
  registerProvider?(create: (control: SkillProviderControl) => SkillProvider): (() => void) | void
}

const PROVIDER = 'delta-code-coding'
const summaries: readonly SkillCandidate[] = Object.freeze(ALL_CODING_SKILLS.map(skill => Object.freeze({
  name: skill.name,
  description: skill.description,
  source: 'runtime' as const,
  provider: PROVIDER,
  rank: 250,
  invocation: Object.freeze({ modelInvocable: true, userInvocable: true }),
  locator: skill.name,
})))
const definitions = new Map(ALL_CODING_SKILLS.map(skill => [skill.name, skill] as const))

function codingProvider(control: SkillProviderControl, isActive: () => boolean): SkillProvider {
  const available = () => isActive() && !control.signal.aborted
  return {
    name: PROVIDER,
    async list(options) {
      options?.signal?.throwIfAborted()
      return available() ? summaries : []
    },
    async get(candidate, options) {
      options?.signal?.throwIfAborted()
      if (!available() || candidate.provider !== PROVIDER || candidate.locator !== candidate.name) {
        return undefined
      }
      const skill = definitions.get(candidate.name)
      const summary = summaries.find(item => item.name === candidate.name)
      if (!skill || !summary) return undefined
      return {
        ...summary,
        content: '# ' + skill.title + '\n\n' + skill.instructions.map(line => '- ' + line).join('\n'),
      }
    },
  }
}

/**
 * Register only with the native lazy-provider lifecycle. Hosts lacking a
 * disposable registerProvider are not supported for Skills: do not fall back
 * to eager register(), or keep an unremovable model-invocable contribution.
 */
export function registerCodingSkills(ctx: Context): () => void {
  let isActive = true
  const registrations = new Map<SkillRegistry, () => void>()
  const unsupported = new WeakSet<SkillRegistry>()
  ctx.inject(['skills'], child => {
    if (!isActive) return
    const registry = (child as unknown as { skills?: SkillRegistry }).skills
    if (!registry || registrations.has(registry) || unsupported.has(registry)) return
    if (typeof registry.registerProvider !== 'function') {
      unsupported.add(registry)
      ctx.logger?.warn?.('delta-code: native Skill provider lifecycle unavailable; coding Skills disabled')
      return
    }
    let isEnabled = true
    let control: SkillProviderControl | undefined
    let dispose: (() => void) | void
    try {
      dispose = registry.registerProvider(lifecycle => {
        control = lifecycle
        return codingProvider(lifecycle, () => isActive && isEnabled)
      })
    } catch (error) {
      ctx.logger?.warn?.('delta-code: coding Skill provider registration failed: ' +
        (error instanceof Error ? error.message : String(error)))
      return
    }
    if (typeof dispose !== 'function') {
      // An incompatible Host may have registered a provider with no disposer.
      // Invalidate the now-empty catalog; stale callers cannot load any body.
      isEnabled = false
      unsupported.add(registry)
      try { control?.invalidate() } catch { /* legacy Host invalidation is optional */ }
      ctx.logger?.warn?.('delta-code: coding Skill provider has no disposer; coding Skills disabled')
      return
    }
    const unregister = dispose
    let isReleased = false
    const release = () => {
      if (isReleased) return
      isReleased = true
      isEnabled = false
      registrations.delete(registry)
      try { unregister() } catch {
        ctx.logger?.warn?.('delta-code: coding Skill provider disposal failed')
      }
    }
    registrations.set(registry, release)
    return release
  })
  return () => {
    if (!isActive) return
    isActive = false
    for (const release of [...registrations.values()].reverse()) release()
    registrations.clear()
  }
}
