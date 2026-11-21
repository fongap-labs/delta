/**
 * DSH Cordis Host adapter v1. The worker uses this narrow contract rather
 * than interpreting service lookup failures as "not mounted".
 *
 * A future Harness integration must supply its own adapter. This module
 * does not claim to support another runtime or grant permissions.
 */
export const DSH_HOST_ADAPTER_VERSION = 'dsh-cordis-v1' as const

export type DshServiceState = 'absent' | 'mounted' | 'inaccessible'

export interface DshServiceProbe {
  state: DshServiceState
  value: unknown
}

/** Refuse local FS fallback if service lookup itself is denied. */
export function lookupDshHostFs(ctx: { get(name: 'fs'): unknown }): unknown {
  try {
    return ctx.get('fs')
  } catch {
    throw new Error('DSH Host FS service inaccessible; refusing local fallback')
  }
}

export function lookupDshWorkingDirectory(
  ctx: { get(name: 'workingDirectory'): unknown },
): unknown {
  try {
    return ctx.get('workingDirectory')
  } catch {
    throw new Error('DSH Host workingDirectory service inaccessible; refusing local fallback')
  }
}

/** Introspection only: never invokes FS, process or workspace methods. */
export function probeDshService(read: () => unknown): DshServiceProbe {
  try {
    const value = read()
    return { state: value == null ? 'absent' : 'mounted', value }
  } catch {
    return { state: 'inaccessible', value: undefined }
  }
}
