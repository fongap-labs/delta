import { isAbsolute, relative, resolve, sep } from 'node:path'

/** Lexical containment violation. Inherits RangeError for existing callers. */
export class SandboxViolationError extends RangeError {
  constructor(message: string) {
    super(message)
    this.name = 'SandboxViolationError'
  }
}

/** Lexical path containment only. Symlinks and races require host-side protection. */
export function isPathInside(root: string, candidate: string): boolean {
  if (typeof root !== 'string' || !root.trim() || typeof candidate !== 'string' || !candidate.trim()) return false
  const base = resolve(root)
  const target = resolve(base, candidate)
  const rel = relative(base, target)
  return rel === '' || (!isAbsolute(rel) && rel !== '..' && !rel.startsWith(`..${sep}`))
}

/** Useful for relative navigation, not an OS authorization or sandbox decision. */
export function resolveInside(root: string, candidate: string): string {
  if (!isPathInside(root, candidate)) throw new SandboxViolationError('path falls outside workspace root')
  return resolve(root, candidate)
}
