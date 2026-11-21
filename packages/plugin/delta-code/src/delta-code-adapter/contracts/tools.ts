/** Pure Delta contract: business modules never need a Host tool registrar. */
export interface DeltaCodeTool {
  readonly name: string
}
export interface DeltaCapabilityState {
  readonly available: boolean
  readonly reason?: 'absent' | 'denied' | 'incompatible'
}
