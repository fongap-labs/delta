/** Host-independent workspace discovery bounds, Git index ranking and batch query rules. */
export const DEFAULT_LIMIT = 100
export const MAX_LIMIT = 250
// Discover direct child checkouts only. Never recurse through the full workspace.
const MAX_DISCOVERY_DIRECTORIES = 96
const MAX_DISCOVERY_REPOSITORIES = 12

/** The same bounded, deterministic candidate ordering in Node and Host mode. */
export function boundedDirectories<T extends { name: string }>(entries: T[]): T[] {
  const sorted = entries.sort((a, b) => a.name.localeCompare(b.name))
  if (sorted.length > MAX_DISCOVERY_DIRECTORIES) {
    throw new Error('workspace has too many subdirectories to scan safely; specify cwd')
  }
  return sorted
}

export function assertRepositoryCapacity(count: number): void {
  if (count >= MAX_DISCOVERY_REPOSITORIES) {
    throw new Error('workspace has too many Git repositories; specify cwd')
  }
}

export function prefixTrackedPaths(paths: string[], prefix: string): string[] {
  return prefix ? paths.map(path => prefix + '/' + path) : paths
}
const SOURCE_EXTENSIONS = new Set(['ts', 'tsx', 'js', 'jsx', 'mjs', 'cjs', 'py', 'rs', 'go', 'java', 'kt', 'cs', 'c', 'cc', 'cpp', 'h', 'hpp', 'vue', 'svelte', 'swift'])
const MAX_BATCH_QUERIES = 8

interface TrackedFileEntry {
  path: string
  lower: string
  file: string
  baseScore: number
}

/** Normalize tracked paths once for every query in the same authorized index read. */
function indexTrackedFiles(paths: readonly string[]): TrackedFileEntry[] {
  return paths.map(path => {
    const lower = path.toLowerCase()
    const file = lower.split('/').at(-1) ?? lower
    const extension = file.split('.').at(-1) ?? ''
    const baseScore = (SOURCE_EXTENSIONS.has(extension) ? 1 : 0) +
      (file === 'package.json' || file === 'cargo.toml' || file === 'pyproject.toml' ||
       file === 'go.mod' || file === 'readme.md' || file === 'agents.md' ? 2 : 0)
    return { path, lower, file, baseScore }
  })
}

function rankIndexedFiles(index: readonly TrackedFileEntry[], query: string, limit: number) {
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean)
  const ranked = index.map(entry => {
    let score = entry.baseScore
    let matches = 0
    for (const term of terms) {
      let weight = 0
      if (entry.file === term) weight = 12
      else if (entry.file.includes(term)) weight = 8
      else if (entry.lower.includes(term)) weight = 4
      // One unmatched language/alias must not cancel a genuine match.
      score += weight
      if (weight > 0) matches++
    }
    return { path: entry.path, score, matches }
  }).filter(entry => terms.length === 0 || entry.matches > 0)
    .sort((a, b) => b.score - a.score || b.matches - a.matches || a.path.localeCompare(b.path))
  return { selected: ranked.slice(0, limit).map(entry => entry.path), matches: ranked.length }
}

/** Preserve the original one-query ranking API and tie-break order. */
export function rankTrackedFiles(paths: string[], query: string, limit: number): string[] {
  return rankIndexedFiles(indexTrackedFiles(paths), query, limit).selected
}

/** Reject invalid batches before calling Host FS or spawning Git. */
export function parseBatchQueries(value: unknown, singleQuery: unknown): string[] | undefined {
  if (value === undefined) return undefined
  if (singleQuery !== undefined) throw new Error('query and queries are mutually exclusive')
  if (!Array.isArray(value) || value.length < 1 || value.length > MAX_BATCH_QUERIES) {
    throw new Error('queries must contain 1..8 distinct terms')
  }
  const seen = new Set<string>()
  return value.map(item => {
    if (typeof item !== 'string' || !item.trim() || item.length > 160 || item.includes('\\') ||
        /[\u0000-\u001f\u007f]/.test(item)) {
      throw new Error('queries must contain safe nonempty strings of at most 160 characters')
    }
    const query = item.trim()
    const normalized = query.toLowerCase()
    if (seen.has(normalized)) throw new Error('queries must not contain duplicates')
    seen.add(normalized)
    return query
  })
}

export function workspaceMapResult(
  paths: string[], repositories: string[], query: string, limit: number, queries?: readonly string[],
) {
  const indexed = indexTrackedFiles(paths)
  if (queries) {
    // Never return an unbounded multiplied list when several queries are batched.
    const perQueryLimit = Math.min(limit, Math.floor(MAX_LIMIT / queries.length))
    const queryResults = queries.map(value => {
      const ranked = rankIndexedFiles(indexed, value, perQueryLimit)
      return { query: value, selected: ranked.selected, truncated: ranked.matches > ranked.selected.length }
    })
    return { total: paths.length, selected: [], truncated: queryResults.some(item => item.truncated),
      repositories, queryResults }
  }
  const selected = rankIndexedFiles(indexed, query, limit).selected
  return { total: paths.length, selected, truncated: paths.length > selected.length,
    repositories, queryResults: [] as Array<{ query: string; selected: string[]; truncated: boolean }> }
}
