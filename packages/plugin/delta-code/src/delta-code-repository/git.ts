/** Stable repository entrypoint: Host Git execution is DSH Adapter-owned. */
export { createGitStatusTool, createGitDiffTool } from '../delta-code-adapter/providers/dsh/git.js'
export { parseGitStatus } from './git-status-policy.js'
