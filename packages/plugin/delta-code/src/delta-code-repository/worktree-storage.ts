/** New Delta Code worktrees live under this single official namespace. */
export const WORKTREE_STORAGE_DIR = '.delta-code'

/** Read/removal compatibility only. Never create new worktrees in the old namespace. */
export const LEGACY_WORKTREE_STORAGE_DIR = '.dsh-delta-code'

/** Keep both current and pre-migration private worktree content out of file listings. */
export function isInternalWorktreePath(path: string): boolean {
  return path.startsWith(WORKTREE_STORAGE_DIR + '/') ||
    path.startsWith(LEGACY_WORKTREE_STORAGE_DIR + '/')
}
