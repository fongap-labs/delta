/** Reject missing and blank required plugin configuration. */
export function requirePluginString(value: unknown, name: string): string {
  if (typeof value !== 'string' || value.trim() === '') {
    throw new TypeError(`plugin config ${name} must be a non-empty string`)
  }
  return value.trim()
}

/** Reject implicit boolean coercion of string-valued configuration. */
export function optionalPluginBoolean(value: unknown, name: string, isDefaultEnabled: boolean): boolean {
  if (value === undefined) return isDefaultEnabled
  if (typeof value !== 'boolean') throw new TypeError(`plugin config ${name} must be a boolean`)
  return value
}
