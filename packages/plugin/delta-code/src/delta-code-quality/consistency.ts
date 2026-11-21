/**
 * Explicit directory language policy and Git added-file evidence.
 * No language inference from historical file counts: migrations and mixed
 * language directories remain possible unless the repository declares a rule.
 */
export type LanguageFamily = 'typescript' | 'javascript'
export interface LanguageConventionRule {
  /** Repository-relative POSIX directory; '.' applies to the whole checkout. */
  directory: string
  language: LanguageFamily
}
export interface LanguageViolation {
  path: string
  directory: string
  expected: LanguageFamily
  actual: LanguageFamily
}

function validPath(value: string): boolean {
  return value !== '' && !value.startsWith('/') && !value.endsWith('/') &&
    !value.includes('\\') && !/[\x00-\x1f\x7f]/.test(value) &&
    !/^[A-Za-z]:/.test(value) &&
    value.split('/').every(segment => segment !== '' && segment !== '.' && segment !== '..')
}

/** Fail on invalid policy instead of silently disabling a requested rule. */
export function normalizeLanguageConventions(raw: unknown): LanguageConventionRule[] {
  if (raw === undefined) return []
  if (!Array.isArray(raw) || raw.length > 64) {
    throw new Error('execution.languageConventions must be an array of at most 64 rules')
  }
  const rules: LanguageConventionRule[] = []
  const seen = new Set<string>()
  for (const item of raw) {
    if (!item || typeof item !== 'object' || Array.isArray(item)) {
      throw new Error('language convention rule must be an object')
    }
    const entry = item as Record<string, unknown>
    const directory = entry.directory
    const language = entry.language
    if (typeof directory !== 'string' || (directory !== '.' && !validPath(directory)) ||
        directory.length > 512) {
      throw new Error('language convention directory must be a safe repository-relative directory')
    }
    if (language !== 'typescript' && language !== 'javascript') {
      throw new Error('language convention language must be typescript or javascript')
    }
    if (seen.has(directory)) throw new Error('duplicate language convention directory: ' + directory)
    seen.add(directory)
    rules.push({ directory, language })
  }
  return rules
}

/** Parse the exact NUL record structure returned by Git -z commands. */
export function parseGitFileList(raw: string): string[] {
  if (raw === '') return []
  if (!raw.endsWith('\0')) throw new Error('incomplete Git NUL-terminated file list')
  const names = raw.slice(0, -1).split('\0')
  if (names.some(name => !validPath(name))) throw new Error('invalid Git file path')
  return names
}

/**
 * HEAD is the baseline. The current index includes staged additions,
 * while --others includes untracked files. Deletions are absent from the
 * current list and therefore cannot be misclassified as additions.
 */
export function identifyAddedFiles(headRaw: string, workingRaw: string): string[] {
  const head = new Set(parseGitFileList(headRaw))
  const current = parseGitFileList(workingRaw)
  return [...new Set(current.filter(name => !head.has(name)))].sort()
}

function languageOf(path: string): LanguageFamily | undefined {
  // Declaration files are valid in both JS and TS projects.
  if (/\.d\.(ts|mts|cts)$/i.test(path)) return undefined
  if (/\.(ts|tsx|mts|cts)$/i.test(path)) return 'typescript'
  if (/\.(js|jsx|mjs|cjs)$/i.test(path)) return 'javascript'
  return undefined
}

function ruleFor(path: string, rules: readonly LanguageConventionRule[]): LanguageConventionRule | undefined {
  let found: LanguageConventionRule | undefined
  for (const rule of rules) {
    if ((rule.directory === '.' || path.startsWith(rule.directory + '/')) &&
        (!found || rule.directory.length > found.directory.length)) {
      found = rule
    }
  }
  return found
}

/** Compare only NEW code paths against explicitly declared directory rules. */
export function checkLanguageConventions(
  added: readonly string[], rules: readonly LanguageConventionRule[],
): LanguageViolation[] {
  const violations: LanguageViolation[] = []
  for (const path of [...new Set(added)].sort()) {
    const actual = languageOf(path)
    if (!actual) continue
    const rule = ruleFor(path, rules)
    if (rule && rule.language !== actual) {
      violations.push({ path, directory: rule.directory, expected: rule.language, actual })
    }
  }
  return violations
}

export function describeLanguageViolations(violations: readonly LanguageViolation[]): string[] {
  return violations.map(item =>
    'new code file ' + item.path + ' uses ' + item.actual +
    '; explicit directory policy ' + item.directory + ' requires ' + item.expected +
    ' (change the file language or update the declared rule)',
  )
}
