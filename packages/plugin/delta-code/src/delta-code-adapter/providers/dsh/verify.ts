import { access, realpath } from 'node:fs/promises'
import { join } from 'node:path'
import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition, ToolRunContext } from '@deepseek-ai/dsh-tools'
import { defineTool, isSubprocessAvailable, runCommand, resolveInside, isPathInside } from '@fongap-labs/delta-plugin-shared'
import type { Settings } from '../../../types.js'
import { readTextSafe, resolveRoot } from '../../../runtime.js'
import { chooseVerificationCommand, type VerificationProject, type VerificationCommand, type VerificationScript, isAllowedVerificationScript } from '../../../delta-code-quality/verify-presets.js'
import { verificationMetadataFiles, detectVerificationProjects, parseDeclaredNodeVerificationScripts, selectVerificationPackageManager } from '../../../delta-code-quality/verification-project-policy.js'
import { resolveAuthorizedGitCwd } from './host-git-cwd.js'
import { classifyVerificationEvidence, verificationNextAction } from '../../../delta-code-quality/failure-policy.js'
import { assessProcessOutcome } from '../../contracts/process-outcome.js'
import { requireHostFs } from './host-fs.js'

const PROJECTS = new Set(['auto', 'node', 'rust', 'python', 'go'])
const MAX_OUTPUT = 32 * 1024

export function redactKnownTokens(text: string, settings: Settings): string {
  let value = text
  for (const account of settings.githubAccounts) {
    const token = account.tokenEnv ? process.env[account.tokenEnv] : undefined
    if (token && token.length >= 8) value = value.split(token).join('[REDACTED]')
  }
  return value.replace(/\b(?:github_pat_[a-zA-Z0-9_]{12,}|gh[pousr]_[a-zA-Z0-9]{20,})\b/g, '[REDACTED]')
}

export function createVerifyTool(ctx: Context, settings: Settings): ToolDefinition {
  return defineTool({
    name: 'code_verify',
    description: 'Run an allowlisted test/typecheck/build/lint/check for Node, Rust, Python or Go in the Host subprocess. Disabled unless execution.allowCommands=true; returns bounded evidence.',
    parameters: {
      script: { type: 'string', required: true, description: 'Allowlisted script or declared Node variant (test:unit).' },
      cwd: { type: 'string', description: 'Optional relative workspace subdirectory.' },
      project: { type: 'string', enum: ['auto', 'node', 'rust', 'python', 'go'], description: 'Project type; auto requires exactly one manifest type.' },
      timeoutMs: { type: 'integer', description: 'Host subprocess timeout in ms, 10000..600000; default 120000.' },
    },
    output: { schema: { type: 'object', additionalProperties: false, properties: {
      script: { type: 'string', required: true },
      project: { type: 'string', required: true },
      command: { type: 'string', required: true },
      exitCode: { type: 'integer', required: true },
      signal: { type: 'string', required: true },
      outcome: { type: 'string', required: true, enum: ['succeeded', 'failed', 'uncertain'] },
      passed: { type: 'boolean', required: true },
      timedOut: { type: 'boolean', required: true },
      output: { type: 'string', required: true },
      truncated: { type: 'boolean', required: true },
      failureKind: { type: 'string', required: true },
      nextAction: { type: 'string', required: true },
    } }, render(_args, value) {
      return [{ type: 'text', text: value.command + ': ' + (value.passed ? 'PASS' : 'FAIL') + ' (exit ' + value.exitCode + ', outcome ' + value.outcome + (value.signal ? ', signal ' + value.signal : '') + ')\n' + value.output + '\nNext: ' + value.nextAction }]
    } },
    async execute(args, exec) {
      if (!isAllowedVerificationScript(args.script)) throw new Error('script is not allowlisted')
      const prepared = await prepareVerify(ctx, settings, exec, { ...args, script: args.script })
      const evidence = await runPreparedVerify(ctx, settings, exec, prepared)
      const failureKind = classifyVerificationEvidence(evidence)
      return { ...evidence, failureKind, nextAction: verificationNextAction(failureKind) }
    },
  })
}

interface VerificationProjectProbe {
  /** Scoped to one preflight, never persisted between tool calls. */
  root: string
  cwd: string
  hostFs: unknown
  files: ReadonlySet<string>
  nodeScripts: readonly string[]
}

export interface PreparedVerification {
  script: VerificationScript
  plan: VerificationCommand
  cwd: string
  timeoutMs: number
  /** Internal snapshot shared only by scripts of the same verification suite. */
  probe: VerificationProjectProbe
}

/** Prepare and validate a check without executing a command. */

/** All Host-mounted metadata reads use DSH Host FS; no local filesystem fallback. */
export async function prepareVerify(
  ctx: Context, settings: Settings, exec: ToolRunContext,
  args: { script: VerificationScript; cwd?: string; project?: VerificationProject; timeoutMs?: number },
  reuseProbe?: VerificationProjectProbe,
): Promise<PreparedVerification> {
  const checkCancelled = () => {
    if (exec.signal.aborted) throw new Error('verification cancelled')
  }
  checkCancelled()
  if (!settings.allowCommands) throw new Error('script execution disabled; set execution.allowCommands=true')
  if (!isSubprocessAvailable(ctx)) throw new Error('host subprocess service is required for script execution')
  if (!isAllowedVerificationScript(args.script)) throw new Error('script is not allowlisted')
  const timeoutMs = args.timeoutMs ?? 120_000
  if (!Number.isInteger(timeoutMs) || timeoutMs < 10_000 || timeoutMs > 600_000) {
    throw new Error('timeoutMs must be between 10000 and 600000')
  }
  const project = args.project ?? 'auto'
  if (typeof project !== 'string' || !PROJECTS.has(project)) throw new Error('project type is not allowlisted')
  const root = await resolveRoot(ctx, exec, settings)
  const cwd = args.cwd ? resolveInside(root, args.cwd) : root
  let canonicalCwd = cwd
  let hostPackage: unknown
  const hostFs = requireHostFs(ctx, ['resolve', 'contains', 'stat', 'readText'], 'refusing local verification fallback')
  // Reuse only immutable project metadata within the same suite. A changed
  // mount/root/cwd is never a cache hit and cannot authorize a different Host.
  if (reuseProbe && (
    reuseProbe.root !== root || reuseProbe.cwd !== cwd || reuseProbe.hostFs !== hostFs
  )) {
    throw new Error('Host verification project changed during preflight')
  }
  const files = reuseProbe ? new Set(reuseProbe.files) : new Set<string>()
  const fileNames = verificationMetadataFiles
  if (hostFs) {
    checkCancelled()
    const canonicalRoot = await hostFs.resolve(root, { cwd: root, signal: exec.signal })
    const canonicalDirectory = await hostFs.resolve(cwd, { cwd: root, signal: exec.signal })
    if (!hostFs.contains(canonicalRoot, canonicalDirectory)) {
      throw new Error('script directory escapes the workspace')
    }
    if (!reuseProbe) {
      for (const name of fileNames) {
        checkCancelled()
        const target = await hostFs.resolve(join(cwd, name), { cwd, signal: exec.signal })
        if (!hostFs.contains(canonicalRoot, target)) {
          throw new Error(name + ' escapes the workspace')
        }
        const metadata = await hostFs.stat(target, exec.signal)
        if (metadata?.type === 'file') {
          files.add(name)
          if (name === 'package.json') hostPackage = target
        }
      }
    }
  } else {
    const canonicalRoot = await realpath(root)
    canonicalCwd = await realpath(cwd)
    if (!isPathInside(canonicalRoot, canonicalCwd)) {
      throw new Error('script directory escapes the workspace')
    }
    if (!reuseProbe) {
      for (const name of fileNames) {
        checkCancelled()
        try {
          const file = await realpath(join(canonicalCwd, name))
          if (!isPathInside(canonicalRoot, file)) {
            throw new Error(name + ' escapes the workspace')
          }
          await access(file)
          files.add(name)
        } catch (error) {
          if (error instanceof Error && error.message.includes('escapes the workspace')) throw error
        }
      }
    }
  }
  const found = detectVerificationProjects(files)
  const nodeScripts: string[] = reuseProbe ? [...reuseProbe.nodeScripts] : []
  if (found.includes('node') && !reuseProbe) {
    checkCancelled()
    const raw = hostFs
      ? await hostFs.readText(hostPackage, exec.signal)
      : await readTextSafe(ctx, exec, canonicalCwd, 'package.json')
    if (Buffer.byteLength(raw, 'utf8') > 1024 * 1024) throw new Error('package.json is too large')
    nodeScripts.push(...parseDeclaredNodeVerificationScripts(JSON.parse(raw)))
  }
  const manager = selectVerificationPackageManager(files)
  const plan = chooseVerificationCommand(
    args.script, project, [...found], manager, nodeScripts, files.has('Cargo.lock'),
  )
  checkCancelled()
  // A Host FS path need not exist in the plugin's own filesystem.
  // Reuse the same Host FS authorization and processPath contract as Git:
  // never spawn in the raw workspace spelling after Host FS was mounted.
  const executionCwd = hostFs
    ? await resolveAuthorizedGitCwd(ctx, root, args.cwd, exec)
    : canonicalCwd
  checkCancelled()
  return {
    script: args.script, plan, cwd: executionCwd, timeoutMs,
    probe: reuseProbe ?? { root, cwd, hostFs, files, nodeScripts },
  }
}

/**
 * Bound Host verification even if the Host never settles its done promise.
 * A deadline requests cancellation, but does NOT prove the command stopped.
 * No local fallback or automatic retry of an unconfirmed execution is allowed.
 */
export async function runPreparedVerify(
  ctx: Context, settings: Settings, exec: ToolRunContext, prepared: PreparedVerification,
) {
  if (exec.signal.aborted) throw new Error('verification cancelled')
  const { plan, cwd, timeoutMs } = prepared
  const stop = new AbortController()
  let timer: ReturnType<typeof setTimeout> | undefined
  let onCancel: (() => void) | undefined
  try {
    const result = await Promise.race([
      runCommand(ctx, {
        command: plan.command, args: plan.args, cwd, signal: stop.signal,
        timeoutMs, maxOutputBytes: MAX_OUTPUT, hostOnly: true,
      }),
      new Promise<never>((_resolve, reject) => {
        timer = setTimeout(() => {
          reject(new Error('Host verification deadline exceeded; execution outcome unconfirmed'))
          stop.abort()
        }, timeoutMs)
      }),
      new Promise<never>((_resolve, reject) => {
        onCancel = () => {
          reject(new Error('verification cancelled'))
          stop.abort()
        }
        exec.signal.addEventListener('abort', onCancel, { once: true })
        if (exec.signal.aborted) onCancel()
      }),
    ])
    const assessment = assessProcessOutcome(result)
    const exitCode = result.exitCode ?? -1
    const combined = result.stdout + '\n' + result.stderr
    const truncated = result.lossy || combined.length > MAX_OUTPUT
    return {
      script: prepared.script,
      project: plan.project,
      command: [plan.command, ...plan.args].join(' '),
      exitCode,
      signal: result.signal ?? '',
      outcome: assessment.status,
      passed: assessment.status === 'succeeded' && assessment.complete && !truncated,
      timedOut: result.timedOut,
      output: redactKnownTokens(combined.slice(0, MAX_OUTPUT), settings),
      truncated,
    }
  } finally {
    if (timer !== undefined) clearTimeout(timer)
    if (onCancel) exec.signal.removeEventListener('abort', onCancel)
  }
}
