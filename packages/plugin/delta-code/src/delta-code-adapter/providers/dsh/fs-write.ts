/** DSH-specific call signature adaptation. This is not a capability probe or a version whitelist. */
import type { DeltaFileWriteContract } from '../../contracts/files.js'
import type { HostFsCapabilities, HostWriteIntent } from './host-fs.js'

/** Resolve the calling session's standing policy through the Host's own public service.
 * Never infer modes, synthesize grants, cache another session's policy, or widen access. */
export function resolveDshFsSandboxPolicy(
  ctx: { get(name: 'sandboxPolicy'): unknown }, fs: HostFsCapabilities,
  exec: { agent?: { session?: unknown } },
): unknown {
  // Older or unconstrained providers have no per-call sandbox contract.
  if (fs.sandboxMode === undefined) return undefined
  const service = ctx.get('sandboxPolicy') as { resolve?: (request: { session?: unknown }) => unknown } | undefined
  if (!service || typeof service.resolve !== 'function') {
    throw new Error('DSH Host FS sandbox policy unavailable; refusing to write')
  }
  const session = exec.agent?.session
  const policy = service.resolve(session === undefined ? {} : { session })
  if (!policy || typeof policy !== 'object') throw new Error('DSH Host returned no sandbox policy; refusing to write')
  return policy
}

export async function writeDshHostText(
  fs: HostFsCapabilities,
  target: unknown,
  content: string,
  signal: AbortSignal,
  contract: DeltaFileWriteContract,
  intent?: HostWriteIntent,
  sandboxPolicy?: unknown,
): Promise<unknown> {
  if (typeof fs.writeText !== 'function') throw new Error('DSH Host FS writeText unavailable')
  // Keep the established four-argument call for legacy/non-sandboxed Hosts.
  const write = (expected: 'overwrite' | HostWriteIntent | undefined) => sandboxPolicy === undefined
    ? fs.writeText!(target, content, expected, signal)
    : fs.writeText!(target, content, expected, signal, sandboxPolicy)
  if (contract === 'guarded-intent') {
    if (!intent) throw new Error('guarded-intent requires an explicit write guard')
    return write(intent)
  }
  if (intent) throw new Error('pinned intent cannot be used for an unconditional write')
  if (contract === 'native-unguarded') {
    // The public FileSystem API uses an omitted expected argument for
    // unconditional writes; signal remains the fourth parameter.
    return write(undefined)
  }
  // A confining Host advertises the public optional-intent contract. The
  // historical overwrite sentinel remains only for legacy providers.
  return write(sandboxPolicy === undefined ? 'overwrite' : undefined)
}
