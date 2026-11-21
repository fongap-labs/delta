import { setTimeout as delay } from 'node:timers/promises'

export type FailureKind = 'permission' | 'environment' | 'transient' | 'verification' | 'uncertain' | 'unknown'

/** Account quota exhaustion is not a short-lived transport failure. */
export function isAccountQuotaExhausted(value: unknown): boolean {
  const message = value instanceof Error ? value.message : typeof value === 'string' ? value : ''
  return /(?:insufficient[_ -]?quota|quota[_ -]?(?:exhausted|exceeded|depleted)|(?:credit|balance)s?\s+(?:exhausted|depleted|insufficient)|usage\s+limit\s+(?:reached|exceeded)|billing\s+limit\s+(?:reached|exceeded))/i.test(message.slice(0, 8192))
}

/** Authorization failures always take precedence over transient transport errors. */
export function classifyFailure(value: unknown): FailureKind {
  const message = value instanceof Error ? value.message : String(value)
  if (/\b(?:EACCES|EPERM|permission denied|access denied|not authorized|unauthorized|forbidden|host refused|host rejected|host denied|HTTP\s*40[13])\b/i.test(message)) {
    return 'permission'
  }
  // Exhausted account quotas require a new authorized capacity source, not blind retries.
  if (isAccountQuotaExhausted(message)) return 'environment'
  // A missing result may reflect a command still running: do not auto-retry it.
  if (/\b(?:outcome (?:is )?(?:unconfirmed|unknown)|verification deadline exceeded|result (?:was )?lost|exit (?:code|status) unavailable|process state unknown)\b/i.test(message)) return 'uncertain'
  if (/\b(?:ENOENT|ENOTDIR|command not found|executable not found|host subprocess service (?:is )?(?:unavailable|required)|missing subprocess|cannot find (?:module|executable)|not recognized as an internal or external command)\b/i.test(message)) return 'environment'
  if (/\b(?:HTTP\s*(?:429|502|503|504)|status[=: ]+(?:429|502|503|504)|ECONNRESET|ECONNREFUSED|ETIMEDOUT|EAI_AGAIN|rate.limit(?:ed)?|temporarily unavailable|service unavailable)\b/i.test(message)) {
    return 'transient'
  }
  if (/\b(?:assertion failed|test failure|tests? failed|compile error|typecheck failed)\b/i.test(message)) {
    return 'verification'
  }
  return 'unknown'
}

/** Treat failed commands with a known exit status as test failures, not Host permission denials. */
export function classifyVerificationEvidence(evidence: {
  passed: boolean; timedOut: boolean; exitCode: number; signal?: string; truncated: boolean; output: string
}): FailureKind | 'none' {
  // Raw Host facts outrank a result's self-reported passed flag.
  if (evidence.timedOut || evidence.signal || !Number.isSafeInteger(evidence.exitCode) ||
      evidence.exitCode < 0) return 'uncertain'
  if (evidence.truncated) return 'unknown'
  if (evidence.exitCode !== 0) {
    return classifyFailure(evidence.output) === 'transient' ? 'transient' : 'verification'
  }
  return evidence.passed ? 'none' : 'unknown'
}

/** Only complete Host evidence can influence verified delivery/Skill feedback counters. */
export type VerificationEvidenceStatus = 'passed' | 'failed' | 'unconfirmed'

function confirmedStep(input: unknown): VerificationEvidenceStatus {
  if (!input || typeof input !== 'object') return 'unconfirmed'
  const step = input as Record<string, unknown>
  if (step.timedOut !== false || step.truncated !== false ||
      step.signal !== '' || !Number.isSafeInteger(step.exitCode)) return 'unconfirmed'
  if (step.passed === true && step.exitCode === 0 &&
      step.outcome === 'succeeded') return 'passed'
  if (step.passed === false && typeof step.exitCode === 'number' &&
      step.exitCode > 0 && step.outcome === 'failed') return 'failed'
  return 'unconfirmed'
}

/** Returns unconfirmed on contradictory, partial or underspecified verification. */
export function verificationEvidenceStatus(toolName: string, result: unknown): VerificationEvidenceStatus {
  if (toolName === 'code_verify') return confirmedStep(result)
  if (toolName !== 'code_verify_suite' || !result || typeof result !== 'object') return 'unconfirmed'
  const suite = result as Record<string, unknown>
  if (!Array.isArray(suite.results) || suite.results.length === 0 ||
      !Array.isArray(suite.skipped)) return 'unconfirmed'
  const steps = suite.results.map(confirmedStep)
  if (steps.includes('unconfirmed')) return 'unconfirmed'
  if (suite.passed === true && suite.skipped.length === 0 &&
      steps.every(step => step === 'passed')) return 'passed'
  if (suite.passed === false && steps.includes('failed')) return 'failed'
  return 'unconfirmed'
}

/** Human-only follow-up; failed verification commands are never retried automatically. */
export function verificationNextAction(kind: FailureKind | 'none'): string {
  switch (kind) {
    case 'none': return 'Verification passed; inspect the diff and delivery requirements.'
    case 'permission': return 'Request Host authorization; do not bypass permissions or retry automatically.'
    case 'environment': return 'Restore the missing Host subprocess or executable; do not retry until the environment is repaired.'
    case 'transient': return 'Investigate the temporary dependency/transport failure; rerun explicitly only after confirming safety.'
    case 'verification': return 'Diagnose the failing check and change the hypothesis before rerunning.'
    case 'uncertain': return 'Inspect whether the Host command is still running or finished; the outcome is unconfirmed. Do not retry automatically.'
    default: return 'Inspect the Host outcome before any rerun; the command may have executed.'
  }
}

/**
 * Only read-only, repeatable operations may call this helper.
 * A failed write or test may have completed before its response was lost.
 */
export async function retryReadOnly<T>(
  run: () => Promise<T>, signal: AbortSignal, maxAttempts = 2,
): Promise<T> {
  if (!Number.isSafeInteger(maxAttempts) || maxAttempts < 1 || maxAttempts > 3) {
    throw new Error('read-only retry attempts must be 1..3')
  }
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    signal.throwIfAborted()
    try {
      return await run()
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      // A gateway may return HTTP 429/5xx while explicitly marking the error
      // non-retryable. Respect that contract even for read-only operations.
      const nonRetryable = /(?:["']?retryable["']?\s*[:=]\s*false\b|\bnon[- ]retryable\b)/i.test(message)
      if (signal.aborted || attempt === maxAttempts || nonRetryable || classifyFailure(error) !== 'transient') {
        throw error
      }
      await delay(100 * attempt, undefined, { signal })
    }
  }
  throw new Error('unreachable read-only retry state')
}
