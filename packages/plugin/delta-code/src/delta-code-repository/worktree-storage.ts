/** Delta Code stores and manages worktrees under one official namespace. */
export const WORKTREE_STORAGE_DIR = '.delta-code'

/** Hide only the active Delta Code managed worktree paths from file listings. */
export function isInternalWorktreePath(path: string): boolean {
  return path.startsWith(WORKTREE_STORAGE_DIR + '/')
}
