/**
 * Pure completed-process evidence assessment. The DSH adapter owns execution;
 * this contract never spawns, retries or grants Host permissions.
 *
 * DSH subprocess outcomes contain exitCode and signal. A zero exit is not
 * proof of success when a process was signaled, timed out, or output is incomplete.
 */
export interface ProcessExitEvidence {
  readonly exitCode?: number | null
  readonly signal?: string | null
  readonly timedOut?: boolean
  readonly lossy?: boolean
}

export type ProcessOutcomeStatus = 'succeeded' | 'failed' | 'uncertain'
export type ProcessOutcomeReason =
  | 'clean-exit' | 'nonzero-exit' | 'timeout' | 'signal'
  | 'missing-exit' | 'invalid-outcome' | 'incomplete-output'

export interface ProcessOutcomeAssessment {
  readonly status: ProcessOutcomeStatus
  readonly complete: boolean
  readonly reason: ProcessOutcomeReason
}

const uncertain = (reason: ProcessOutcomeReason): ProcessOutcomeAssessment =>
  ({ status: 'uncertain', complete: false, reason })

/** Missing or malformed Host exit facts fail closed; output loss is never complete evidence. */
export function assessProcessOutcome(value: ProcessExitEvidence | null | undefined): ProcessOutcomeAssessment {
  if (!value || typeof value !== 'object') return uncertain('invalid-outcome')
  if (value.timedOut === true) return uncertain('timeout')
  if (value.timedOut !== false) return uncertain('invalid-outcome')
  if (value.signal !== null) {
    return uncertain(typeof value.signal === 'string' && value.signal.length > 0
      ? 'signal' : 'invalid-outcome')
  }
  if (value.exitCode === null || value.exitCode === undefined) return uncertain('missing-exit')
  if (!Number.isSafeInteger(value.exitCode) || value.exitCode < 0) return uncertain('invalid-outcome')
  if (value.exitCode !== 0) return { status: 'failed', complete: false, reason: 'nonzero-exit' }
  if (value.lossy !== false) return { status: 'succeeded', complete: false, reason: 'incomplete-output' }
  return { status: 'succeeded', complete: true, reason: 'clean-exit' }
}
