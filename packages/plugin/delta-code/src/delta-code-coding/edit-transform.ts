/**
 * Validation and text transformation shared by preflight and legacy multi-edit.
 * These pure functions do not authorize filesystem access or perform writes.
 * Batch preflight, Host FS capability checks, readback, and recovery stay with
 * the caller, which preserves the two execution modes' distinct guarantees.
 */
export interface TextEditInput {
  content?: unknown
  find?: unknown
  replace?: unknown
  expectedSha256?: unknown
}

/** Reject high-confidence binary data in a UTF-8 source editing operation. */
export function assertEditableText(value: string): void {
  if (value.includes('\0')) throw new Error('binary-like file contains NUL; text edit refused')
}

export type PlannedTextEdit =
  | { action: 'overwrite'; content: string }
  | { action: 'replace'; find: string; replace: string }

/**
 * Preflight requires exactly one operation; the legacy mode retains its
 * distinct skipped result for an edit with neither operation specified.
 */
export function planTextEdit(edit: TextEditInput, isPreflight: boolean): PlannedTextEdit | undefined {
  const hasContent = edit.content !== undefined
  const hasFind = edit.find !== undefined
  if (isPreflight ? hasContent === hasFind : hasContent && hasFind) {
    throw new Error('provide exactly one of content or find/replace')
  }
  if (!isPreflight && edit.expectedSha256 !== undefined) {
    throw new Error('expectedSha256 requires preflight=true')
  }
  if (!hasContent && !hasFind) return undefined
  if (hasFind) {
    if (isPreflight && edit.expectedSha256 !== undefined) {
      throw new Error('expectedSha256 is only supported for content overwrite')
    }
    if (typeof edit.find !== 'string' || edit.find.length === 0 || typeof edit.replace !== 'string') {
      throw new Error('find must be non-empty and replace must be a string')
    }
    assertEditableText(edit.find)
    assertEditableText(edit.replace)
    return { action: 'replace', find: edit.find, replace: edit.replace }
  }
  if (typeof edit.content !== 'string') throw new Error('content must be a string')
  assertEditableText(edit.content)
  return { action: 'overwrite', content: edit.content }
}

/** In guarded preflight reject ambiguous matches; legacy mode still replaces the first one. */
export function applyTextEdit(original: string, edit: Extract<PlannedTextEdit, { action: 'replace' }>, shouldRequireUnique = false): string {
  assertEditableText(original)
  // Match newline spellings without normalizing the entire file, which would
  // change unrelated bytes and invalidate guarded snapshots. Keep exact
  // literal semantics for searches that do not contain a newline.
  const find = edit.find.replace(/\r\n/g, '\n')
  if (!find.includes('\n')) {
    const index = original.indexOf(edit.find)
    if (index === -1) throw new Error('find substring not present')
    if (shouldRequireUnique && original.indexOf(edit.find, index + 1) !== -1) {
      throw new Error('find substring matches multiple locations; supply a more specific target')
    }
    return original.slice(0, index) + edit.replace + original.slice(index + edit.find.length)
  }
  const escaped = find.split('\n').map(part => part.replace(/[.*+?^$(){}|[\]\\]/g, '\\$&'))
  const pattern = new RegExp(escaped.join('(?:\\r\\n|(?<!\\r)\\n)'), 'g')
  const match = pattern.exec(original)
  if (!match) throw new Error('find substring not present')
  // Check overlapping matches too, just as the literal search did.
  pattern.lastIndex = match.index + 1
  if (shouldRequireUnique && pattern.exec(original)) {
    throw new Error('find substring matches multiple locations; supply a more specific target')
  }
  const matched = match[0]
  const hasCrLf = matched.includes('\r\n')
  const hasLf = /(?<!\r)\n/.test(matched)
  let replacement = edit.replace
  if (hasCrLf && hasLf) {
    // An ambiguous mixed-ending span must not be silently converted.
    if (matched !== edit.find) throw new Error('mixed line endings in target; supply exact find text')
  } else {
    replacement = replacement.replace(/\r?\n/g, hasCrLf ? '\r\n' : '\n')
  }
  return original.slice(0, match.index) + replacement + original.slice(match.index + matched.length)
}
