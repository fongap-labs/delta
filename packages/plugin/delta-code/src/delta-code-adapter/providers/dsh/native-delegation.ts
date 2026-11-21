import type { Context } from '@deepseek-ai/cordis'

interface PublicNativeSubagents {
  readonly startActivation?: unknown
  readonly rootOf?: (agent: object) => unknown
  readonly getProvider?: (name: string) => unknown
}

/**
 * Observe the official, optional DSH subagent capability seam.
 * This checker never starts children, reserves work or grants authorization.
 * Only a live owning Agent and an actually registered provider can produce
 * an advisory parallel candidate; Host admission still decides execution.
 */
export function observeNativeDelegation(ctx: Context): {
  readonly canDelegate: (agent: unknown, provider: string) => boolean
  readonly dispose: () => void
} {
  let isActive = true
  let mounted: PublicNativeSubagents | undefined
  const injection = ctx.inject(['subagents'], child => {
    if (!isActive) return
    const service = (child as unknown as { subagents?: PublicNativeSubagents }).subagents
    if (!service) return
    mounted = service
    return () => {
      if (mounted === service) mounted = undefined
    }
  })

  return {
    canDelegate(agent, provider) {
      if (!isActive || !agent || typeof agent !== 'object' ||
          typeof provider !== 'string' || !provider.trim() ||
          provider.length > 128 || /[\u0000-\u001f\u007f]/.test(provider)) return false
      const service = mounted
      try {
        return !!service &&
          typeof service.startActivation === 'function' &&
          typeof service.rootOf === 'function' &&
          typeof service.getProvider === 'function' &&
          !!service.rootOf(agent) &&
          !!service.getProvider(provider)
      } catch {
        return false
      }
    },
    dispose() {
      if (!isActive) return
      isActive = false
      mounted = undefined
      const disposeInjection = (injection as { dispose?: () => Promise<void> } | null)?.dispose
      if (typeof disposeInjection === 'function') {
        void Promise.resolve().then(() => disposeInjection.call(injection)).catch(() => {
          ctx.logger?.warn?.('delta-code: native delegation probe disposal failed')
        })
      }
    },
  }
}
