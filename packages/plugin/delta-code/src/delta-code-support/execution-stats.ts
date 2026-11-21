/**
 * Ephemeral per-native-session quality counters for Delta tools, plus optional
 * name-only Host `tools/result` observations. Never collect native arguments,
 * file contents, credentials or durable telemetry.
 */
import { createHmac, randomBytes } from 'node:crypto'
import { performance } from 'node:perf_hooks'
import type { ToolDefinition, ToolRunContext } from '@deepseek-ai/dsh-tools'
import { defineTool } from '@fongap-labs/delta-plugin-shared'
import { classifyFailure, verificationEvidenceStatus, type FailureKind } from '../delta-code-quality/failure-policy.js'
import type { ExecutionObservation } from '../delta-code-quality/execution-convergence.js'

const MAX_SIGNATURES = 64
const MAX_FINGERPRINT_BYTES = 16 * 1024
const MUTATION_TOOLS = new Set(['code_multi_edit', 'code_github_commit_file', 'code_github_branch', 'code_github_create_pr', 'code_git_worktree'])

interface ToolCounters {
  name: string
  calls: number
  errors: number
  failedOutcomes: number
  duplicateCalls: number
}

interface NativeToolCounters { name: string; results: number; errors: number }

interface SessionCounters {
  lastObservation: ExecutionObservation
  nativeResults: number
  nativeErrors: number
  nativeUnconfirmed: number
  nativeTools: Map<string, NativeToolCounters>
  calls: number
  duplicateCalls: number
  repeatedFailedCalls: number
  errors: number
  failedOutcomes: number
  verificationPassed: number
  verificationFailed: number
  verificationUnconfirmed: number
  focusedPassed: number
  deliveryVerified: number
  deliveryBlocked: number
  totalDurationMs: number
  failureKinds: Record<FailureKind, number>
  tools: Map<string, ToolCounters>
  signatures: Map<string, boolean>
}

function emptyCounters(): SessionCounters {
  return {
    lastObservation: { kind: 'none' },
    nativeResults: 0, nativeErrors: 0, nativeUnconfirmed: 0,
    nativeTools: new Map(),
    calls: 0, duplicateCalls: 0, repeatedFailedCalls: 0, errors: 0,
    failedOutcomes: 0, verificationPassed: 0, verificationFailed: 0,
    verificationUnconfirmed: 0,
    focusedPassed: 0, deliveryVerified: 0, deliveryBlocked: 0,
    totalDurationMs: 0,
    failureKinds: { permission: 0, environment: 0, transient: 0,
      verification: 0, uncertain: 0, unknown: 0 },
    tools: new Map(), signatures: new Map(),
  }
}

function sessionFromAgent(agentValue: unknown): object | undefined {
  const agent = agentValue as { session?: unknown } | undefined
  const session = agent?.session
  return session !== null && typeof session === 'object' ? session : undefined
}

function sessionOf(exec: ToolRunContext): object | undefined {
  return sessionFromAgent(exec.agent)
}

function outcomeFailed(name: string, value: unknown): boolean {
  if (!value || typeof value !== 'object') return false
  const result = value as { passed?: unknown; stage?: unknown }
  if (result.passed === false) return true
  if (name === 'code_delivery_gate' && result.stage === 'blocked') return true
  if (name === 'code_resume' && result.stage === 'blocked') return true
  return false
}

export class ExecutionStats {
  private readonly fingerprintsKey = randomBytes(32)
  private sessions = new WeakMap<object, SessionCounters>()
  private readonly ownedNames = new Set<string>()

  /** A fingerprint never leaves this process; only bounded counts are exported. */
  private signature(name: string, args: unknown): string | undefined {
    try {
      const serialized = JSON.stringify(args)
      if (typeof serialized !== 'string' ||
          Buffer.byteLength(serialized) > MAX_FINGERPRINT_BYTES) return undefined
      return createHmac('sha256', this.fingerprintsKey)
        .update(name).update('\0').update(serialized).digest('hex')
    } catch {
      return undefined
    }
  }

  async observe<T>(name: string, args: unknown, exec: ToolRunContext, run: () => Promise<T> | T): Promise<T> {
    const session = sessionOf(exec)
    if (!session) return await run()
    let state = this.sessions.get(session)
    if (!state) {
      state = emptyCounters()
      this.sessions.set(session, state)
    }
    const counters = state
    let tool = counters.tools.get(name)
    if (!tool) {
      tool = { name, calls: 0, errors: 0, failedOutcomes: 0, duplicateCalls: 0 }
      counters.tools.set(name, tool)
    }
    counters.calls++
    tool.calls++
    const fingerprint = this.signature(name, args)
    if (fingerprint) {
      const isPreviouslyFailed = counters.signatures.get(fingerprint)
      if (isPreviouslyFailed !== undefined) {
        counters.duplicateCalls++
        tool.duplicateCalls++
        if (isPreviouslyFailed) counters.repeatedFailedCalls++
        counters.signatures.delete(fingerprint)
      }
    }
    const started = performance.now()
    let isFailed = false
    try {
      const value = await run()
      isFailed = outcomeFailed(name, value)
      if (isFailed) {
        counters.failedOutcomes++
        tool.failedOutcomes++
      }
      if (value && typeof value === 'object') {
        const result = value as { passed?: unknown; stage?: unknown; scope?: unknown }
        if (name === 'code_verify' || name === 'code_verify_suite') {
          const status = verificationEvidenceStatus(name, value)
          counters.lastObservation = { kind: 'verification', status }
          if (status === 'passed') {
            counters.verificationPassed++
            if (result.scope === 'focused') counters.focusedPassed++
          } else if (status === 'failed') counters.verificationFailed++
          else counters.verificationUnconfirmed++
        }
        if (name === 'code_delivery_gate') {
          counters.lastObservation = { kind: 'delivery', ready: result.stage === 'verified' && (result as { ready?: unknown }).ready === true }
          if (result.stage === 'verified' && (result as { ready?: unknown }).ready === true) {
            counters.deliveryVerified++
          } else if (result.stage === 'blocked') counters.deliveryBlocked++
        }
      }
      // Any mutation invalidates prior verification for future progress decisions.
      if (MUTATION_TOOLS.has(name)) counters.lastObservation = { kind: 'mutation' }
      return value
    } catch (error) {
      isFailed = true
      counters.errors++
      tool.errors++
      const kind = classifyFailure(error)
      counters.lastObservation = { kind: 'failure', failure: kind }
      counters.failureKinds[kind]++
      throw error
    } finally {
      counters.totalDurationMs += Math.max(0, Math.round(performance.now() - started))
      if (fingerprint) {
        counters.signatures.delete(fingerprint)
        counters.signatures.set(fingerprint, isFailed)
        if (counters.signatures.size > MAX_SIGNATURES) {
          const oldest = counters.signatures.keys().next().value
          if (oldest !== undefined) counters.signatures.delete(oldest)
        }
      }
    }
  }

  /** Scalar only, scoped to an actual native Session; never exposes tool arguments. */
  readRepeatedFailedCalls(agent: unknown): number {
    const session = sessionFromAgent(agent)
    return session ? this.sessions.get(session)?.repeatedFailedCalls ?? 0 : 0
  }

  /** Only observed execution outcomes are eligible; never reconstruct from user text. */
  readLastObservation(agent: unknown): ExecutionObservation {
    const session = sessionFromAgent(agent)
    const observation = session ? this.sessions.get(session)?.lastObservation : undefined
    return observation ? { ...observation } : { kind: 'none' }
  }

  /** Observe only a Host result event, not a guaranteed native call or retry. */
  observeNativeResult(rawExecution: unknown, rawOutcome: unknown): void {
    if (!rawExecution || typeof rawExecution !== 'object') return
    const execution = rawExecution as { agent?: unknown; name?: unknown }
    if (typeof execution.name !== 'string' || !/^[\w.:-]{1,80}$/.test(execution.name) ||
        this.ownedNames.has(execution.name)) return
    const session = sessionFromAgent(execution.agent)
    if (!session) return
    let state = this.sessions.get(session)
    if (!state) { state = emptyCounters(); this.sessions.set(session, state) }
    const outcome = rawOutcome && typeof rawOutcome === 'object'
      ? rawOutcome as { isError?: unknown } : undefined
    const isError = outcome?.isError === true
    state.nativeResults++
    if (isError) state.nativeErrors++
    if (outcome?.isError !== true && outcome?.isError !== false) state.nativeUnconfirmed++
    const name = state.nativeTools.size >= 64 && !state.nativeTools.has(execution.name)
      ? '(other)' : execution.name
    let counter = state.nativeTools.get(name)
    if (!counter) {
      counter = { name, results: 0, errors: 0 }
      state.nativeTools.set(name, counter)
    }
    counter.results++
    if (isError) counter.errors++
  }

  wrap(tool: ToolDefinition): ToolDefinition {
    this.ownedNames.add(tool.name)
    // Exclude the stats reader itself so viewing counters is never a new sample.
    if (tool.name === 'code_execution_stats') return tool
    return { ...tool, execute: (args: any, exec: ToolRunContext) =>
      this.observe(tool.name, args, exec, () => tool.execute(args, exec)) } as ToolDefinition
  }

  read(exec: ToolRunContext) {
    const session = sessionOf(exec)
    const current = session ? this.sessions.get(session) : undefined
    const state = current ?? emptyCounters()
    const scope: 'current-session' | 'unavailable' = session ? 'current-session' : 'unavailable'
    return {
      scope,
      nativeResults: state.nativeResults,
      nativeErrors: state.nativeErrors,
      nativeUnconfirmed: state.nativeUnconfirmed,
      nativeTools: [...state.nativeTools.values()].map(item => ({ ...item })),
      calls: state.calls,
      duplicateCalls: state.duplicateCalls,
      repeatedFailedCalls: state.repeatedFailedCalls,
      errors: state.errors,
      failedOutcomes: state.failedOutcomes,
      verificationPassed: state.verificationPassed,
      verificationFailed: state.verificationFailed,
      verificationUnconfirmed: state.verificationUnconfirmed,
      focusedPassed: state.focusedPassed,
      deliveryVerified: state.deliveryVerified,
      deliveryBlocked: state.deliveryBlocked,
      totalDurationMs: state.totalDurationMs,
      failureKinds: { ...state.failureKinds },
      tools: [...state.tools.values()].map(item => ({ ...item })),
    }
  }

  clear(): void {
    this.sessions = new WeakMap()
    this.ownedNames.clear()
    this.fingerprintsKey.fill(0)
  }
}

/** Read-only counters, scoped strictly to the requesting native session. */
export function createExecutionStatsTool(stats: ExecutionStats): ToolDefinition {
  return defineTool({
    name: 'code_execution_stats',
    description: 'Show ephemeral Delta tool execution counters and optional name-only native Host result events. Native result counts do not prove exact call counts or duplicate signatures. Never records arguments, tokens, paths, credentials or persistent telemetry.',
    parameters: {},
    output: {
      schema: { type: 'object', additionalProperties: false, properties: {
        scope: { type: 'string', required: true, enum: ['current-session', 'unavailable'] },
        nativeResults: { type: 'integer', required: true },
        nativeErrors: { type: 'integer', required: true },
        nativeUnconfirmed: { type: 'integer', required: true },
        nativeTools: { type: 'array', required: true, items: { type: 'object', additionalProperties: false, properties: {
          name: { type: 'string', required: true },
          results: { type: 'integer', required: true },
          errors: { type: 'integer', required: true },
        } } },
        calls: { type: 'integer', required: true },
        duplicateCalls: { type: 'integer', required: true },
        repeatedFailedCalls: { type: 'integer', required: true },
        errors: { type: 'integer', required: true },
        failedOutcomes: { type: 'integer', required: true },
        verificationPassed: { type: 'integer', required: true },
        verificationFailed: { type: 'integer', required: true },
        verificationUnconfirmed: { type: 'integer', required: true },
        focusedPassed: { type: 'integer', required: true },
        deliveryVerified: { type: 'integer', required: true },
        deliveryBlocked: { type: 'integer', required: true },
        totalDurationMs: { type: 'integer', required: true },
        failureKinds: { type: 'object', required: true, additionalProperties: false, properties:
          Object.fromEntries(['permission', 'environment', 'transient', 'verification', 'uncertain', 'unknown']
            .map(key => [key, { type: 'integer', required: true }])) },
        tools: { type: 'array', required: true, items: { type: 'object', additionalProperties: false, properties: {
          name: { type: 'string', required: true },
          calls: { type: 'integer', required: true },
          errors: { type: 'integer', required: true },
          failedOutcomes: { type: 'integer', required: true },
          duplicateCalls: { type: 'integer', required: true },
        } } },
      } },
      render(_args, value) {
        return [{ type: 'text', text: [
          'Code Worker execution stats: ' + value.scope,
          'Observed native Host results/errors/unconfirmed: ' + value.nativeResults + '/' +
            value.nativeErrors + '/' + value.nativeUnconfirmed,
          'Calls: ' + value.calls + ', repeated signatures: ' + value.duplicateCalls +
            ', repeated after failure: ' + value.repeatedFailedCalls,
          'Thrown errors: ' + value.errors + ', failed results: ' + value.failedOutcomes,
          'Verification pass/fail/unconfirmed: ' + value.verificationPassed + '/' + value.verificationFailed +
            '/' + value.verificationUnconfirmed + ' (focused passes: ' + value.focusedPassed + ')',
          'Delivery gate verified/blocked: ' + value.deliveryVerified + '/' + value.deliveryBlocked,
          'Recorded tool duration (ms): ' + value.totalDurationMs,
          'Native result observations exclude arguments and do not measure retries or duplicate signatures. Delta duplicate signatures may be intentional.',
        ].join('\n') }]
      },
    },
    async execute(_args, exec) {
      return stats.read(exec)
    },
  })
}
