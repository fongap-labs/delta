import { defineTool } from '@deepseek-ai/dsh-tools'
import type { ToolDefinition } from '@deepseek-ai/dsh-tools'

/** Always use the actual Harness tool schema implementation. */
export { defineTool }

export interface RegisterToolResult {
  registered: boolean
  disposer?: (() => void) | undefined
  error?: Error | undefined
}

/** Register with the real Cordis tool registry; never emulate registration. */
export function registerTool(context: unknown, definition: ToolDefinition): RegisterToolResult {
  const host = context as { tools?: { register?: (tool: ToolDefinition) => (() => void) | undefined } } | null | undefined
  if (typeof host?.tools?.register !== 'function') {
    return { registered: false, error: new Error('host tools.register is not available') }
  }
  const disposer = host.tools.register(definition)
  return { registered: true, disposer }
}
