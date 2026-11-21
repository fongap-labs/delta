/** Safe, bounded GitHub Skill layouts. Never trusts raw Git tree paths. */
export const SKILL_SOURCE_PATH = /^(?:[a-z0-9]+(?:-[a-z0-9]+)*\/){1,3}SKILL\.md$/
const LICENSE_FILE = /^LICENSE(?:\.md|\.txt)?$/i

export function skillNameFromPath(path: unknown): string | null {
  if (typeof path !== 'string' || path.length > 240 || !SKILL_SOURCE_PATH.test(path)) return null
  return path.split('/').at(-2) ?? null
}

/**
 * Closest legal terms override repository metadata. If the nearest LICENSE
 * cannot be validated, fail closed; do not silently fall back to a root MIT.
 * Blob type, mode and SHA are checked separately by the review boundary.
 */
export interface SkillTreeFile {
  readonly path: string
  readonly type?: string
  readonly mode?: string
  readonly sha?: string
}

export function nearestSkillLicense(
  tree: readonly SkillTreeFile[], skillPath: string,
): SkillTreeFile | undefined {
  // Secondary Markdown evidence can inherit the same nearest directory license.
  const isSafeDocument = /^(?:[a-z0-9]+(?:-[a-z0-9]+)*\/){1,5}[A-Za-z0-9][A-Za-z0-9._-]*\.(?:md|txt)$/i.test(skillPath)
  if (!skillNameFromPath(skillPath) && !isSafeDocument) return undefined
  const directories = skillPath.split('/').slice(0, -1)
  // A category-level license governs descendants unless a nearer file
  // overrides it. Multiple competing LICENSE files at one level are
  // ambiguous: never silently pick the first one.
  while (true) {
    const prefix = directories.length ? directories.join('/') + '/' : ''
    const matches = tree.filter(entry =>
      typeof entry?.path === 'string' && entry.path.startsWith(prefix) &&
        LICENSE_FILE.test(entry.path.slice(prefix.length)))
    if (matches.length === 1) return matches[0]
    if (matches.length > 1) return undefined
    if (directories.length === 0) return undefined
    directories.pop()
  }
}
