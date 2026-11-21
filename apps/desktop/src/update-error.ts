/** Localized update preparation failures with separately displayed diagnostics. */
import type { DesktopUpdatePreparationFailureKind, DesktopUpdateState } from './ipc.ts'
import { redactDiagnostic } from './privacy-redaction.ts'

/** Only explicitly safe main-process facts may be exposed as technical details. */
export class DesktopUpdatePreparationError extends Error {
  /**
   * @param kind - Stable preparation cause shared by native and Web presentations.
   * @param message - Locale-owned recovery guidance.
   * @param technicalDetails - Main-owned facts, excluding raw subprocess output and credentials.
   */
  constructor(readonly kind: DesktopUpdatePreparationFailureKind, message: string, readonly technicalDetails?: string) { super(message) }
}

/**
 * Preserve the stable filesystem error code and a redacted message; never expose paths or destinations.
 * @param error - Rejected updater download or filesystem error.
 * @returns Structured filesystem cause when supplied by the failing operation.
 */
export function downloadFailureDetails(error: unknown): Pick<DesktopUpdateState, 'downloadFailure' | 'technicalDetails'> {
  if (!(error instanceof Error) || !('code' in error) || typeof error.code !== 'string') return {}
  const code = error.code
  const kind = code === 'ENOSPC' ? 'space' : ['EACCES', 'EPERM', 'EROFS'].includes(code) ? 'permission' : code === 'EBUSY' ? 'busy' : undefined
  return {
    ...(kind === undefined ? {} : { downloadFailure: kind }),
    technicalDetails: redactDiagnostic([code, error.message].join('\n')),
  }
}
