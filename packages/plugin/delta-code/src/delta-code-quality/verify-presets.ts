export type VerificationBaseScript = 'test' | 'typecheck' | 'build' | 'lint' | 'check'
export type VerificationScript = VerificationBaseScript | `${VerificationBaseScript}:${string}`
const BASE_SCRIPTS = new Set<string>(['test', 'typecheck', 'build', 'lint', 'check'])
const SAFE_SCRIPT = /^(?:test|typecheck|build|lint|check)(?::[a-z0-9][a-z0-9_-]{0,63})?$/
/** Named variants run only if declared in the selected Node project's package.json. */
export function isAllowedVerificationScript(script: unknown): script is VerificationScript {
  return typeof script === 'string' && SAFE_SCRIPT.test(script)
}
function isBaseVerificationScript(script: string): script is VerificationBaseScript {
  return BASE_SCRIPTS.has(script)
}
export type VerificationProject = 'auto' | 'node' | 'rust' | 'python' | 'go'

export interface VerificationCommand {
  project: Exclude<VerificationProject, 'auto'>
  command: string
  args: string[]
}

const PRESETS: Record<Exclude<VerificationProject, 'auto' | 'node'>, Partial<Record<VerificationBaseScript, [string, ...string[]]>>> = {
  rust: {
    test: ['cargo', 'test'],
    typecheck: ['cargo', 'check'],
    check: ['cargo', 'check'],
    lint: ['cargo', 'clippy', '--all-targets', '--', '-D', 'warnings'],
    build: ['cargo', 'build'],
  },
  python: {
    test: ['python', '-m', 'pytest'],
    check: ['python', '-m', 'compileall', '-q', '.'],
    lint: ['python', '-m', 'ruff', 'check', '.'],
    typecheck: ['python', '-m', 'mypy', '.'],
    build: ['python', '-m', 'build'],
  },
  go: {
    test: ['go', 'test', './...'],
    check: ['go', 'vet', './...'],
    lint: ['go', 'vet', './...'],
    build: ['go', 'build', './...'],
    typecheck: ['go', 'test', '-run', '^$', './...'],
  },
}

/** Select only a known command: never execute raw model-supplied shell strings. */
export function chooseVerificationCommand(
  script: VerificationScript,
  project: VerificationProject,
  present: readonly Exclude<VerificationProject, 'auto'>[],
  manager: 'npm' | 'pnpm' | 'bun' | 'yarn' = 'npm',
  nodeScripts: readonly string[] = [],
  isRustLocked = false,
): VerificationCommand {
  if (!isAllowedVerificationScript(script)) throw new Error('script is not allowlisted')
  const discovered = [...new Set(present)]
  if (project !== 'auto' && !discovered.includes(project)) {
    throw new Error(project + ' project manifest not found in the selected directory')
  }
  if (project === 'auto' && discovered.length !== 1) {
    throw new Error(discovered.length === 0
      ? 'no supported project manifest in the selected directory'
      : 'multiple project types detected; select project=node, rust, python or go explicitly')
  }
  const selected = project === 'auto' ? discovered[0]! : project
  if (selected === 'node') {
    if (!nodeScripts.includes(script)) throw new Error('package.json does not declare script ' + script)
    return { project: 'node', command: manager, args: ['run', script] }
  }
  if (!isBaseVerificationScript(script)) throw new Error('named verification scripts require a Node package.json declaration')
  const preset = PRESETS[selected]?.[script]
  if (!preset) throw new Error(selected + ' does not provide a safe preset for ' + script)
  const args = preset.slice(1)
  if (selected === 'rust' && isRustLocked && script !== 'lint') args.push('--locked')
  return { project: selected, command: preset[0], args }
}

/** A focused pass is advisory; a complete delivery gate still validates final changes. */
export interface ImpactVerificationSelection {
  scope: 'full' | 'focused'
  selected: VerificationScript[]
  deferred: VerificationScript[]
  reason: string
}

const CODE_EXTENSIONS = new Set(['ts', 'tsx', 'js', 'jsx', 'mjs', 'cjs', 'rs', 'py', 'go'])
const UI_EXTENSIONS = new Set(['tsx', 'jsx', 'vue', 'svelte', 'css', 'scss', 'less', 'html'])
const STYLE_EXTENSIONS = new Set(['css', 'scss', 'less', 'html'])
const PROJECT_METADATA = /(?:^|\/)(?:\.github|package\.json|(?:pnpm-lock\.yaml|package-lock\.json|yarn\.lock|bun\.lockb?|Cargo\.(?:toml|lock)|go\.(?:mod|sum)|pyproject\.toml|requirements(?:-[^/]*)?\.txt)|tsconfig(?:\.[^/]*)?\.json|(?:vite|webpack|rollup|vitest|jest|eslint|prettier)\.config\.[^/]+)(?:\/|$)/i

/**
 * Only narrows explicitly supplied, allowlisted verification candidates.
 * Changed paths are caller-provided hints, not a verified Git diff. Unknown or
 * cross-language changes retain the complete suite instead of guessing.
 */
export function selectImpactVerification(
  scripts: readonly VerificationScript[], changedPaths?: unknown,
): ImpactVerificationSelection {
  const full = (reason: string): ImpactVerificationSelection => ({
    scope: 'full', selected: [...scripts], deferred: [], reason,
  })
  if (changedPaths === undefined) return full('Complete verification requested.')
  if (!Array.isArray(changedPaths) || changedPaths.length < 1 || changedPaths.length > 64) {
    throw new Error('changedPaths must contain 1..64 repository-relative paths')
  }
  const paths: string[] = []
  for (const value of changedPaths) {
    if (typeof value !== 'string' || value.length < 1 || value.length > 240 ||
        value.startsWith('/') || /^[a-zA-Z]:/.test(value) || value.includes('\\') ||
        value.includes('\0') || value.split('/').some(segment => !segment || segment === '.' || segment === '..')) {
      throw new Error('changedPaths must contain safe repository-relative paths')
    }
    paths.push(value)
  }
  if (new Set(paths).size !== paths.length) throw new Error('changedPaths must not contain duplicates')
  const types = new Set<string>()
  let hasUi = false
  let hasStyle = false
  for (const path of paths) {
    if (PROJECT_METADATA.test(path)) return full('Project metadata or CI changes require the complete suite.')
    const normalized = path.toLowerCase()
    const ext = normalized.split('/').at(-1)?.split('.').at(-1) ?? ''
    if (!CODE_EXTENSIONS.has(ext) && !UI_EXTENSIONS.has(ext)) {
      return full('Unclassified change requires the complete suite.')
    }
    const kind = ext === 'rs' ? 'rust' : ext === 'py' ? 'python' : ext === 'go' ? 'go' : 'node'
    types.add(kind)
    hasUi ||= UI_EXTENSIONS.has(ext)
    hasStyle ||= STYLE_EXTENSIONS.has(ext)
  }
  if (types.size !== 1) return full('Cross-language changes require the complete suite.')

  const has = (script: VerificationScript): boolean => scripts.includes(script)
  const selected = new Set<VerificationScript>()
  const first = (...candidates: VerificationScript[]): VerificationScript | undefined =>
    candidates.find(has)
  if (!hasStyle) {
    const staticCheck = first('typecheck', 'check')
    if (staticCheck) selected.add(staticCheck)
  }
  const preferredTest = first('test:unit', 'test')
  const hasTestCandidate = scripts.some(script => script === 'test' || script.startsWith('test:'))
  if (hasTestCandidate && !preferredTest) {
    return full('No declared test:unit or test candidate supports a safe focused pass.')
  }
  if (preferredTest) selected.add(preferredTest)
  if (hasUi && has('build')) selected.add('build')
  if (hasStyle && !has('build')) return full('UI style changes require a build candidate.')
  if (!selected.size) return full('No safe focused verification candidate was supplied.')
  const chosen = scripts.filter(script => selected.has(script))
  if (chosen.length === scripts.length) return full('All declared checks remain necessary.')
  return {
    scope: 'focused', selected: chosen,
    deferred: scripts.filter(script => !selected.has(script)),
    reason: 'Focused checks selected from caller-provided paths; not Git-authenticated completion evidence.',
  }
}
