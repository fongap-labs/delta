/** Existing external entrypoint; pins the default provider without exposing it to core logic. */
import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import type { Settings } from '../types.js'
import { createGitHubTools as createFeatureGitHubTools } from '../delta-code-repository/github.js'
import { createGitHubTransport } from '../delta-code-adapter/providers/github/api.js'
export { selectGitHubAccount, repositoryFromRemote } from '../delta-code-repository/github.js'

export function createGitHubTools(ctx: Context, settings: Settings): ToolDefinition[] {
  return createFeatureGitHubTools(ctx, settings, createGitHubTransport(settings.githubAccounts))
}
