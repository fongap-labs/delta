/** Remove upstream test suites that installed npm packages never load at runtime. */

import { existsSync, readFileSync, readdirSync, rmSync } from 'node:fs'
import { extname, join } from 'node:path'

const TEST_DIRECTORIES = new Set([
  'test', 'tests', '__tests__', 'fixture', 'fixtures', 'testdata', 'bench', 'benchmark', 'benchmarks',
])
const SOURCE_EXTENSIONS = new Set(['.js', '.cjs', '.mjs', '.jsx', '.ts', '.cts', '.mts', '.tsx', '.json'])
const ENTRY_FIELDS = ['main', 'module', 'browser', 'exports', 'imports', 'bin', 'types', 'typings']
const PACKAGE_MANAGER_ENTRIES = new Set(['.bin', '.pnpm'])

interface PackageFile { path: string; candidate: string | undefined }

/**
 * Match a quoted string that names `name` as a whole path segment, such as
 * `./test/parse.js`, `../tests`, `fixtures/schema.json` or a bare `'test'`
 * passed to `path.join`. Bare names are deliberately conservative: a package
 * that also compares `NODE_ENV === 'test'` simply keeps its test directory.
 */
function referencePattern(name: string): RegExp {
  const segment = name.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&')
  const quote = '["\'`]'
  const text = '[^"\'`\\r\\n]*'
  return new RegExp(`${quote}(?:${text}[\\\\/])?${segment}(?:[\\\\/]${text})?${quote}`, 'u')
}

function collect(root: string, directory: string, candidate: string | undefined, files: PackageFile[], candidates: Set<string>): void {
  for (const entry of readdirSync(join(root, directory), { withFileTypes: true })) {
    const path = directory ? `${directory}/${entry.name}` : entry.name
    if (entry.isDirectory()) {
      // Nested dependency containers are separate packages with their own references.
      if (entry.name === 'node_modules') continue
      const owner = candidate ?? (TEST_DIRECTORIES.has(entry.name) ? path : undefined)
      if (owner === path) candidates.add(path)
      collect(root, path, owner, files, candidates)
    } else if (entry.isFile()) {
      files.push({ path, candidate })
    }
  }
}

function prunePackage(root: string, prefix: string, removed: string[], isDryRun: boolean): void {
  const files: PackageFile[] = []
  const candidates = new Set<string>()
  collect(root, '', undefined, files, candidates)
  if (candidates.size === 0) return
  const runtimeSources: string[] = []
  try {
    const manifest = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')) as Record<string, unknown>
    runtimeSources.push(JSON.stringify(ENTRY_FIELDS.map(field => manifest[field] ?? null)))
  } catch {
    // An unreadable manifest gives no evidence that test directories are unused.
    return
  }
  const candidateSources = new Map<string, string[]>([...candidates].map(candidate => [candidate, []]))
  for (const file of files) {
    if (file.path === 'package.json' || !SOURCE_EXTENSIONS.has(extname(file.path).toLowerCase())) continue
    const text = readFileSync(join(root, file.path), 'utf8')
    if (file.candidate === undefined) runtimeSources.push(text)
    else candidateSources.get(file.candidate)?.push(text)
  }
  // A non-runtime directory only counts as a referrer once runtime code keeps it.
  const kept = new Set<string>()
  const patterns = new Map<string, RegExp>()
  let hasChanged = true
  while (hasChanged) {
    hasChanged = false
    const sources = [...runtimeSources, ...[...kept].flatMap(candidate => candidateSources.get(candidate) ?? [])]
    for (const candidate of candidates) {
      if (kept.has(candidate)) continue
      const name = candidate.slice(candidate.lastIndexOf('/') + 1)
      const pattern = patterns.get(name) ?? referencePattern(name)
      patterns.set(name, pattern)
      if (sources.some(source => pattern.test(source))) {
        kept.add(candidate)
        hasChanged = true
      }
    }
  }
  for (const candidate of [...candidates].sort()) {
    if (kept.has(candidate)) continue
    if (!isDryRun) rmSync(join(root, candidate), { recursive: true })
    removed.push(`${prefix}${candidate}`)
  }
}

function visitContainer(container: string, prefix: string, removed: string[], isDryRun: boolean): void {
  for (const entry of readdirSync(container, { withFileTypes: true })) {
    if (!entry.isDirectory() || PACKAGE_MANAGER_ENTRIES.has(entry.name)) continue
    if (entry.name.startsWith('@')) {
      for (const scoped of readdirSync(join(container, entry.name), { withFileTypes: true })) {
        if (scoped.isDirectory()) visitPackage(join(container, entry.name, scoped.name), `${prefix}${entry.name}/${scoped.name}/`, removed, isDryRun)
      }
    } else {
      visitPackage(join(container, entry.name), `${prefix}${entry.name}/`, removed, isDryRun)
    }
  }
}

function visitPackage(directory: string, prefix: string, removed: string[], isDryRun: boolean): void {
  if (!existsSync(join(directory, 'package.json'))) {
    visitTree(directory, prefix, removed, isDryRun)
    return
  }
  prunePackage(directory, prefix, removed, isDryRun)
  const nested = join(directory, 'node_modules')
  if (existsSync(nested)) visitContainer(nested, `${prefix}node_modules/`, removed, isDryRun)
}

function visitTree(directory: string, prefix: string, removed: string[], isDryRun: boolean): void {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue
    const path = join(directory, entry.name)
    if (entry.name === 'node_modules') visitContainer(path, `${prefix}node_modules/`, removed, isDryRun)
    else visitTree(path, `${prefix}${entry.name}/`, removed, isDryRun)
  }
}

/**
 * Remove each package's outermost test, fixture, test-data and benchmark
 * directories when none of the package's runtime sources or package.json entry
 * points reference that directory by path. References from another removable
 * directory count only when that directory is itself retained. Referenced
 * directories (for example runtime modules kept under `lib/emoji/test/`) stay.
 * @param root - Directory containing one or more `node_modules` trees.
 * @param options - Set `isDryRun` to report removable directories without deleting them.
 * @returns Removed directories relative to `root`, using `/` separators.
 */
export function pruneUnreferencedNpmTests(root: string, options: { isDryRun?: boolean } = {}): string[] {
  const removed: string[] = []
  visitTree(root, '', removed, options.isDryRun === true)
  return removed.sort()
}
