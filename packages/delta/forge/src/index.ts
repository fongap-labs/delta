export { ForgeClient } from './index.mjs'
import { apply as applyForgeCommand, createForgeProviders } from './command.mjs'
import { applyAgentForgeTool } from './agent-tool.mjs'

export { name } from './command.mjs'
export const inject = ['commands', 'credentials', 'tools', 'approval']

/** One user-configured Forge provider. */
export interface ForgeProviderConfig {
  /** HTTPS origin; an explicit origin does not inherit the public token. */
  origin?: string
  /** Credential reference resolved for each request. */
  tokenRef?: string
}

/** Optional GitHub and GitLab provider settings. */
export interface ForgeConfig {
  /** Settings for GitHub requests. */
  github?: ForgeProviderConfig
  /** Settings for GitLab requests. */
  gitlab?: ForgeProviderConfig
}

/** Mount user-command and model-tool entry points over one provider configuration. */
export function apply(ctx: object, config: ForgeConfig = {}): void {
  applyForgeCommand(ctx, config)
  applyAgentForgeTool(ctx, createForgeProviders(config))
}
