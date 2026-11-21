import { dirname, isAbsolute, relative, resolve, sep } from 'node:path'

/** Host-independent project metadata classification; performs no filesystem access. */
export const PROJECT_FILES = [
  'AGENTS.md', 'CLAUDE.md', 'README.md', 'package.json', 'pnpm-lock.yaml',
  'yarn.lock', 'bun.lock', 'bun.lockb', 'package-lock.json',
  'Cargo.toml', 'Cargo.lock', 'pyproject.toml', 'uv.lock', 'requirements.txt',
  'go.mod', 'go.sum', 'Makefile', 'justfile', 'tsconfig.json',
  'tauri.conf.json', 'tauri.conf.json5', 'Tauri.toml', 'build.gradle', 'build.gradle.kts',
  'settings.gradle', 'settings.gradle.kts', 'pytest.ini', 'vitest.config.ts', 'vitest.config.mts',
  'jest.config.js', 'eslint.config.js', '.github/workflows/ci.yml',
] as const
/** List at most eight authorized ancestor directories, nearest first, within workspace root. */
export function ancestorRuleDirectories(cwd: string, root: string): string[] {
  const base = resolve(root)
  let current = resolve(cwd)
  const relativeCwd = relative(base, current)
  if (!relativeCwd || relativeCwd === '..' ||
      relativeCwd.startsWith('..' + sep) || isAbsolute(relativeCwd)) return []
  const dirs: string[] = []
  for (let depth = 0; depth < 8 && current !== base; depth++) {
    current = dirname(current)
    const fromRoot = relative(base, current)
    if (fromRoot === '..' || fromRoot.startsWith('..' + sep) || isAbsolute(fromRoot)) break
    dirs.push(current)
  }
  return dirs
}

const SCRIPT_NAMES = ['test', 'typecheck', 'lint', 'check', 'build'] as const
const MAX_DESCRIPTION = 240

export interface ProjectModuleProfile {
  path: string
  technologies: string[]
  scripts: string[]
  suggestedChecks: string[]
}

export interface ProjectProfile {
  cwd: string
  files: string[]
  technologies: string[]
  scripts: string[]
  guidance: string[]
  suggestedChecks: string[]
  modules?: ProjectModuleProfile[]
  moduleScanTruncated?: boolean
}

function technologies(files: Set<string>): string[] {
  const found: string[] = []
  if (files.has('package.json')) found.push('node')
  if (files.has('tsconfig.json')) found.push('typescript')
  if (files.has('Cargo.toml')) found.push('rust')
  if (files.has('pyproject.toml') || files.has('requirements.txt')) found.push('python')
  if (files.has('go.mod')) found.push('go')
  if (['tauri.conf.json', 'tauri.conf.json5', 'Tauri.toml'].some(name => files.has(name))) found.push('tauri')
  if (['build.gradle', 'build.gradle.kts', 'settings.gradle', 'settings.gradle.kts'].some(name => files.has(name))) found.push('gradle')
  return found
}

/** Pure metadata extraction shared by local and Host-authoritative readers. */
export function declaredScripts(raw: string): string[] {
  if (Buffer.byteLength(raw, 'utf8') > 1024 * 1024) return []
  try {
    const parsed: unknown = JSON.parse(raw)
    const defined = (parsed as { scripts?: unknown } | null)?.scripts
    if (!defined || typeof defined !== 'object' || Array.isArray(defined)) return []
    return SCRIPT_NAMES.filter(name =>
      typeof (defined as Record<string, unknown>)[name] === 'string')
  } catch {
    return []
  }
}

/** Extract only recognized framework names, never expose dependency values or command bodies. */
export function declaredTechnologies(raw: string): string[] {
  if (Buffer.byteLength(raw, 'utf8') > 1024 * 1024) return []
  try {
    const parsed = JSON.parse(raw) as { dependencies?: Record<string, unknown>; devDependencies?: Record<string, unknown> } | null
    const deps = new Set([...Object.keys(parsed?.dependencies ?? {}), ...Object.keys(parsed?.devDependencies ?? {})])
    return [
      ...(deps.has('vue') ? ['vue'] : []),
      ...(deps.has('react') ? ['react'] : []),
      ...(deps.has('typescript') ? ['typescript'] : []),
      ...(deps.has('@tauri-apps/api') || deps.has('@tauri-apps/cli') ? ['tauri'] : []),
    ]
  } catch { return [] }
}

export function profileSummary(cwd: string, files: string[], scripts: string[], ancestorGuidance: string[] = [],
  frameworkNames: string[] = []): ProjectProfile {
  const suggestedChecks: string[] = scripts.filter(name =>
    name === 'typecheck' || name === 'lint' || name === 'check' || name === 'test')
  if (suggestedChecks.length === 0 && scripts.includes('build')) suggestedChecks.push('build')
  return {
    cwd: cwd.slice(0, MAX_DESCRIPTION),
    files, technologies: [...new Set([...technologies(new Set(files)), ...frameworkNames])], scripts,
    guidance: [...files.filter(name => name === 'AGENTS.md' || name === 'CLAUDE.md'), ...ancestorGuidance],
    suggestedChecks,
  }
}
