import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import { defineTool } from '@fongap-labs/delta-plugin-shared'
import { DSH_HOST_ADAPTER_VERSION, lookupDshHostFs, lookupDshWorkingDirectory, probeDshService } from './host-adapter.js'

/** Inspect only method presence; do not invoke FS, subprocess or workspace capabilities. */
export function inspectHostCapabilities(ctx: Context) {
  const fsProbe = probeDshService(() => lookupDshHostFs(ctx))
  const workspaceProbe = probeDshService(() => lookupDshWorkingDirectory(ctx))
  const fs = fsProbe.value
  const workingDirectory = workspaceProbe.value
  const host = ctx as Context & { subprocess?: unknown }
  const subprocessProbe = probeDshService(() => host.subprocess)
  const toolsProbe = probeDshService(() => ctx.tools)
  const has = (value: unknown, method: string): boolean => {
    try {
      return value !== null && (typeof value === 'object' || typeof value === 'function')
        && typeof (value as Record<string, unknown>)[method] === 'function'
    } catch {
      return false // Method getters are not proof of permission or a working capability.
    }
  }
  return {
    adapterVersion: DSH_HOST_ADAPTER_VERSION,
    fsState: fsProbe.state,
    workspaceState: workspaceProbe.state,
    fsAccessible: Boolean(fs),
    fsResolve: has(fs, 'resolve'),
    fsReadText: has(fs, 'readText'),
    fsWriteText: has(fs, 'writeText'),
    subprocessState: subprocessProbe.state,
    subprocessMounted: subprocessProbe.state === 'mounted',
    subprocessSpawn: has(subprocessProbe.value, 'spawn'),
    subprocessResolveExecutable: has(subprocessProbe.value, 'resolveExecutable'),
    processOutcomeContract: 'unverified' as const,
    workspaceAccessible: Boolean(workingDirectory),
    workspaceEnsure: has(workingDirectory, 'ensure'),
    toolRegistryState: toolsProbe.state,
    toolRegistry: has(toolsProbe.value, 'register'),
    writeContract: 'unverified' as const,
  }
}

/**
 * Read-only DSH runtime diagnostics. This is not a permission grant or a
 * real FS write/CLI smoke test; actual Host calls require separate evidence.
 */
export function createHostProbeTool(ctx: Context): ToolDefinition {
  return defineTool({
    name: 'code_host_probe',
    description: 'Report available native DSH service methods without reading files, running commands, exposing credentials or testing write permissions. Use during Host compatibility diagnosis.',
    parameters: {},
    output: {
      schema: {
        type: 'object',
        additionalProperties: false,
        properties: {
          adapterVersion: { type: 'string', required: true, enum: ['dsh-cordis-v1'] },
          fsState: { type: 'string', required: true, enum: ['absent', 'mounted', 'inaccessible'] },
          workspaceState: { type: 'string', required: true, enum: ['absent', 'mounted', 'inaccessible'] },
          fsAccessible: { type: 'boolean', required: true },
          fsResolve: { type: 'boolean', required: true },
          fsReadText: { type: 'boolean', required: true },
          fsWriteText: { type: 'boolean', required: true },
          subprocessState: { type: 'string', required: true, enum: ['absent', 'mounted', 'inaccessible'] },
          subprocessMounted: { type: 'boolean', required: true },
          subprocessSpawn: { type: 'boolean', required: true },
          subprocessResolveExecutable: { type: 'boolean', required: true },
          processOutcomeContract: { type: 'string', required: true, enum: ['unverified'] },
          workspaceAccessible: { type: 'boolean', required: true },
          workspaceEnsure: { type: 'boolean', required: true },
          toolRegistryState: { type: 'string', required: true, enum: ['absent', 'mounted', 'inaccessible'] },
          toolRegistry: { type: 'boolean', required: true },
          writeContract: { type: 'string', required: true, enum: ['unverified'] },
        },
      },
      render(_args, value) {
        const flags = [
          ['FS service', value.fsAccessible],
          ['FS resolve', value.fsResolve],
          ['FS readText', value.fsReadText],
          ['FS writeText', value.fsWriteText],
          ['Subprocess mounted', value.subprocessMounted],
          ['Subprocess spawn', value.subprocessSpawn],
          ['Subprocess resolveExecutable', value.subprocessResolveExecutable],
          ['Workspace service', value.workspaceAccessible],
          ['Workspace ensure', value.workspaceEnsure],
          ['Tool registry', value.toolRegistry],
        ]
        return [{ type: 'text', text: [
          'DSH Host method presence (read-only; no permission tested):',
          'Adapter: ' + value.adapterVersion,
          'FS service state: ' + value.fsState,
          'Workspace service state: ' + value.workspaceState,
          'Subprocess service state: ' + value.subprocessState,
          'Tool registry state: ' + value.toolRegistryState,
          ...flags.map(([name, available]) => name + ': ' + (available ? 'available' : 'unavailable')),
          'FS writeText contract: unverified (requires actual Host smoke test)',
          'Subprocess exit/permission contract: unverified (method presence is not execution evidence)',
        ].join('\n') }]
      },
    },
    async execute(_args, exec) {
      if (exec.signal.aborted) throw new Error('host probe cancelled')
      return inspectHostCapabilities(ctx)
    },
  })
}
