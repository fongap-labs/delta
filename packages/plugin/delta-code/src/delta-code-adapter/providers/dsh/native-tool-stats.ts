import type { Context } from '@deepseek-ai/cordis'
import type { ExecutionStats } from '../../../delta-code-support/execution-stats.js'

type NativeRuntime = {
  on?: (event: string, callback: (execution: unknown, outcome: unknown) => void) => (() => void) | void
}

/**
 * Optional, disposable Host result observer. It sees no native arguments,
 * cannot infer duplicate calls, and does not alter tool execution.
 */
export function registerNativeToolStats(ctx: Context, stats: ExecutionStats): () => void {
  const runtime = ctx as unknown as NativeRuntime
  if (typeof runtime.on !== 'function') return () => {}
  let isActive = true
  let unbind: (() => void) | void
  try {
    unbind = runtime.on('tools/result', (execution, outcome) => {
      if (!isActive) return
      try { stats.observeNativeResult(execution, outcome) } catch {
        // Never break a native tool call or modify its outcome for telemetry.
      }
    })
  } catch {
    // Unsupported native event must not prevent Delta Code from mounting.
    return () => {}
  }
  return () => {
    isActive = false
    if (typeof unbind === 'function') unbind()
  }
}
