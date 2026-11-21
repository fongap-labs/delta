/** Host-independent interpretation of a language-server availability probe. */
export interface LspCommandResult {
  exitCode: number | null | undefined
  timedOut?: boolean
  signal?: string | null
}

export function classifyLspCommandResult(result: LspCommandResult): {
  available: boolean
  note: string
} {
  const available = result.exitCode === 0 && !result.timedOut && !result.signal
  if (result.timedOut) return { available: false, note: 'timed out' }
  if (!available) return {
    available: false,
    note: result.signal ? 'terminated by signal ' + result.signal : 'exit code ' + String(result.exitCode),
  }
  return { available: true, note: '' }
}
