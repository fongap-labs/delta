/** Centralized DSH Host FS capability contract. */
import { lookupDshHostFs } from './host-adapter.js'
export type HostWriteIntent =
  | { kind: 'createIfAbsent' }
  | { kind: 'replaceIfVersion'; version: string }

export interface HostFsCapabilities {
  resolve(path: string, options: { cwd: string; signal?: AbortSignal }): Promise<unknown>
  contains?(parent: unknown, child: unknown): boolean
  stat?(target: unknown, signal?: AbortSignal): Promise<{ type: string; version?: unknown; size?: number } | undefined>
  readText?(target: unknown, signal?: AbortSignal): Promise<string>
  /** Optional Host capability fact: a confining backend requires per-call Host policy. */
  readonly sandboxMode?: unknown
  writeText?(target: unknown, content: string, expected?: 'overwrite' | HostWriteIntent, signal?: AbortSignal, sandboxPolicy?: unknown): Promise<unknown>
  listDir?(target: unknown, signal?: AbortSignal): Promise<unknown[]>
  lstat?(path: string, opts?: { cwd?: string }, signal?: AbortSignal): Promise<{ type: string } | undefined>
  processPath?(target: unknown): string
}

export type HostFs<K extends keyof HostFsCapabilities> = {
  [P in K]-?: NonNullable<HostFsCapabilities[P]>
}

/** Host's stable core contract; optional capabilities are probed at use sites. */
export type HostFsService = HostFsCapabilities & HostFs<'readText' | 'writeText'>

/**
 * A mounted but incomplete Host is denied, NEVER downgraded to local Node IO.
 * An absent mount returns undefined so callers retain their established fallback.
 */
export function requireHostFs<K extends keyof HostFsCapabilities>(
  ctx: { get(name: 'fs'): unknown },
  required: readonly K[],
  refusal: string,
): HostFs<K> | undefined {
  const mounted: unknown = lookupDshHostFs(ctx)
  if (!mounted) return undefined
  const candidate = mounted as HostFsCapabilities
  for (const capability of required) {
    if (typeof candidate[capability] !== 'function') {
      throw new Error(`DSH Host FS missing ${required.join('/')}; ${refusal}`)
    }
  }
  return mounted as HostFs<K>
}
