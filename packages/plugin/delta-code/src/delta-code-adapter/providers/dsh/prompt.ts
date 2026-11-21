import type { Context } from '@deepseek-ai/cordis'
import type { Settings } from '../../../types.js'
import { DEFAULT_GUIDANCE, errorMessage } from '../../../runtime.js'

const PROMPT_SECTION_NAME = 'delta-code:guidance'

/** A deferred Host service cannot install a prompt after this generation stops. */
export function registerDeltaCodePrompt(ctx: Context, settings: Settings): () => void {
  let isActive = true
  const disposers: Array<() => void> = []
  ctx.inject(['systemPrompt'], child => {
    if (!isActive) return
    try {
      const order = child.systemPrompt?.getSectionOrder('TOOL_EDIT')
      const release = child.systemPrompt?.section({
        name: PROMPT_SECTION_NAME,
        order: Number.isFinite(order) ? (order as number) : 1300,
        text: settings.systemPromptText || DEFAULT_GUIDANCE,
      })
      if (typeof release === 'function') disposers.push(release)
    } catch (error) {
      ctx.logger?.warn?.('delta-code: failed to register system prompt section: ' + errorMessage(error))
    }
  })
  return () => {
    isActive = false
    for (const release of disposers.splice(0).reverse()) {
      try { release() } catch { /* host cleanup */ }
    }
  }
}
