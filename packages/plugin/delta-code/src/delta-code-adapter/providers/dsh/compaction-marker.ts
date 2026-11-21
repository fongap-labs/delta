import type { Context } from '@deepseek-ai/cordis'
import {
  EMPTY_COMPACTION, foldCompactionMarker, parseCompactionMarker,
  type CompactionMarker, type NativeCompactionReader,
} from '../../../delta-code-quality/compaction-continuity.js'

type ProjectionHandle = { stateOf(session: unknown): CompactionMarker; dispose(): Promise<void> }
type ProjectionRegistry = { register(definition: unknown): Promise<ProjectionHandle> }

/**
 * Observe only a durable native compaction summary watermark. Never subscribe
 * to raw transcript text or add a second context/compaction engine.
 */
export function registerNativeCompactionMarker(ctx: Context): NativeCompactionReader {
  const runtime = ctx as unknown as {
    inject: (services: string[], listener: (child: unknown) => void | (() => void)) => unknown
  }
  let isActive = true
  let epoch = 0
  let handle: ProjectionHandle | undefined
  const release = (old?: ProjectionHandle): void => {
    if (!old) return
    void Promise.resolve().then(() => old.dispose()).catch(() => {
      ctx.logger?.warn?.('delta-code: native compaction marker disposal failed')
    })
  }
  const clear = (): void => {
    const old = handle
    handle = undefined
    release(old)
  }
  const definition = {
    key: 'deltaCodeCompactionMarker',
    stateVersion: 1,
    stateSchema: { parse: parseCompactionMarker },
    init: () => ({ ...EMPTY_COMPACTION }),
    apply: foldCompactionMarker,
  }
  const injection = runtime.inject(['sessionProjections'], child => {
    if (!isActive) return
    const service = (child as { sessionProjections?: ProjectionRegistry }).sessionProjections
    if (typeof service?.register !== 'function') return
    const currentEpoch = ++epoch
    clear()
    let isAttached = true
    try {
      void Promise.resolve(service.register(definition)).then(value => {
        if (!isActive || !isAttached || epoch !== currentEpoch) { release(value); return }
        handle = value
      }).catch(() => {
        ctx.logger?.warn?.('delta-code: native compaction marker registration failed')
      })
    } catch {
      ctx.logger?.warn?.('delta-code: native compaction marker registration failed')
    }
    return () => {
      isAttached = false
      if (epoch !== currentEpoch) return
      epoch++
      clear()
    }
  })
  return {
    read(agent) {
      if (!isActive || !handle || !agent || typeof agent !== 'object') return undefined
      const session = (agent as { session?: unknown }).session
      if (!session) return undefined
      return parseCompactionMarker(handle.stateOf(session))
    },
    dispose() {
      if (!isActive) return
      isActive = false
      epoch++
      clear()
      const dispose = (injection as { dispose?: () => Promise<void> } | null)?.dispose
      if (typeof dispose === 'function') {
        void Promise.resolve().then(() => dispose.call(injection)).catch(() => {
          ctx.logger?.warn?.('delta-code: native compaction injection disposal failed')
        })
      }
    },
  }
}
