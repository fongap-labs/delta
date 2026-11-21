/** Delta file-access boundary: Host capabilities first, authorized local fallback otherwise. */
import { open, mkdir, lstat, realpath, stat } from 'node:fs/promises'
import { constants } from 'node:fs'
import { dirname, relative, resolve, sep } from 'node:path'
import type { Context } from '@deepseek-ai/cordis'
import type { ToolRunContext } from '@deepseek-ai/dsh-tools'
import { resolveInside, isPathInside, SandboxViolationError } from '@fongap-labs/delta-plugin-shared'
import type { Settings } from '../types.js'
import { requireHostFs } from './providers/dsh/host-fs.js'
import { resolveDshFsSandboxPolicy, writeDshHostText } from './providers/dsh/fs-write.js'
import { lookupDshWorkingDirectory } from './providers/dsh/host-adapter.js'
import type { HostFsCapabilities, HostWriteIntent } from './providers/dsh/host-fs.js'

export async function resolveRoot(
  ctx: Context,
  exec: ToolRunContext,
  settings: Settings,
): Promise<string> {
  const workingDirectory = lookupDshWorkingDirectory(ctx) as
    | { ensure: (agent: unknown, signal?: AbortSignal) => Promise<string | undefined | null> }
    | undefined
  if (workingDirectory && typeof workingDirectory.ensure !== 'function') {
    throw new Error('DSH Host workingDirectory missing ensure; refusing local fallback')
  }
  if (workingDirectory && exec.agent !== undefined) {
    // A Host error is an explicit failure, not permission to use process.cwd().
    // Only an absent Host-provided directory may use the configured fallback.
    const dir = await workingDirectory.ensure(exec.agent, exec.signal)
    if (dir) return dir
  }
  if (settings.workspaceRoot) return settings.workspaceRoot
  return process.cwd()
}


/**
 * Check the real filesystem path when the optional Host FS is unavailable.
 * Purely lexical resolveInside() is insufficient for existing symlinks.
 * The Host FS remains authoritative when it is present.
 */
async function assertLocalPathInside(root: string, target: string, isWrite: boolean): Promise<void> {
  const canonicalRoot = await realpath(root)
  if (isWrite) {
    // Refuse final-path symlinks even if their target does not yet exist.
    try {
      if ((await lstat(target)).isSymbolicLink()) {
        throw new SandboxViolationError('symbolic-link edits are not allowed', { root, target })
      }
    } catch (error) {
      if (error instanceof SandboxViolationError) throw error
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error
    }
  }
  let probe = isWrite ? dirname(target) : target
  while (true) {
    try {
      const canonicalPath = await realpath(probe)
      if (!isPathInside(canonicalRoot, canonicalPath)) {
        throw new SandboxViolationError('path escapes workspace through a symbolic link', { root, target })
      }
      return
    } catch (error) {
      if (error instanceof SandboxViolationError) throw error
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error
      const parent = dirname(probe)
      if (parent === probe) {
        throw new SandboxViolationError('cannot resolve path inside workspace', { root, target })
      }
      probe = parent
    }
  }
}

/** Validate an edit target before a batch begins.
 * Local fallback checks the real existing path; native Host FS owns its path policy.
 */
/**
 * Guarded writes use an explicit upstream FsWriteIntent. We never infer this
 * signature by making probe writes or checking function arity.
 */
/** Preserve a specific Host version or absence condition across read/modify/write. */
export type GuardedWriteIntent = HostWriteIntent

async function nativeWriteTarget(
  fs: HostFsCapabilities,
  root: string,
  absolute: string,
  signal: AbortSignal,
): Promise<unknown> {
  if (typeof fs.contains !== 'function') {
    throw new Error('native-unguarded requires DSH Host FS resolve/contains/writeText')
  }
  signal.throwIfAborted()
  const parent = await fs.resolve(root, { cwd: root, signal })
  const target = await fs.resolve(absolute, { cwd: root, signal })
  if (!fs.contains(parent, target)) {
    throw new SandboxViolationError('Host FS write target escapes workspace', { root, target: absolute })
  }
  signal.throwIfAborted()
  return target
}

async function guardedWriteTarget(
  fs: HostFsCapabilities,
  root: string,
  absolute: string,
  signal: AbortSignal,
): Promise<{ target: unknown; intent: GuardedWriteIntent }> {
  if (typeof fs.stat !== 'function') {
    throw new Error('guarded-intent requires DSH Host FS resolve/contains/stat/writeText')
  }
  const target = await nativeWriteTarget(fs, root, absolute, signal)
  const info = await fs.stat(target, signal)
  if (info && (info.type !== 'file' || typeof info.version !== 'string' || !info.version)) {
    throw new Error('guarded-intent requires a regular file with an opaque version')
  }
  signal.throwIfAborted()
  return { target, intent: info
    ? { kind: 'replaceIfVersion', version: info.version as string }
    : { kind: 'createIfAbsent' } }
}

export async function validateWriteTargetSafe(
  ctx: Context,
  exec: ToolRunContext,
  root: string,
  relPath: string,
  settings?: Pick<Settings, 'fsWriteContract'>,
): Promise<GuardedWriteIntent | undefined> {
  const absolute = resolveInside(root, relPath)
  const fs = requireHostFs(ctx, ['resolve', 'writeText'], 'refusing local fallback')
  if (fs) {
    if (settings?.fsWriteContract === 'guarded-intent') {
      const { intent } = await guardedWriteTarget(fs, root, absolute, exec.signal)
      return intent
    }
    if (settings?.fsWriteContract === 'native-unguarded' || (fs as HostFsCapabilities).sandboxMode !== undefined) {
      await nativeWriteTarget(fs, root, absolute, exec.signal)
      return undefined
    }
    await fs.resolve(absolute, { cwd: root, signal: exec.signal })
    return undefined
  }
  if (settings?.fsWriteContract === 'guarded-intent' ||
      settings?.fsWriteContract === 'native-unguarded') {
    throw new Error(settings.fsWriteContract + ' requires a mounted DSH Host FS')
  }
  await assertLocalPathInside(root, absolute, true)
}

export async function readTextSafe(
  ctx: Context,
  exec: ToolRunContext,
  root: string,
  relPath: string,
): Promise<string> {
  const absolute = resolveInside(root, relPath)
  const fs = requireHostFs(ctx, ['resolve', 'readText'], 'refusing local fallback')
  if (fs) {
    const signal = exec.signal
    const resolved = await fs.resolve(absolute, { cwd: root, signal })
    return fs.readText(resolved, signal)
  }
  if (process.platform === 'linux') return readLocalTextAnchored(root, absolute, exec.signal)
  return readLocalTextVerified(root, absolute, exec.signal)
}

/**
 * Anchor each directory traversal to an open descriptor on Linux. Unlike
 * path-based realpath/open sequences, renaming or replacing an intermediate
 * directory cannot redirect a subsequent lookup outside the opened parent.
 * Windows/macOS keep the existing compatibility fallback.
 */
async function openAnchoredLocalFile(
  root: string, absolute: string, isWrite: boolean, signal?: AbortSignal,
): Promise<Awaited<ReturnType<typeof open>>> {
  if (process.platform !== 'linux') throw new Error('anchored local file access requires Linux')
  signal?.throwIfAborted()
  const parts = relative(resolve(root), absolute).split(sep)
  if (parts.length === 0 || parts.some(part => !part || part === '.' || part === '..')) {
    throw new SandboxViolationError('anchored file path escapes workspace', { root, target: absolute })
  }
  // A trusted root is captured once; each child is opened relative to its
  // parent's descriptor. Do not use absolute names for child traversal.
  const canonicalRoot = await realpath(root)
  const rootBefore = await lstat(canonicalRoot)
  if (!rootBefore.isDirectory()) throw new Error('workspace root must be a directory')
  const opened: Array<Awaited<ReturnType<typeof open>>> = []
  try {
    const rootHandle = await open(canonicalRoot,
      constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW)
    opened.push(rootHandle)
    const rootAfter = await rootHandle.stat()
    if (!rootAfter.isDirectory() ||
        rootBefore.dev !== rootAfter.dev || rootBefore.ino !== rootAfter.ino) {
      throw new SandboxViolationError('workspace root changed while opening', { root, target: absolute })
    }
    for (const part of parts.slice(0, -1)) {
      signal?.throwIfAborted()
      const parent = opened[opened.length - 1]!
      const child = '/proc/self/fd/' + parent.fd + '/' + part
      let directory: Awaited<ReturnType<typeof open>>
      try {
        directory = await open(child,
          constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW)
      } catch (error) {
        if (!isWrite || (error as NodeJS.ErrnoException).code !== 'ENOENT') throw error
        try {
          await mkdir(child)
        } catch (mkdirError) {
          if ((mkdirError as NodeJS.ErrnoException).code !== 'EEXIST') throw mkdirError
        }
        // If another process substituted a symlink, O_NOFOLLOW refuses it.
        directory = await open(child,
          constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW)
      }
      opened.push(directory)
    }
    signal?.throwIfAborted()
    const parent = opened[opened.length - 1]!
    const target = '/proc/self/fd/' + parent.fd + '/' + parts[parts.length - 1]!
    const flags = isWrite
      ? constants.O_WRONLY | constants.O_CREAT | constants.O_NOFOLLOW
      : constants.O_RDONLY | constants.O_NOFOLLOW
    const file = await open(target, flags, 0o666)
    try {
      const metadata = await file.stat()
      if (!metadata.isFile() || (isWrite && metadata.nlink > 1)) {
        throw new SandboxViolationError('anchored target is not a regular unshared file',
          { root, target: absolute })
      }
    } catch (error) {
      await file.close()
      throw error
    }
    return file
  } catch (error) {
    // A symlink or non-directory component is a sandbox refusal, not a
    // raw /proc/self/fd path error; keep the established tool error contract.
    const code = (error as NodeJS.ErrnoException).code
    if (code === 'ELOOP' || code === 'ENOTDIR') {
      throw new SandboxViolationError('symbolic-link or invalid directory blocked by workspace sandbox',
        { root, target: absolute })
    }
    throw error
  } finally {
    for (const directory of opened.reverse()) await directory.close()
  }
}

export async function readLocalTextAnchored(
  root: string, absolute: string, signal?: AbortSignal,
): Promise<string> {
  const file = await openAnchoredLocalFile(root, absolute, false, signal)
  try {
    return await file.readFile('utf8')
  } finally {
    await file.close()
  }
}

export async function writeLocalTextAnchored(
  root: string, absolute: string, content: string, signal?: AbortSignal,
): Promise<void> {
  const file = await openAnchoredLocalFile(root, absolute, true, signal)
  try {
    signal?.throwIfAborted()
    await file.truncate(0)
    await file.writeFile(content, 'utf8')
  } finally {
    await file.close()
  }
}

/**
 * Portable fallback for Hosts without FS: open without truncation, then verify
 * the opened file matches the resolved workspace path before reading/writing.
 *
 * Unlike Linux's descriptor-relative traversal, these checks cannot close a
 * malicious parent's rename/reparse-point TOCTOU window on Windows/macOS.
 * Environments requiring strong isolation must mount an authoritative Host FS.
 */
async function openVerifiedLocalFile(
  root: string, absolute: string, isWrite: boolean, signal?: AbortSignal,
): Promise<Awaited<ReturnType<typeof open>>> {
  signal?.throwIfAborted()
  await assertLocalPathInside(root, absolute, isWrite)
  const canonicalRoot = await realpath(root)
  const canonicalParent = await realpath(dirname(absolute))
  if (!isPathInside(canonicalRoot, canonicalParent)) {
    throw new SandboxViolationError('parent directory escapes the workspace', { root, target: absolute })
  }

  // O_TRUNC is deliberately absent: no existing bytes change until the
  // post-open validation has completed. Never fall back to a path write.
  const flags = isWrite
    ? constants.O_WRONLY | constants.O_CREAT | (constants.O_NOFOLLOW ?? 0)
    : constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0)
  const handle = await open(absolute, flags, 0o666)
  try {
    const metadata = await handle.stat()
    if (!metadata.isFile() || (isWrite && metadata.nlink > 1)) {
      throw new SandboxViolationError('target is not a regular unshared file',
        { root, target: absolute })
    }
    signal?.throwIfAborted()

    // On platforms without O_NOFOLLOW, lstat and realpath catch persistent
    // symlinks and junctions, and fstat/stat detects most path substitutions.
    const named = await lstat(absolute)
    if (named.isSymbolicLink()) {
      throw new SandboxViolationError('symbolic-link file access is not allowed',
        { root, target: absolute })
    }
    const rootAfter = await realpath(root)
    const parentAfter = await realpath(dirname(absolute))
    const targetAfter = await realpath(absolute)
    if (rootAfter !== canonicalRoot || parentAfter !== canonicalParent
        || !isPathInside(rootAfter, targetAfter)) {
      throw new SandboxViolationError('workspace path changed while opening file',
        { root, target: absolute })
    }
    const namedMetadata = await stat(absolute)
    if (!namedMetadata.isFile() || metadata.dev !== namedMetadata.dev
        || metadata.ino !== namedMetadata.ino
        || (isWrite && namedMetadata.nlink > 1)) {
      throw new SandboxViolationError('opened file does not match workspace path',
        { root, target: absolute })
    }
    signal?.throwIfAborted()
    return handle
  } catch (error) {
    await handle.close()
    throw error
  }
}

/** Opened-handle read: never reopen the target by its mutable pathname. */
export async function readLocalTextVerified(
  root: string, absolute: string, signal?: AbortSignal,
): Promise<string> {
  const handle = await openVerifiedLocalFile(root, absolute, false, signal)
  try {
    return await handle.readFile('utf8')
  } finally {
    await handle.close()
  }
}

/** Opened-handle write: truncate only after the target has been validated. */
export async function writeLocalTextVerified(
  root: string, absolute: string, content: string, signal?: AbortSignal,
): Promise<void> {
  const handle = await openVerifiedLocalFile(root, absolute, true, signal)
  try {
    signal?.throwIfAborted()
    await handle.truncate(0)
    await handle.writeFile(content, 'utf8')
  } finally {
    await handle.close()
  }
}

/**
 * Open a file without following its final path component on supported OSes.
 * This narrows the check-to-open symlink race, but parent swaps remain possible.
 */
export async function writeLocalTextNoFollow(absolute: string, content: string): Promise<void> {
  const noFollow = constants.O_NOFOLLOW ?? 0
  const flags = constants.O_WRONLY | constants.O_CREAT | constants.O_TRUNC | noFollow
  const handle = await open(absolute, flags, 0o666)
  try {
    await handle.writeFile(content, 'utf8')
  } finally {
    await handle.close()
  }
}

export async function writeTextSafe(
  ctx: Context,
  exec: ToolRunContext,
  root: string,
  relPath: string,
  content: string,
  settings?: Pick<Settings, 'fsWriteContract'>,
  pinnedIntent?: GuardedWriteIntent,
): Promise<unknown> {
  if (pinnedIntent && settings?.fsWriteContract !== 'guarded-intent') {
    throw new Error('pinned write intent requires guarded-intent Host FS')
  }
  const absolute = resolveInside(root, relPath)
  const fs = requireHostFs(ctx, ['resolve', 'writeText'], 'refusing local fallback')
  if (fs) {
    const signal = exec.signal
    const policyFs = fs as HostFsCapabilities
    // Resolve at each mutation (including guarded recovery) so session mode
    // changes take effect without a plugin reload or locally cached grants.
    const resolvePolicy = () => resolveDshFsSandboxPolicy(
      ctx as unknown as { get(name: 'sandboxPolicy'): unknown }, policyFs,
      exec as unknown as { agent?: { session?: unknown } },
    )
    if (settings?.fsWriteContract === 'guarded-intent') {
      const { target, intent } = await guardedWriteTarget(fs, root, absolute, signal)
      // Never replace the preflight snapshot's version with a newer one:
      // the Host must atomically reject concurrent changes, not adopt them.
      return writeDshHostText(fs, target, content, signal, 'guarded-intent', pinnedIntent ?? intent, resolvePolicy())
    }
    if (settings?.fsWriteContract === 'native-unguarded') {
      const target = await nativeWriteTarget(fs, root, absolute, signal)
      return writeDshHostText(fs, target, content, signal, 'native-unguarded', undefined, resolvePolicy())
    }
    // Sandbox-aware providers must retain Delta's own workspace boundary,
    // even when the Host grants the session broader permissions.
    const resolved = policyFs.sandboxMode !== undefined
      ? await nativeWriteTarget(fs, root, absolute, signal)
      : await fs.resolve(absolute, { cwd: root, signal })
    await writeDshHostText(fs, resolved, content, signal, 'legacy-overwrite', undefined, resolvePolicy())
    return
  }
  if (settings?.fsWriteContract === 'guarded-intent' ||
      settings?.fsWriteContract === 'native-unguarded') {
    throw new Error(settings.fsWriteContract + ' requires a mounted DSH Host FS')
  }
  if (process.platform === 'linux') {
    await writeLocalTextAnchored(root, absolute, content, exec.signal)
    return
  }
  await assertLocalPathInside(root, absolute, true)
  await mkdir(dirname(absolute), { recursive: true })
  // Check again after creation and before opening the verified handle.
  await assertLocalPathInside(root, absolute, true)
  await writeLocalTextVerified(root, absolute, content, exec.signal)
}
