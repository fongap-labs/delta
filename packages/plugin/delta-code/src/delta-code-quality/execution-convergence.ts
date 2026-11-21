/** Host-independent, read-only execution decision based on native Todo and observed tool evidence. */
import type { FailureKind, VerificationEvidenceStatus } from './failure-policy.js'
import type { ResumeState } from './resume-policy.js'
import { remainingDeliveryTasks } from './delivery-policy.js'

export type ExecutionObservation =
  | { kind: 'none' }
  | { kind: 'verification'; status: VerificationEvidenceStatus }
  | { kind: 'failure'; failure: FailureKind }
  | { kind: 'mutation' }
  | { kind: 'delivery'; ready: boolean }

export type ExecutionTransition = 'blocked' | 'reconcile' | 'diagnose' | 'continue' | 'verify'

export interface ExecutionDecision {
  transition: ExecutionTransition
  nextAction: string
  remaining: string[]
  observation: ExecutionObservation['kind']
  /** Never a claim of passed tests or completed delivery. */
  deliveryConfirmed: false
}

/** No model-written task status or textual error is allowed as evidence here. */
export function decideExecutionTransition(
  state: ResumeState | undefined, observation: ExecutionObservation,
): ExecutionDecision {
  const choose = (transition: ExecutionTransition, nextAction: string, remaining: string[] = []): ExecutionDecision =>
    ({ transition, nextAction, remaining, observation: observation.kind, deliveryConfirmed: false })
  if (!state || state.invalid || state.writeSeq < 0 || !state.tasks?.length) {
    return choose('blocked', 'Restore the native Host Todo checkpoint before continuing; do not invent task progress.')
  }
  let remaining: string[]
  try { remaining = remainingDeliveryTasks(state.tasks) } catch {
    return choose('blocked', 'Repair invalid native Todo state before continuing.')
  }
  if (observation.kind === 'failure') {
    switch (observation.failure) {
      case 'permission':
        return choose('blocked', 'Request Host permission; never bypass authorization or retry the denied operation.', remaining)
      case 'environment':
        return choose('blocked', 'Restore missing Host capability, quota or executable before resuming.', remaining)
      case 'uncertain':
      case 'unknown':
        return choose('reconcile', 'Inspect actual Host/Git state before retrying an operation with an unconfirmed outcome.', remaining)
      case 'transient':
        return choose('reconcile', 'Check provider recovery and idempotency before any explicit retry.', remaining)
      case 'verification':
        return choose('diagnose', 'Diagnose concrete failing checks and edit only the affected scope.', remaining)
    }
  }
  if (observation.kind === 'verification') {
    if (observation.status === 'failed') {
      return choose('diagnose', 'Inspect the failed verification and fix its cause; do not repeat unchanged checks.', remaining)
    }
    if (observation.status === 'unconfirmed') {
      return choose('reconcile', 'Verify the incomplete test outcome with fresh Host evidence before continuing.', remaining)
    }
  }
  if (observation.kind === 'mutation') {
    return choose('verify', 'A source-changing tool ran after prior checks; run fresh targeted verification.', remaining)
  }
  if (remaining.length) {
    return choose('continue', 'Reconcile Git state and validate the next incomplete native Todo item: ' + remaining[0], remaining)
  }
  // Native Todo completion and earlier verification are not a delivery certificate.
  return choose('verify', 'Recheck current Git HEAD, run fresh final checks and require code_delivery_gate before delivery.')
}
