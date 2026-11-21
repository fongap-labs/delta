/** Delta: session log request upload has been removed, not merely disabled in settings. */
export type * from './types.ts'
export const name = 'session-log-deepseek'
export const inject: readonly string[] = []
/** Compatibility-only entry; never adds dsh_session_log to a model request. */
export function apply(): void {}
