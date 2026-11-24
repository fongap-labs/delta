export { ForgeClient } from './index.mjs'
import { apply as applyForgeCommand, createForgeProviders } from './command.mjs'
import { applyAgentForgeTool } from './agent-tool.mjs'

export { name } from './command.mjs'
export const inject = ['commands', 'credentials', 'tools', 'approval']

/** Mount user-command and model-tool entry points over one provider configuration. */
export function apply(ctx: object, config: Record<string, unknown> = {}): void {
  applyForgeCommand(ctx, config)
  applyAgentForgeTool(ctx, createForgeProviders(config))
}
