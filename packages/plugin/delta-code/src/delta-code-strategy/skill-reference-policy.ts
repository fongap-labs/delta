/** Pure, bounded resolution of secondary Markdown Skill documents within one pinned folder. */
import { referencedLocalSkillFiles } from './free-skill-policy.js'

const MAX_REFERENCES = 4
const MAX_PATH = 240
const SAFE_SEGMENT = /^[A-Za-z0-9][A-Za-z0-9._-]*$/

export interface PinnedSkillReference {
  readonly path: string
  readonly source: string
}

/**
 * No URL decoding, traversal, filesystem paths, network schemes or executable
 * dependencies. A referenced file is evidence to review, never a trust grant.
 */
export function resolveSkillReferences(
  sourcePath: string, markdown: string,
): readonly PinnedSkillReference[] {
  const raw = referencedLocalSkillFiles(markdown)
  if (raw.length > MAX_REFERENCES) {
    throw Error('Skill references exceed bounded review limit')
  }
  const parent = sourcePath.split('/').slice(0, -1)
  if (!parent.length || !parent.every(part => SAFE_SEGMENT.test(part))) {
    throw Error('Invalid pinned Skill source path')
  }
  const output: PinnedSkillReference[] = []
  const seen = new Set<string>()
  for (const source of raw) {
    if (source.length === 0 || source.length > MAX_PATH ||
        /[%?#:\\\u0000-\u001f\u007f]/.test(source) &&
        !/^[^?#]+#[^?#]+$/.test(source)) {
      throw Error('Unsafe secondary Skill reference')
    }
    const withoutFragment = source.split('#')[0]!
    // A fragment is permitted only when the document path itself is safe.
    if (!withoutFragment || /[%?:\\\u0000-\u001f\u007f]/.test(withoutFragment) ||
        withoutFragment.startsWith('/') || withoutFragment.startsWith('~')) {
      throw Error('Unsafe secondary Skill reference')
    }
    const parts = withoutFragment.startsWith('./')
      ? withoutFragment.slice(2).split('/') : withoutFragment.split('/')
    if (parts.some(part => !SAFE_SEGMENT.test(part) || part === '.' || part === '..') ||
        !/\.(?:md|txt)$/i.test(parts.at(-1) ?? '') ||
        /^SKILL\.md$/i.test(parts.at(-1) ?? '')) {
      throw Error('Unsupported secondary Skill reference')
    }
    const path = [...parent, ...parts].join('/')
    if (path.length > MAX_PATH || seen.has(path.toLowerCase())) {
      throw Error('Duplicate or oversized secondary Skill reference')
    }
    seen.add(path.toLowerCase())
    output.push({ path, source })
  }
  return output
}
