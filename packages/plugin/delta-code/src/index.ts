/** Delta Code plugin entry. All business features are composed by the kernel. */
import type { Context } from '@deepseek-ai/cordis'
import type { CodeWorkerConfig } from './types.js'
import { normalizeConfig } from './runtime.js'
import { mountDeltaCode } from './delta-code-composition.js'

export { Config } from './config.js'
export type { CodeWorkerConfig, GitHubAccountConfig, LspServerConfig } from './types.js'

export const name = '@fongap-labs/delta-code'
// Non-command features only require the native tool registry.
// Command-backed features inject subprocess separately and degrade independently.
export const inject = ['tools']

export function apply(ctx: Context, config?: CodeWorkerConfig): () => void {
  let currentSettings = normalizeConfig(config)
  let release = mountDeltaCode(ctx, currentSettings)
  let isClosed = false
  const runtime = ctx as unknown as {
    on?: (event: string, listener: () => void) => void | (() => void)
  }
  const stopUpdates = runtime.on?.('loader/volatile-update', () => {
    if (isClosed) return
    // Validate the replacement before releasing the currently mounted generation.
    const nextSettings = normalizeConfig(config)
    const previousSettings = currentSettings
    release()
    try {
      release = mountDeltaCode(ctx, nextSettings)
      currentSettings = nextSettings
    } catch (error) {
      try {
        release = mountDeltaCode(ctx, previousSettings)
      } catch (restoreError) {
        ctx.logger?.error?.('delta-code: failed to restore previous configuration after reload: ' +
          String(restoreError))
        throw new AggregateError([error, restoreError], 'Delta Code reload and rollback both failed')
      }
      throw error
    }
  })
  const dispose = () => {
    if (isClosed) return
    isClosed = true
    if (typeof stopUpdates === 'function') stopUpdates()
    release()
  }
  // Returning a disposer supports direct callers. Cordis additionally needs
  // an owned effect so fiber.dispose() releases root-scoped registrations.
  if (typeof ctx.effect === 'function') ctx.effect(() => dispose, 'delta-code:plugin-disposal')
  return dispose
}
