/**
 * Coordinate Gateway exhaustion with DSH's native request-error retry policy.
 *
 * DSH already owns retry counters, session events and cancelled waits. This
 * hook must NEVER create a second retry loop: it runs first in the native
 * request-error waterfall, prevents a blind replay after a long Gateway
 * cooldown, or enforces the missing portion of a short Retry-After after
 * DSH's native retry decision has settled.
 */
import type { Context } from '@deepseek-ai/cordis'
import { isAccountQuotaExhausted } from '../../../delta-code-quality/failure-policy.js'

type RecoveryAction = { kind: string } | undefined
type RecoveryEvent = {
  failure?: { message?: string; providerRetryAfterMs?: number }
  retryPolicy?: { mode?: string; maxDelayMs?: number }
  signal?: AbortSignal
}
type RecoveryListener = (event: RecoveryEvent, next: () => Promise<RecoveryAction>) => Promise<RecoveryAction>
type RuntimeEventContext = {
  on?: (name: string, callback: RecoveryListener, prepend?: boolean) => (() => void) | void
  logger?: { info?: (message: string) => void }
}

const MAX_ERROR_MESSAGE_LENGTH = 8192
const DEFAULT_MAX_AUTO_RETRY_WAIT_MS = 10_000
const GATEWAY_EXHAUSTED_CODES = new Set([
  'gateway_attempt_budget_exhausted',
  'gateway_failover_budget_exhausted',
  'gateway_no_dispatchable_node',
])

type GatewayRetryHint = { retryAfterMs: number; suppressAutomaticReplay: boolean }

/** Only accept a Gateway-owned error contract, never an arbitrary phrase. */
function gatewayRetryHint(message: unknown): GatewayRetryHint | null {
  if (typeof message !== 'string' || message.length > MAX_ERROR_MESSAGE_LENGTH) return null
  const match = /^(?:HTTP\s+)?(?:429|503|504):\s*(\{[\s\S]+\})\s*$/.exec(message.trim())
  if (!match) return null
  try {
    const body = JSON.parse(match[1]!) as Record<string, unknown>
    const details = (body.details ?? (body.error && typeof body.error === 'object'
      ? (body.error as Record<string, unknown>).details : undefined)) as Record<string, unknown> | undefined
    if (!details || !GATEWAY_EXHAUSTED_CODES.has(String(details.gateway_code ?? ''))) return null
    if (details.retryable === false) return null
    const value = details.retry_after_ms
    if (typeof value !== 'number' || !Number.isSafeInteger(value) || value <= 0 || value > 86_400_000) return null
    // Gateway #402 marks mixed timeout/rate-limit exhaustion with a long
    // cooldown as x-should-retry:false. The DSH error event may lose that
    // header, but preserves the Gateway-owned structured failure kinds.
    // Pure 429 exhaustion deliberately retains the native Retry-After path.
    const kinds = details.failure_kinds
    const hadHeadersTimeout = kinds !== null && typeof kinds === 'object' && !Array.isArray(kinds) &&
      Number((kinds as Record<string, unknown>).headers_timeout) > 0
    return {
      retryAfterMs: value,
      suppressAutomaticReplay: details.gateway_code === 'gateway_attempt_budget_exhausted' && hadHeadersTimeout && value >= 5_000,
    }
  } catch {
    return null
  }
}

export function gatewayRetryAfterMs(message: unknown): number | null {
  return gatewayRetryHint(message)?.retryAfterMs ?? null
}

function waitWithAbort(ms: number, signal?: AbortSignal): Promise<boolean> {
  if (signal?.aborted) return Promise.resolve(false)
  if (ms <= 0) return Promise.resolve(true)
  return new Promise(resolve => {
    const timer = setTimeout(() => {
      signal?.removeEventListener('abort', abort)
      resolve(true)
    }, ms)
    function abort(): void {
      clearTimeout(timer)
      resolve(false)
    }
    signal?.addEventListener('abort', abort, { once: true })
  })
}

export type RetryGuardInternals = {
  now?: () => number
  wait?: (delayMs: number, signal?: AbortSignal) => Promise<boolean>
}

/** Native request-error middleware, prepended before the stock retry plugin. */
export function registerGatewayRetryCoordinator(
  ctx: Context,
  isEnabled: () => boolean,
  internals: RetryGuardInternals = {},
): () => void {
  const runtime = ctx as unknown as RuntimeEventContext
  if (typeof runtime.on !== 'function') return () => {}
  const now = internals.now ?? Date.now
  const wait = internals.wait ?? waitWithAbort
  let isActive = true
  const unbind = runtime.on('agent/request-error', async (event, next) => {
    if (!isActive || !isEnabled() || event?.retryPolicy?.mode === 'always') return next()
    if (isAccountQuotaExhausted(event?.failure?.message)) {
      runtime.logger?.info?.('dsh-delta-code: account quota is exhausted; stopping automatic replay')
      return undefined
    }
    const hint = gatewayRetryHint(event?.failure?.message)
    if (!hint) return next()
    if (event.signal?.aborted) return undefined
    // Mirror the Gateway's explicit no-immediate-replay decision even when
    // its HTTP header did not survive the Host Provider adapter.
    if (hint.suppressAutomaticReplay) {
      runtime.logger?.info?.('dsh-delta-code: Gateway deferred exhausted mixed failures; stopping automatic replay')
      return undefined
    }
    const hintMs = hint.retryAfterMs
    const nativeMaximum = event.retryPolicy?.maxDelayMs
    const maximum = typeof nativeMaximum === 'number' && Number.isFinite(nativeMaximum) && nativeMaximum > 0
      ? nativeMaximum
      : DEFAULT_MAX_AUTO_RETRY_WAIT_MS
    // The original failure went through multiple Gateway dispatches. Do not
    // immediately replay that entire plan when recovery requires a long wait.
    if (hintMs > maximum) {
      runtime.logger?.info?.('dsh-delta-code: Gateway cooldown exceeds native auto-retry limit; stopping immediate replay')
      return undefined
    }
    // DSH owns the only retry decision and its durable session event. Some
    // adapters omit providerRetryAfterMs although the Gateway embeds the hint
    // in error JSON. Fill the remainder without duplicating that retry.
    const began = now()
    const decision = await next()
    if (decision?.kind !== 'retry' || !isActive || event.signal?.aborted) return decision
    if (typeof event.failure?.providerRetryAfterMs === 'number' && event.failure.providerRetryAfterMs >= hintMs) return decision
    const remaining = Math.max(0, hintMs - Math.max(0, now() - began))
    if (remaining === 0) return decision
    if (!await wait(remaining, event.signal)) return undefined
    if (!isActive || event.signal?.aborted) return undefined
    return decision
  }, true)
  return () => {
    isActive = false
    if (typeof unbind === 'function') unbind()
  }
}
