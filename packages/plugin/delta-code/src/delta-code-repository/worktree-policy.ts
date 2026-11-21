/** Host-independent worktree task and branch naming policy. */
const TASK = /^[a-z0-9][a-z0-9_-]{0,63}$/
const REF = /^(?!-)(?!.*\.\.)(?!.*[~^:?*\\\[\x00-\x20])(?:[a-zA-Z0-9_][a-zA-Z0-9_.-]*\/)*[a-zA-Z0-9_][a-zA-Z0-9_.-]*$/

export function validateWorktreeName(task: string, base: string): void {
  if (!TASK.test(task)) throw new Error('invalid worktree task slug')
  if (!REF.test(base) || base.endsWith('.lock') || base === '@') throw new Error('invalid base branch or commit')
}
