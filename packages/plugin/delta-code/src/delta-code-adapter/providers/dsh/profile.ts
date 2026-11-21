import { access, readFile, realpath, readdir, stat } from 'node:fs/promises'
import { join, relative } from 'node:path'
import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import { defineTool, isPathInside, resolveInside } from '@fongap-labs/delta-plugin-shared'
import type { Settings } from '../../../types.js'
import { resolveRoot } from '../../../runtime.js'
import { requireHostFs } from './host-fs.js'
import { PROJECT_FILES, ancestorRuleDirectories, declaredScripts, declaredTechnologies, profileSummary } from '../../../delta-code-coding/project-profile-policy.js'
import type { ProjectProfile, ProjectModuleProfile } from '../../../delta-code-coding/project-profile-policy.js'
import { lookupDshHostFs } from './host-adapter.js'
import type { HostFsCapabilities } from './host-fs.js'
export type { ProjectProfile } from '../../../delta-code-coding/project-profile-policy.js'

export async function inspectProject(cwd: string, root: string): Promise<ProjectProfile> {
  const canonicalRoot = await realpath(root)
  const canonicalCwd = await realpath(cwd)
  if (!isPathInside(canonicalRoot, canonicalCwd)) throw new Error('project directory escapes workspace')

  const files: string[] = []
  for (const name of PROJECT_FILES) {
    try {
      const file = await realpath(join(canonicalCwd, name))
      if (!isPathInside(canonicalRoot, file)) continue
      await access(file)
      files.push(name)
    } catch { /* file absent or unsafe */ }
  }
  let scripts: string[] = []
  let frameworks: string[] = []
  if (files.includes('package.json')) {
    try {
      const raw = await readFile(join(canonicalCwd, 'package.json'), 'utf8')
      scripts = declaredScripts(raw)
      frameworks = declaredTechnologies(raw)
    } catch { /* profile remains useful without package.json */ }
  }
  const ancestors: string[] = []
  for (const directory of ancestorRuleDirectories(canonicalCwd, canonicalRoot)) {
    for (const name of ['AGENTS.md', 'CLAUDE.md']) {
      try {
        const path = await realpath(join(directory, name))
        if (!isPathInside(canonicalRoot, path)) continue
        await access(path)
        ancestors.push(relative(canonicalCwd, join(directory, name)))
      } catch { /* rule absent or unsafe */ }
    }
  }
  return profileSummary(canonicalCwd, files, scripts, ancestors, frameworks)
}


/** DSH FS is authoritative when mounted; do not switch to local Node reads on refusal. */
export async function inspectProjectWithHost(
  ctx: Context, exec: { signal: AbortSignal }, cwd: string, root: string,
): Promise<ProjectProfile> {
  const fs = requireHostFs(ctx, ['resolve', 'contains', 'stat', 'readText'], 'refusing local profile fallback')
  if (!fs) return inspectProject(cwd, root)
  if (exec.signal.aborted) throw new Error('project profile cancelled')
  const rootTarget = await fs.resolve(root, { cwd: root, signal: exec.signal })
  if (exec.signal.aborted) throw new Error('project profile cancelled')
  const cwdTarget = await fs.resolve(cwd, { cwd: root, signal: exec.signal })
  if (!fs.contains(rootTarget, cwdTarget)) throw new Error('project directory escapes workspace')
  if (exec.signal.aborted) throw new Error('project profile cancelled')
  const directoryInfo = await fs.stat(cwdTarget, exec.signal)
  if (!directoryInfo || directoryInfo.type !== 'directory') {
    throw new Error('project cwd is not a Host-authorized directory')
  }
  if (exec.signal.aborted) throw new Error('project profile cancelled')
  const files: string[] = []
  let packageTarget: unknown
  for (const name of PROJECT_FILES) {
    if (exec.signal.aborted) throw new Error('project profile cancelled')
    const target = await fs.resolve(join(cwd, name), { cwd, signal: exec.signal })
    if (!fs.contains(rootTarget, target)) continue
    const info = await fs.stat(target, exec.signal)
    if (exec.signal.aborted) throw new Error('project profile cancelled')
    if (info?.type !== 'file') continue
    files.push(name)
    if (name === 'package.json') packageTarget = target
  }
  let scripts: string[] = []
  let frameworks: string[] = []
  if (packageTarget !== undefined) {
    // Host read failures remain fatal: never silently use local Node data.
    const raw = await fs.readText(packageTarget, exec.signal)
    if (exec.signal.aborted) throw new Error('project profile cancelled')
    scripts = declaredScripts(raw)
    frameworks = declaredTechnologies(raw)
  }
  const ancestors: string[] = []
  for (const directory of ancestorRuleDirectories(cwd, root)) {
    if (exec.signal.aborted) throw new Error('project profile cancelled')
    const parent = await fs.resolve(directory, { cwd: root, signal: exec.signal })
    if (!fs.contains(rootTarget, parent)) continue
    if (exec.signal.aborted) throw new Error('project profile cancelled')
    const parentInfo = await fs.stat(parent, exec.signal)
    if (parentInfo?.type !== 'directory') continue
    for (const name of ['AGENTS.md', 'CLAUDE.md']) {
      if (exec.signal.aborted) throw new Error('project profile cancelled')
      const candidate = await fs.resolve(join(directory, name), { cwd: root, signal: exec.signal })
      if (!fs.contains(rootTarget, candidate)) continue
      const info = await fs.stat(candidate, exec.signal)
      if (info?.type === 'file') ancestors.push(relative(cwd, join(directory, name)))
    }
  }
  if (exec.signal.aborted) throw new Error('project profile cancelled')
  return profileSummary(cwd, files, scripts, ancestors, frameworks)
}


/** Bounded, metadata-only discovery. Root scripts remain root-scoped; child
 * scripts are never merged into a root verification command. */
const MAX_MODULE_DEPTH = 3
const MAX_MODULE_DIRECTORIES = 48
const MAX_MODULES = 24
const MODULE_MANIFESTS = new Set<string>([
  'package.json', 'Cargo.toml', 'pyproject.toml', 'requirements.txt', 'go.mod',
  'tauri.conf.json', 'tauri.conf.json5', 'Tauri.toml', 'build.gradle', 'build.gradle.kts',
  'settings.gradle', 'settings.gradle.kts',
])
const SKIP_DIRECTORIES = new Set([
  'node_modules', 'target', 'dist', 'build', 'out', 'coverage', 'vendor', 'src',
  'lib', 'public', 'assets', 'tests', 'test', 'docs', '__pycache__',
])
const PREFERRED_DIRECTORIES = new Set(['apps', 'packages', 'crates', 'src-tauri', 'frontend', 'backend',
  'client', 'server', 'desktop', 'windows', 'android', 'web', 'modules', 'services'])
type ModuleEntry = { name: string; type: string; target: unknown }
function eligibleEntry(entry: ModuleEntry): boolean {
  return typeof entry?.name === 'string' && !!entry.name && entry.name !== '.' && entry.name !== '..' &&
    !/[\\/\0]/.test(entry.name)
}
function candidateDirectories(entries: ModuleEntry[]): ModuleEntry[] {
  return entries.filter(entry => eligibleEntry(entry) && entry.type === 'directory' &&
    !entry.name.startsWith('.') && !SKIP_DIRECTORIES.has(entry.name.toLowerCase()))
    .sort((a, b) => Number(PREFERRED_DIRECTORIES.has(b.name.toLowerCase())) -
      Number(PREFERRED_DIRECTORIES.has(a.name.toLowerCase())) || a.name.localeCompare(b.name))
}
type ModuleReader = {
  root: unknown
  list(target: unknown): Promise<ModuleEntry[]>
  read(target: unknown): Promise<string | undefined>
}
async function discoverModules(reader: ModuleReader, signal: AbortSignal): Promise<{
  modules: ProjectModuleProfile[]; truncated: boolean
}> {
  const queue: Array<{ target: unknown; path: string; depth: number }> = [
    { target: reader.root, path: '', depth: 0 },
  ]
  const modules: ProjectModuleProfile[] = []
  let visited = 0
  let isTruncated = false
  while (queue.length && visited < MAX_MODULE_DIRECTORIES && modules.length < MAX_MODULES) {
    signal.throwIfAborted()
    const directory = queue.shift()!
    visited++
    const entries = await reader.list(directory.target)
    signal.throwIfAborted()
    if (directory.path) {
      const files = entries.filter(entry => eligibleEntry(entry) && entry.type === 'file' &&
        (PROJECT_FILES as readonly string[]).includes(entry.name)).map(entry => entry.name)
      if (files.some(name => MODULE_MANIFESTS.has(name))) {
        const manifest = entries.find(entry => entry.name === 'package.json' && entry.type === 'file')
        const raw = manifest ? await reader.read(manifest.target) : undefined
        signal.throwIfAborted()
        const summary = profileSummary(directory.path, files, raw ? declaredScripts(raw) : [], [],
          raw ? declaredTechnologies(raw) : [])
        modules.push({ path: directory.path, technologies: summary.technologies,
          scripts: summary.scripts, suggestedChecks: summary.suggestedChecks })
      }
    }
    if (directory.depth >= MAX_MODULE_DEPTH) continue
    for (const child of candidateDirectories(entries)) {
      if (visited + queue.length >= MAX_MODULE_DIRECTORIES) { isTruncated = true; break }
      queue.push({ target: child.target, path: directory.path ? directory.path + '/' + child.name : child.name,
        depth: directory.depth + 1 })
    }
  }
  return { modules, truncated: isTruncated || queue.length > 0 }
}

async function addModuleProfiles(ctx: Context, exec: { signal: AbortSignal }, cwd: string, root: string,
  profile: ProjectProfile): Promise<ProjectProfile> {
  const mounted = lookupDshHostFs(ctx) as HostFsCapabilities | undefined
  let reader: ModuleReader
  if (mounted) {
    // Optional listing is capability-gated. Never fall back to Node for a mounted Host.
    if (typeof mounted.listDir !== 'function') return profile
    const fs = requireHostFs(ctx, ['resolve', 'contains', 'stat', 'readText', 'listDir'],
      'refusing local module discovery fallback')!
    const boundary = await fs.resolve(root, { cwd: root, signal: exec.signal })
    const target = await fs.resolve(cwd, { cwd: root, signal: exec.signal })
    if (!fs.contains(boundary, target)) throw new Error('project directory escapes workspace')
    reader = {
      root: target,
      async list(directory) {
        exec.signal.throwIfAborted()
        if (!fs.contains(boundary, directory)) return []
        if ((await fs.stat(directory, exec.signal))?.type !== 'directory') return []
        const entries = await fs.listDir(directory, exec.signal) as ModuleEntry[]
        if (!Array.isArray(entries)) throw new Error('invalid Host project directory listing')
        return entries.filter(entry => eligibleEntry(entry) && fs.contains(boundary, entry.target))
      },
      async read(file) {
        exec.signal.throwIfAborted()
        if (!fs.contains(boundary, file)) return undefined
        const info = await fs.stat(file, exec.signal)
        if (info?.type !== 'file' || (info.size !== undefined && info.size > 1024 * 1024)) return undefined
        return fs.readText(file, exec.signal)
      },
    }
  } else {
    const boundary = await realpath(root)
    const target = await realpath(cwd)
    if (!isPathInside(boundary, target)) throw new Error('project directory escapes workspace')
    reader = {
      root: target,
      async list(directory) {
        const path = await realpath(directory as string)
        if (!isPathInside(boundary, path)) return []
        return (await readdir(path, { withFileTypes: true }))
          .filter(entry => !entry.isSymbolicLink() && (entry.isDirectory() || entry.isFile()))
          .map(entry => ({ name: entry.name, type: entry.isDirectory() ? 'directory' : 'file',
            target: join(path, entry.name) }))
      },
      async read(file) {
        const path = await realpath(file as string)
        if (!isPathInside(boundary, path)) return undefined
        const info = await stat(path)
        if (!info.isFile() || info.size > 1024 * 1024) return undefined
        return readFile(path, 'utf8')
      },
    }
  }
  const scan = await discoverModules(reader, exec.signal)
  exec.signal.throwIfAborted()
  return { ...profile, technologies: [...new Set([
    ...profile.technologies, ...scan.modules.flatMap(module => module.technologies),
  ])], modules: scan.modules, moduleScanTruncated: scan.truncated }
}

export function createProjectProfileTool(ctx: Context, settings: Settings): ToolDefinition {
  return defineTool({
    name: 'code_project_profile',
    description: 'Quick read-only project preflight: detect tech stack, project rule files and existing verification scripts, including bounded submodule discovery without reading source bodies. Use before planning or choosing tests.',
    parameters: {
      cwd: { type: 'string', description: 'Optional workspace-relative project directory.' },
    },
    output: {
      schema: { type: 'object', additionalProperties: false, properties: {
        cwd: { type: 'string', required: true },
        files: { type: 'array', required: true, items: { type: 'string' } },
        technologies: { type: 'array', required: true, items: { type: 'string' } },
        scripts: { type: 'array', required: true, items: { type: 'string' } },
        guidance: { type: 'array', required: true, items: { type: 'string' } },
        suggestedChecks: { type: 'array', required: true, items: { type: 'string' } },
        modules: { type: 'array', items: { type: 'object', additionalProperties: false, properties: {
          path: { type: 'string', required: true },
          technologies: { type: 'array', required: true, items: { type: 'string' } },
          scripts: { type: 'array', required: true, items: { type: 'string' } },
          suggestedChecks: { type: 'array', required: true, items: { type: 'string' } },
        } } },
        moduleScanTruncated: { type: 'boolean' },
      } },
      render(_args, result) {
        return [{ type: 'text', text: [
          'Project: ' + result.cwd,
          'Stack: ' + (result.technologies.join(', ') || '(unknown)'),
          'Rules: ' + (result.guidance.join(', ') || '(none within workspace root)'),
          'Scripts: ' + (result.scripts.join(', ') || '(none)'),
          'Suggested checks: ' + (result.suggestedChecks.join(', ') || '(none)'),
          'Detected files: ' + result.files.join(', '),
          ...(result.modules ?? []).map(module => 'Module ' + module.path + ': ' +
            (module.technologies.join(', ') || '(unknown)') + '; scripts: ' +
            (module.scripts.join(', ') || '(none)') + '; suggested checks: ' +
            (module.suggestedChecks.join(', ') || '(none)')),
          ...(result.moduleScanTruncated ? ['Module discovery reached its bounded scan limit; pass a specific cwd for further profiling.'] : []),
        ].join('\n') }]
      },
    },
    async execute(args, exec) {
      const root = await resolveRoot(ctx, exec, settings)
      const cwd = args.cwd ? resolveInside(root, args.cwd) : root
      const profile = await inspectProjectWithHost(ctx, exec, cwd, root)
      return addModuleProfiles(ctx, exec, cwd, root, profile)
    },
  })
}
