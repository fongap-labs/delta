export interface PluginEffectContext {
  effect(factory: () => void | (() => void)): unknown
}

/** Register effects through the Cordis lifecycle; callers own disposer creation. */
export function registerPluginEffect(context: PluginEffectContext, register: () => void | (() => void)): void {
  if (context === null || typeof context?.effect !== 'function') {
    throw new TypeError('plugin effect lifecycle is unavailable')
  }
  context.effect(register)
}
