/** Host-owned command execution: there is deliberately no node:child_process fallback. */
export interface RunCommandOptions {
  command: string
  args?: string[] | undefined
  cwd?: string | undefined
  timeoutMs?: number | undefined
  maxOutputBytes?: number | undefined
  graceMs?: number | undefined
  env?: Record<string, string> | undefined
  signal?: AbortSignal | undefined
  hostOnly?: boolean | undefined
}
export interface RunCommandResult {
  stdout: string
  stderr: string
  exitCode: number | null
  signal: string | null
  timedOut: boolean
  lossy: boolean
  source: 'host' | 'node'
}
export class SubprocessError extends Error {
  readonly exitCode: number | null | undefined
  readonly signal: string | null | undefined
  readonly timedOut: boolean
  constructor(message: string, options: { exitCode?: number | null; signal?: string | null; timedOut?: boolean } = {}) {
    super(message)
    this.name = 'SubprocessError'
    this.exitCode = options.exitCode
    this.signal = options.signal
    this.timedOut = options.timedOut === true
  }
}
interface Reader {
  readFrom(offset: number): { text?: string; lossy?: boolean } | undefined
}
interface HostHandle {
  done: Promise<{ exitCode?: number | null; signal?: string | null } | undefined>
  collected?: { stdout?: Reader; stderr?: Reader }
}
interface HostService {
  spawn(spec: {
    argv: string[]
    cwd: string | undefined
    stdio: {
      stdin: 'ignore'
      stdout: { maxBytes: number; spill: { maxBytes: number } }
      stderr: { maxBytes: number; spill: { maxBytes: number } }
    }
    graceMs: number
    signal: AbortSignal | undefined
    env: Record<string, string> | undefined
  }): HostHandle
}
function subprocessOf(context: unknown): HostService | undefined {
  if (context === null || typeof context !== 'object') return undefined
  const service = (context as { subprocess?: HostService | null }).subprocess
  return service && typeof service.spawn === 'function' ? service : undefined
}
export function isSubprocessMounted(context: unknown): boolean {
  return context !== null && typeof context === 'object' &&
    (context as { subprocess?: unknown }).subprocess != null
}
export function isSubprocessAvailable(context: unknown): boolean {
  return subprocessOf(context) !== undefined
}
function read(reader: Reader | undefined): { text: string; lossy: boolean } {
  const value = reader?.readFrom(0)
  return { text: value?.text ?? '', lossy: value?.lossy === true }
}
export async function runCommand(context: unknown, options: RunCommandOptions): Promise<RunCommandResult> {
  if (typeof options?.command !== 'string' || !options.command.trim()) {
    throw new SubprocessError('command is required')
  }
  const service = subprocessOf(context)
  if (!service) throw new SubprocessError('host subprocess service unavailable; refusing local execution')
  const maxOutputBytes = options.maxOutputBytes ?? 65536
  const timeoutMs = options.timeoutMs ?? 120000
  const graceMs = options.graceMs ?? 5000
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0 || !Number.isFinite(maxOutputBytes) || maxOutputBytes <= 0 ||
      !Number.isFinite(graceMs) || graceMs < 0) {
    throw new SubprocessError('invalid subprocess execution limits')
  }
  const timeoutSignal = AbortSignal.timeout(timeoutMs)
  const signal = options.signal ? AbortSignal.any([options.signal, timeoutSignal]) : timeoutSignal
  const handle = service.spawn({
    argv: [options.command, ...(options.args ?? [])],
    cwd: options.cwd,
    stdio: {
      stdin: 'ignore',
      stdout: { maxBytes: maxOutputBytes, spill: { maxBytes: maxOutputBytes } },
      stderr: { maxBytes: maxOutputBytes, spill: { maxBytes: maxOutputBytes } },
    },
    graceMs,
    signal,
    env: options.env,
  })
  const outcome = await handle.done
  const stdout = read(handle.collected?.stdout)
  const stderr = read(handle.collected?.stderr)
  return {
    stdout: stdout.text, stderr: stderr.text,
    exitCode: outcome?.exitCode ?? null,
    signal: outcome?.signal ?? null,
    timedOut: timeoutSignal.aborted,
    lossy: stdout.lossy || stderr.lossy,
    source: 'host',
  }
}
export async function runCommandOrThrow(context: unknown, options: RunCommandOptions): Promise<RunCommandResult> {
  const result = await runCommand(context, options)
  if (result.timedOut || result.exitCode !== 0) {
    throw new SubprocessError(result.timedOut ? 'host command timed out' : 'host command failed', {
      exitCode: result.exitCode, signal: result.signal, timedOut: result.timedOut,
    })
  }
  return result
}
