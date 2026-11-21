import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import { defineTool, runCommand, isSubprocessMounted } from '@fongap-labs/delta-plugin-shared'
import type { LspProbeResult, Settings } from '../../../types.js'
import { TOOL_PREFIX, resolveRoot, errorMessage } from '../../../runtime.js'
import { resolveAuthorizedGitCwd } from '../../../delta-code-repository/host-git-cwd.js'
import { lookupDshHostFs } from './host-adapter.js'
import { classifyLspCommandResult } from '../../../delta-code-coding/lsp-probe-policy.js'

export function createLspProbeTool(ctx: Context, settings: Settings): ToolDefinition {
  return defineTool({
    name: `${TOOL_PREFIX}lsp_probe`,
    description:
      'Probe the workspace for available language servers (by checking command availability via `--help`/`--version`). Returns which configured LSP servers are installed and runnable.',
    parameters: {
      cwd: { type: 'string', description: 'Optional subdirectory inside the workspace to probe from.' },
    },
    output: {
      schema: {
        type: 'object',
        additionalProperties: false,
        properties: {
          servers: {
            type: 'array',
            required: true,
            description: 'Probe results per configured server.',
            items: {
              type: 'object',
              additionalProperties: false,
              properties: {
                language: { type: 'string', required: true },
                command: { type: 'string', required: true },
                available: { type: 'boolean', required: true },
                note: { type: 'string' },
              },
            },
          },
        },
      },
      render(_args, value) {
        const lines = ['LSP probe:']
        for (const server of value.servers) {
          const flag = server.available ? 'ok' : 'no'
          lines.push(
            `  [${flag}] ${server.language}: ${server.command}${server.note ? ` — ${server.note}` : ''}`,
          )
        }
        return [{ type: 'text', text: lines.join('\n') }]
      },
    },
    async execute(args, exec) {
      if (exec.signal.aborted) throw new Error('lsp probe cancelled')
      const root = await resolveRoot(ctx, exec, settings)
      // Host FS is authoritative for virtual/remote workspaces and symlink
      // containment. Never inspect a mounted Host workspace with Node realpath.
      let canonicalCwd: string
      try {
        canonicalCwd = await resolveAuthorizedGitCwd(ctx, root, args.cwd, exec)
      } catch (error) {
        // Retain the public LSP workspace error contract while surfacing
        // the reason from the shared Host FS containment check.
        if (error instanceof Error && /^Git cwd escapes (?:Host )?workspace/.test(error.message)) {
          throw new Error('lsp cwd escapes workspace: ' + error.message)
        }
        throw error
      }
      const isHostFsMounted = Boolean(lookupDshHostFs(ctx))
      const servers: LspProbeResult[] = []
      for (const server of settings.lspServers) {
        if (exec.signal.aborted) throw new Error('lsp probe cancelled')
        let isAvailable = false
        let note = ''
        try {
          const result = await runCommand(ctx, {
            command: server.command,
            args: ['--version'],
            cwd: canonicalCwd,
            signal: exec.signal,
            timeoutMs: 8_000,
            maxOutputBytes: 4 * 1024,
            hostOnly: isHostFsMounted || isSubprocessMounted(ctx),
          })
          if (exec.signal.aborted) throw new Error('lsp probe cancelled')
          const outcome = classifyLspCommandResult(result)
          isAvailable = outcome.available
          note = outcome.note
        } catch (error) {
          // Cancellation is a control signal, not a failed language-server probe.
          if (exec.signal.aborted) throw new Error('lsp probe cancelled')
          isAvailable = false
          note = errorMessage(error).split('\n')[0]!.slice(0, 240)
        }
        servers.push({
          language: server.language,
          command: server.command,
          available: isAvailable,
          note,
        })
      }
      return { servers }
    },
  })
}
