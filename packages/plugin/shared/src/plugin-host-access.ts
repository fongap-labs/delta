/** Host capabilities are required, never silently replaced with local OS access. */
export class MissingPluginHostServiceError extends Error {
  constructor(readonly service: string, readonly operation: string) {
    super(`plugin host service unavailable: ${service}.${operation}`)
    this.name = 'MissingPluginHostServiceError'
  }
}

/** Validate a mounted Cordis service without constructing a second plugin host. */
export function requirePluginHostService<T>(context: unknown, service: string, operation: string): T {
  if (context === null || typeof context !== 'object') {
    throw new MissingPluginHostServiceError(service, operation)
  }
  const mounted = (context as Record<string, unknown>)[service]
  if (mounted === null || (typeof mounted !== 'object' && typeof mounted !== 'function') ||
      typeof (mounted as Record<string, unknown>)[operation] !== 'function') {
    throw new MissingPluginHostServiceError(service, operation)
  }
  return mounted as T
}
