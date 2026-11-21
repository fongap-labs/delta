/** A Delta feature is a collection of tools and optional lifecycle resources. */
export interface DeltaCodeFeature<T extends { name: string }> {
  readonly id: string
  readonly tools?: readonly T[]
  readonly activate?: () => void | (() => void)
}

export interface DeltaCodeRegistrar<T> {
  register(tool: T): void | (() => void)
}

/** Compose once, validate the graph, then activate. No Host-specific runtime. */
export function mountDeltaCodeFeatures<T extends { name: string }>(
  features: readonly DeltaCodeFeature<T>[],
  registrar: DeltaCodeRegistrar<T>,
): () => void {
  const ids = new Set<string>()
  const tools = new Set<string>()
  for (const feature of features) {
    if (!feature.id || ids.has(feature.id)) throw new Error('duplicate or empty Delta Code feature id: ' + feature.id)
    ids.add(feature.id)
    for (const tool of feature.tools ?? []) {
      if (!tool.name || tools.has(tool.name)) throw new Error('duplicate or empty Delta Code tool: ' + tool.name)
      tools.add(tool.name)
    }
  }
  let isMounted = true
  const disposers: Array<() => void> = []
  const dispose = (): void => {
    if (!isMounted) return
    isMounted = false
    for (const release of disposers.reverse()) {
      try { release() } catch { /* disposal is best effort and idempotent */ }
    }
    disposers.length = 0
  }
  try {
    for (const feature of features) {
      for (const tool of feature.tools ?? []) {
        const release = registrar.register(tool)
        if (typeof release === 'function') disposers.push(release)
      }
      const release = feature.activate?.()
      if (typeof release === 'function') disposers.push(release)
    }
    return dispose
  } catch (error) {
    dispose()
    throw error
  }
}
