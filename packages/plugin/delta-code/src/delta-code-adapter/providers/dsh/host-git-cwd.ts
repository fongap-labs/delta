import { realpath } from 'node:fs/promises'
import type { Context } from '@deepseek-ai/cordis'
import type { ToolRunContext } from '@deepseek-ai/dsh-tools'
import { isPathInside, resolveInside } from '@fongap-labs/delta-plugin-shared'
import { requireHostFs } from './host-fs.js'

/**
 * Resolve a Git subprocess directory without crossing the DSH Host FS boundary.
 *
 * Mounted Host FS always owns resolution, containment, directory metadata and
 * the process-visible path. Never call Node realpath after a Host FS denial,
 * incomplete contract or a remote/virtual workspace resolution.
 *
 * The Node path is ONLY for a Host which has no FS service mounted. In both
 * modes Git itself still runs exclusively through DSH Host subprocess.
 */
export async function resolveAuthorizedGitCwd(
  ctx: Context, root: string, requested: string | undefined, exec: ToolRunContext,
): Promise<string> {
  if (exec.signal.aborted) throw new Error('Git workspace validation cancelled')
  // Host filesystem paths can belong to a different OS. Never use the
  // plugin's Node path.join/resolveInside for a mounted remote Host cwd.
  if (requested && (requested.includes('\0') || requested.trim() === '')) {
    throw new Error('invalid Host workspace relative path')
  }
  const fs = requireHostFs(ctx, ['resolve', 'contains', 'stat', 'processPath'], 'refusing local Git fallback')
  if (fs) {
    const parent = await fs.resolve(root, { cwd: root, signal: exec.signal })
    if (exec.signal.aborted) throw new Error('Git workspace validation cancelled')
    const target = await fs.resolve(requested ?? root, { cwd: root, signal: exec.signal })
    if (!fs.contains(parent, target)) throw new Error('Git cwd escapes Host workspace')
    if (exec.signal.aborted) throw new Error('Git workspace validation cancelled')
    const info = await fs.stat(target, exec.signal)
    if (!info || info.type !== 'directory') throw new Error('Git cwd is not a Host-authorized directory')
    if (exec.signal.aborted) throw new Error('Git workspace validation cancelled')
    const processCwd = fs.processPath(target)
    if (typeof processCwd !== 'string' || !processCwd || processCwd.includes('\0')) {
      throw new Error('DSH Host FS returned invalid Git process path')
    }
    return processCwd
  }
  // Legacy local-only FS: check real paths and symlinks, not just lexical path.
  const cwd = requested ? resolveInside(root, requested) : root
  const [canonicalRoot, canonicalCwd] = await Promise.all([realpath(root), realpath(cwd)])
  if (!isPathInside(canonicalRoot, canonicalCwd)) throw new Error('Git cwd escapes workspace')
  if (exec.signal.aborted) throw new Error('Git workspace validation cancelled')
  return canonicalCwd
}
