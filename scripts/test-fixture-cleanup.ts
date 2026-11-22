/**
 * Junction-safe fixture cleanup for Windows. Test fixtures junction the REAL
 * `scripts/`, `node_modules`, and tsx package directories so installer probes
 * resolve through them; Windows recursive deletion — both Node's `rmSync` and
 * Git's `worktree remove` — follows MOUNT_POINT junctions into their targets
 * and would delete the repository's own directories. POSIX `unlink`/`rm`
 * already remove symlinks without following them, so the walk is a no-op
 * there.
 */

import { accessSync, constants, lstatSync, readdirSync, rmSync, unlinkSync } from 'node:fs'
import { homedir, tmpdir } from 'node:os'
import { dirname, isAbsolute, join, parse, relative, sep } from 'node:path'
import { canonicalPath, writableRoots } from '@deepseek-ai/dsh-sandbox'

/**
 * Recursively unlink every symbolic link (junction) under `path`.
 * @param path - the fixture tree whose reparse points are unlinked.
 */
export function unlinkFixtureLinks(path: string): void {
  const visit = (entry: string): void => {
    let stat: ReturnType<typeof lstatSync>
    try {
      stat = lstatSync(entry)
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') return
      throw error
    }
    if (stat.isSymbolicLink() || !stat.isDirectory()) {
      if (stat.isSymbolicLink()) unlinkSync(entry)
      return
    }
    for (const child of readdirSync(entry)) visit(join(entry, child))
  }
  visit(path)
}

/**
 * Remove one fixture tree after its junctions are unlinked (see
 * {@link unlinkFixtureLinks}). Retries the removal: Windows releases child
 * process and antivirus file handles asynchronously, and an unretried
 * `rmSync` fails immediately with EPERM under load. A 10-second retry window
 * (50 attempts × 200 ms) covers the failover pool's slow handle release;
 * release is one-shot (a terminated child's handles drain, not reacquired),
 * so a bounded window suffices and never pins afterEach cleanup.
 * @param path - the fixture tree to remove.
 */
export function removeFixtureSafely(path: string): void {
  unlinkFixtureLinks(path)
  rmSync(path, { recursive: true, force: true, maxRetries: 50, retryDelay: 200 })
}

/** Selection and containment helpers shared by filesystem sandbox test fixtures. */
function contains(root: string, path: string): boolean {
  const suffix = relative(root, path)
  return suffix === '' || suffix !== '..' && !suffix.startsWith('..' + sep) && !isAbsolute(suffix)
}

/**
 * Select a writable temp sibling parent, or home when that parent is unavailable for writes.
 * The caller atomically allocates and owns cleanup of the generated workspace.
 * @param tempRoot - platform temporary directory.
 * @param home - fallback when temp siblings require a system directory or a non-writable parent.
 * @returns existing parent outside the automatic temporary write grants.
 */
export function outsideTempWorkspaceParent(tempRoot = tmpdir(), home = homedir()): string {
  const temporary = canonicalPath(tempRoot)
  const systemTemporary = canonicalPath('/tmp')
  const parent = dirname(temporary)
  if (temporary === systemTemporary || parent === parse(parent).root || contains(systemTemporary, parent)) return home
  try {
    accessSync(parent, constants.W_OK)
  } catch (error) {
    // A non-writable parent cannot host siblings; allocation failures still propagate from mkdtemp.
    const code = (error as NodeJS.ErrnoException).code
    if (code === 'EACCES' || code === 'EPERM' || code === 'EROFS') return home
    throw error
  }
  return parent
}

/**
 * Reject a workspace whose write could succeed without the session's workspace grant.
 * @param cwd - allocated workspace to check, with symlinks resolved before comparison.
 * @returns nothing; throws when an automatic temporary write grant contains the workspace.
 */
export function assertWorkspaceOutsideTemp(cwd: string): void {
  const path = canonicalPath(cwd)
  for (const root of writableRoots({ mode: 'workspace-write', workspaceRoot: '/tmp' })) {
    if (contains(root, path)) throw new Error('snapshot workspace ' + cwd + ' must be outside temporary writable root ' + root)
  }
}
