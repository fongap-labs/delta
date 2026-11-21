/** Host-independent mode and compensation policy for code_multi_edit. */
export function shouldPreflightMultiEdit(isPreflight: boolean | undefined, editCount: number): boolean {
  return isPreflight === true || (isPreflight !== false && editCount > 1)
}

/** Host FS guarded-intent is the only contract where compensation is permitted. */
export function canCompensateMultiEdit(
  shouldPreflight: boolean, isDryRun: boolean, fsWriteContract: string | undefined,
): boolean {
  return shouldPreflight && !isDryRun && fsWriteContract === 'guarded-intent'
}
