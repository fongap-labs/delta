/** Bounded, redacted error summary for the local Desktop recovery dialog. */
import { redactDiagnostic } from './privacy-redaction.ts'

/**
 * Keep stack traces, enumerable properties and nested causes out of UI state.
 * @param error - Startup or runtime failure.
 * @returns Serializable error state.
 */
export function desktopErrorState(error: unknown): { phase: 'error'; message: string } {
  const message = error instanceof AggregateError
    ? [error.message, ...error.errors.map(item => desktopErrorState(item).message)].join('\n')
    : error instanceof Error ? error.message : String(error)
  return { phase: 'error', message: redactDiagnostic(message) }
}
