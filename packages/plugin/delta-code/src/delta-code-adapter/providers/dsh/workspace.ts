import { lstat, readdir, realpath } from 'node:fs/promises'
import { join, relative, sep, posix, win32 } from 'node:path'
import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition, ToolRunContext } from '@deepseek-ai/dsh-tools'
import { defineTool, runCommand, isSubprocessMounted, resolveInside, isPathInside } from '@fongap-labs/delta-plugin-shared'
import type { Settings } from '../../../types.js'
import { resolveRoot } from '../../../runtime.js'
import { resolveAuthorizedGitCwd } from './host-git-cwd.js'
import { requireHostFs } from './host-fs.js'
import { lookupDshHostFs } from './host-adapter.js'
import type { HostFs } from './host-fs.js'
import { DEFAULT_LIMIT, MAX_LIMIT, boundedDirectories, assertRepositoryCapacity, prefixTrackedPaths, parseBatchQueries, workspaceMapResult } from '../../../delta-code-repository/workspace-policy.js'
import { isInternalWorktreePath } from '../../../delta-code-repository/worktree-storage.js'

/** Refuse a symbolic-link .git marker; accept both directory repos and linked worktree files. */
async function hasGitMarker(directory: string): Promise<boolean> {
  try {
    const marker = await lstat(join(directory, '.git'))
    return marker.isFile() || marker.isDirectory()
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return false
    throw error
  }
}

async function discoverDirectRepositories(root: string, signal: AbortSignal): Promise<string[]> {
  signal.throwIfAborted()
  const children = boundedDirectories((await readdir(root, { withFileTypes: true }))
    .filter((entry) => entry.isDirectory() && !entry.isSymbolicLink()))
  const repositories: string[] = []
  for (const entry of children) {
    signal.throwIfAborted()
    const child = join(root, entry.name)
    const canonicalChild = await realpath(child)
    if (!isPathInside(root, canonicalChild) || canonicalChild === root) continue
    if (!(await hasGitMarker(child))) continue
    assertRepositoryCapacity(repositories.length)
    repositories.push(child)
  }
  return repositories
}

interface TrackedFiles {
  isGitRepository: boolean
  paths: string[]
}

/**
 * Only the DSH-controlled subprocess may enumerate tracked files when mounted.
 * Non-Git exit codes are distinguished from provider denial (which throws).
 */
async function listTrackedFiles(
  ctx: Context, settings: Settings, directory: string, signal: AbortSignal,
): Promise<TrackedFiles> {
  const command = await runCommand(ctx, {
    command: settings.gitCommand, args: ['ls-files', '-z', '--cached'],
    cwd: directory, signal, timeoutMs: 15_000, maxOutputBytes: 1024 * 1024,
    hostOnly: Boolean(lookupDshHostFs(ctx)) || isSubprocessMounted(ctx),
  })
  if (command.timedOut || command.signal || command.lossy) {
    throw new Error('unable to enumerate tracked files without timeout or truncation')
  }
  if (command.exitCode !== 0) return { isGitRepository: false, paths: [] }
  if (command.stdout && !command.stdout.endsWith('\0')) {
    throw new Error('incomplete Git tracked-file evidence')
  }
  const paths = command.stdout ? command.stdout.slice(0, -1).split('\0') : []
  // Paths returned by an external Git provider are not an authorization
  // grant. Reject malformed/escaping names before displaying or joining.
  for (const path of paths) {
    if (!path || path.includes('\\') || path.startsWith('/') ||
        path.split('/').some(segment => !segment || segment === '.' || segment === '..')) {
      throw new Error('unsafe Git tracked-file path')
    }
  }
  return { isGitRepository: true, paths: paths.filter(path => !isInternalWorktreePath(path)) }
}

/**
 * Common post-authorization indexing for discovered repositories.
 * The caller must validate the directory using its own Host or Node FS.
 */
async function indexDiscoveredRepository(
  ctx: Context, settings: Settings, processCwd: string, displayPrefix: string, signal: AbortSignal,
): Promise<string[]> {
  const tracked = await listTrackedFiles(ctx, settings, processCwd, signal)
  if (!tracked.isGitRepository) {
    throw new Error('discovered Git repository could not be indexed; check Host permission')
  }
  return prefixTrackedPaths(tracked.paths, displayPrefix)
}

interface HostWorkspaceEntry {
  name: string
  type: string
  target: unknown
}
/** Required Host capability set for discovery. */
type WorkspaceHostFs = HostFs<'resolve' | 'contains' | 'listDir' | 'lstat' | 'processPath'>

/** Discover only immediate Host FS directories; never probe Node's filesystem. */
async function discoverHostRepositories(
  ctx: Context, settings: Settings, root: string, fs: WorkspaceHostFs,
  rootTarget: unknown, children: HostWorkspaceEntry[],
  signal: AbortSignal, exec: ToolRunContext,
): Promise<{ repositories: string[]; paths: string[] }> {
  signal.throwIfAborted()
  // The root listing was already validated during the .git marker check.
  // Reuse it rather than asking the Host to enumerate the same root twice.
  const candidates = boundedDirectories(children.filter(entry => entry.type === 'directory'))
  const repositories: string[] = []
  const paths: string[] = []
  for (const entry of candidates) {
    signal.throwIfAborted()
    if (!entry.name || entry.name === '.' || entry.name === '..' ||
        entry.name.includes('/') || entry.name.includes('\\') || entry.name.includes('\0')) {
      throw new Error('invalid Host directory entry')
    }
    const info = await fs.lstat(entry.name, { cwd: root }, signal)
    if (info?.type !== 'directory') continue // no symlinked child repositories
    if (!fs.contains(rootTarget, entry.target)) throw new Error('Host repository escapes workspace')
    const contents = (await fs.listDir(entry.target, signal)) as HostWorkspaceEntry[]
    if (!Array.isArray(contents)) throw new Error('invalid Host repository listing')
    const marker = contents.find(child => child.name === '.git')
    if (!marker || (marker.type !== 'file' && marker.type !== 'directory')) continue
    const markerInfo = await fs.lstat('.git', { cwd: fs.processPath(entry.target) }, signal)
    if (markerInfo?.type !== marker.type || !fs.contains(rootTarget, marker.target)) continue
    assertRepositoryCapacity(repositories.length)
    const processCwd = await resolveAuthorizedGitCwd(ctx, root, entry.name, exec)
    const indexed = await indexDiscoveredRepository(ctx, settings, processCwd, entry.name, signal)
    repositories.push(entry.name)
    paths.push(...indexed)
  }
  return { repositories, paths }
}

/** Read only tracked paths; scan at most one level when the workspace is a multi-repo parent. */
export function createWorkspaceMapTool(ctx: Context, settings: Settings): ToolDefinition {
  return defineTool({
    name: 'code_workspace_map',
    description: 'Return a compact ranked map of tracked Git paths. Use queries to search up to 8 terms from one fresh index read; otherwise use legacy query. Never caches paths across calls. Discovers only immediate child repositories.',
    parameters: {
      query: { type: 'string', description: 'Optional single search; cannot be combined with queries.' },
      queries: { type: 'array', description: 'Optional 1..8 distinct searches sharing one fresh Git enumeration; results are capped at 250 paths total.',
        items: { type: 'string' } },
      cwd: { type: 'string', description: 'Optional workspace-relative or authorized absolute subdirectory or task worktree. When provided, only this directory is indexed.' },
      limit: { type: 'integer', description: 'Maximum paths, 1..250; defaults to 100.' },
    },
    output: {
      schema: { type: 'object', additionalProperties: false, properties: {
        total: { type: 'integer', required: true },
        selected: { type: 'array', required: true, items: { type: 'string' } },
        truncated: { type: 'boolean', required: true },
        repositories: { type: 'array', required: true, items: { type: 'string' } },
        queryResults: { type: 'array', required: true, items: {
          type: 'object', additionalProperties: false, properties: {
            query: { type: 'string', required: true },
            selected: { type: 'array', required: true, items: { type: 'string' } },
            truncated: { type: 'boolean', required: true },
          },
        } },
      } },
      render(_args, result) {
        const header = 'Repositories: ' + result.repositories.join(', ') +
          '\nTracked files: ' + result.total
        if (result.queryResults.length) {
          return [{ type: 'text', text: header + '\n' + result.queryResults.map(item =>
            'Query: ' + item.query + '\n' + item.selected.join('\n') +
            (item.truncated ? '\n(more matching files omitted)' : '')).join('\n') }]
        }
        return [{ type: 'text', text: header + '\n' + result.selected.join('\n') +
          (result.truncated ? '\n(more files omitted or did not match query)' : '') }]
      },
    },
    async execute(args, exec) {
      const limit = args.limit ?? DEFAULT_LIMIT
      if (!Number.isSafeInteger(limit) || limit < 1 || limit > MAX_LIMIT) {
        throw new Error('limit must be an integer between 1 and 250')
      }
      const queries = parseBatchQueries(args.queries, args.query)
      const root = await resolveRoot(ctx, exec, settings)
      const fs = requireHostFs(ctx, ['resolve', 'contains', 'listDir', 'lstat', 'processPath'], 'refusing local discovery fallback')
      if (fs) {
        // Resolve both paths through Host FS. Derive display prefixes from
        // canonical process paths, not model-supplied absolute paths.
        const processCwd = await resolveAuthorizedGitCwd(ctx, root, args.cwd, exec)
        let prefix = ''
        if (args.cwd) {
          const processRoot = await resolveAuthorizedGitCwd(ctx, root, undefined, exec)
          const pathApi = win32.isAbsolute(processRoot) && !posix.isAbsolute(processRoot) ? win32 : posix
          const relativeCwd = pathApi.relative(processRoot, processCwd)
          if (pathApi.isAbsolute(relativeCwd) || relativeCwd === '..' ||
              relativeCwd.startsWith('..' + pathApi.sep) || relativeCwd.includes('\0')) {
            throw new Error('Git process cwd escapes workspace')
          }
          prefix = relativeCwd.split(pathApi.sep).join('/')
        }
        const base = await listTrackedFiles(ctx, settings, processCwd, exec.signal)
        let repositories = [prefix || '.']
        let paths: string[]
        if (base.isGitRepository) {
          paths = prefixTrackedPaths(base.paths, prefix)
        } else {
          if (args.cwd) throw new Error('specified Git repository could not be indexed; check cwd and Host permission')
          const rootTarget = await fs.resolve(root, { cwd: root, signal: exec.signal })
          const rootEntries = (await fs.listDir(rootTarget, exec.signal)) as HostWorkspaceEntry[]
          if (!Array.isArray(rootEntries)) throw new Error('invalid Host workspace directory listing')
          const rootMarker = rootEntries.find(entry => entry.name === '.git')
          if (rootMarker && (rootMarker.type === 'file' || rootMarker.type === 'directory')) {
            throw new Error('specified Git repository could not be indexed; check cwd and Host permission')
          }
          const scan = await discoverHostRepositories(ctx, settings, root, fs, rootTarget, rootEntries, exec.signal, exec)
          repositories = scan.repositories
          paths = scan.paths
          if (!repositories.length) {
            throw new Error('no Git repository at workspace root or in immediate subdirectories; specify cwd')
          }
        }
        exec.signal.throwIfAborted()
        return workspaceMapResult(paths, repositories, args.query ?? '', limit, queries)
      }
      const cwd = args.cwd ? resolveInside(root, args.cwd) : root
      const canonicalRoot = await realpath(root)
      const canonicalCwd = await realpath(cwd)
      if (!isPathInside(canonicalRoot, canonicalCwd)) {
        throw new Error('workspace directory escapes the repository')
      }
      const base = await listTrackedFiles(ctx, settings, canonicalCwd, exec.signal)
      let repositories = [canonicalCwd]
      let paths: string[] = []
      if (base.isGitRepository) {
        paths = prefixTrackedPaths(base.paths, relative(canonicalRoot, canonicalCwd).split(sep).join('/'))
      } else {
        // Do not turn a damaged/denied explicit repository into an unrequested search.
        if (args.cwd || await hasGitMarker(canonicalCwd)) {
          throw new Error('specified Git repository could not be indexed; check cwd and Host permission')
        }
        repositories = await discoverDirectRepositories(canonicalRoot, exec.signal)
        if (!repositories.length) {
          throw new Error('no Git repository at workspace root or in immediate subdirectories; specify cwd')
        }
        for (const directory of repositories) {
          exec.signal.throwIfAborted()
          const prefix = relative(canonicalRoot, directory).split(sep).join('/')
          paths.push(...await indexDiscoveredRepository(ctx, settings, directory, prefix, exec.signal))
        }
      }
      return workspaceMapResult(paths,
        repositories.map(directory => relative(canonicalRoot, directory).split(sep).join('/') || '.'),
        args.query ?? '', limit, queries)
    },
  })
}
