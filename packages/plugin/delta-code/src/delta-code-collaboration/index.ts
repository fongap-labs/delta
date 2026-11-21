/** Delegation is a policy, not a second Agent runtime. */
export interface DeltaDelegationRequest {
  readonly independentTasks: number
  readonly sharedWriteTargets: boolean
  readonly nativeSubagentsAvailable: boolean
}
export type DeltaDelegation = 'single-agent' | 'native-subagents'

export function selectDeltaDelegation(request: DeltaDelegationRequest): DeltaDelegation {
  if (!request.nativeSubagentsAvailable || request.sharedWriteTargets || request.independentTasks < 2)
    return 'single-agent'
  return 'native-subagents'
}
