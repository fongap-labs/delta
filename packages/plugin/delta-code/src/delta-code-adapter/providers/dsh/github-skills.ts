/** DSH-native remote Skill Provider registration. No eager/legacy Skill fallback. */
import type { Context } from '@deepseek-ai/cordis'
import { createApprovedGitHubSkillProvider, type RemoteSkillProvider, type GitHubSkillFetcher } from '../github/approved-skills.js'
import { createTaskSkillCancellation } from './task-skill-cancellation.js'

interface SkillProviderControl {
  readonly signal: AbortSignal
  readonly invalidate: () => void
}
interface SkillHost {
  registerProvider?(create: (control: SkillProviderControl) => RemoteSkillProvider): (() => void) | void
}

export function registerApprovedGitHubSkills(ctx: Context, fetcher: GitHubSkillFetcher = fetch): () => void {
  let isActive = true
  const registrations = new Map<SkillHost, () => void>()
  const unsupported = new WeakSet<SkillHost>()
  const controls = new Set<SkillProviderControl>()
  const tasks = createTaskSkillCancellation(ctx, () => {
    for (const control of controls) control.invalidate()
  })
  ctx.inject(['skills'], child => {
    if (!isActive) return
    const registry = (child as unknown as { skills?: SkillHost }).skills
    if (!registry || registrations.has(registry) || unsupported.has(registry)) return
    if (typeof registry.registerProvider !== 'function') {
      unsupported.add(registry)
      ctx.logger?.warn?.('dsh-delta-code: native Skill provider lifecycle unavailable; approved GitHub Skills disabled')
      return
    }
    let isEnabled = true
    let control: SkillProviderControl | undefined
    const cancelled = new AbortController()
    let dispose: (() => void) | void
    try {
      dispose = registry.registerProvider(lifecycle => {
        control = lifecycle
        controls.add(lifecycle)
        const provider = createApprovedGitHubSkillProvider(fetcher, undefined, {
          signal: AbortSignal.any([lifecycle.signal, cancelled.signal]),
        })
        return {
          ...provider,
          async list(options) {
            if (!isEnabled || !isActive) return []
            const task = tasks.lookup(options as { scope?: unknown })
            return task.isExternalDisabled ? [] : provider.list(options)
          },
          async get(candidate, options) {
            if (!isEnabled || !isActive) return undefined
            const task = tasks.lookup(options as { scope?: unknown })
            if (task.isExternalDisabled) return undefined
            const signal = task.signal === undefined ? options?.signal :
              options?.signal === undefined ? task.signal : AbortSignal.any([task.signal, options.signal])
            return provider.get(candidate, { ...options, signal })
          },
        }
      })
    } catch (error) {
      if (control) controls.delete(control)
      cancelled.abort()
      ctx.logger?.warn?.('dsh-delta-code: approved GitHub Skill provider unavailable: ' +
        (error instanceof Error ? error.message : String(error)))
      return
    }
    if (typeof dispose !== 'function') {
      if (control) controls.delete(control)
      isEnabled = false
      cancelled.abort()
      unsupported.add(registry)
      try { control?.invalidate() } catch { /* missing lifecycle is already disabled */ }
      ctx.logger?.warn?.('dsh-delta-code: approved GitHub Skill provider has no disposer; remote Skills disabled')
      return
    }
    const unregister = dispose
    let isReleased = false
    const release = () => {
      if (isReleased) return
      isReleased = true
      isEnabled = false
      cancelled.abort()
      if (control) controls.delete(control)
      registrations.delete(registry)
      try { unregister() } catch {
        ctx.logger?.warn?.('dsh-delta-code: approved GitHub Skill provider disposal failed')
      }
    }
    registrations.set(registry, release)
    return release
  })
  return () => {
    if (!isActive) return
    isActive = false
    tasks.dispose()
    for (const release of [...registrations.values()].reverse()) release()
    registrations.clear()
    controls.clear()
  }
}
