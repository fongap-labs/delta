/** Native compaction watermark only: never copy the summary, transcript or prompts. */
export interface CompactionMarker {
  summarySeq: number
  summaries: number
  invalid: boolean
}
export interface CompactionEvent { type: string; seq: number; data?: unknown }
export const EMPTY_COMPACTION: CompactionMarker = { summarySeq: -1, summaries: 0, invalid: false }

/** Pure native-log fold; ignore all unrelated events and never infer summary quality. */
export function foldCompactionMarker(state: CompactionMarker, event: CompactionEvent): CompactionMarker {
  if (event.type !== 'compaction/summary') return state
  const data = event.data as { summary?: unknown } | undefined
  if (state.invalid || !Number.isSafeInteger(event.seq) || event.seq < 0 ||
      event.seq <= state.summarySeq || !Array.isArray(data?.summary) ||
      !Number.isSafeInteger(state.summaries) || state.summaries < 0 ||
      state.summaries >= Number.MAX_SAFE_INTEGER) {
    return { ...state, invalid: true }
  }
  return { summarySeq: event.seq, summaries: state.summaries + 1, invalid: false }
}

/** Validate a host-cached checkpoint before trusting its watermark. */
export function parseCompactionMarker(value: unknown): CompactionMarker {
  if (!value || typeof value !== 'object') throw new Error('invalid native compaction checkpoint')
  const state = value as Partial<CompactionMarker>
  if (!Number.isSafeInteger(state.summarySeq) || state.summarySeq! < -1 ||
      !Number.isSafeInteger(state.summaries) || state.summaries! < 0 ||
      typeof state.invalid !== 'boolean' ||
      (state.summarySeq === -1 && state.summaries !== 0) ||
      (state.summarySeq! >= 0 && state.summaries === 0)) {
    throw new Error('invalid native compaction checkpoint')
  }
  return state as CompactionMarker
}

export interface NativeCompactionReader {
  read(agent: unknown): CompactionMarker | undefined
  dispose(): void
}

/** A watermark is evidence that compaction occurred, not proof of preserved decisions. */
export function compactionReview(
  marker: CompactionMarker | undefined, todoSeq: number,
): { state: 'unavailable' | 'none' | 'review'; lastSummarySeq: number; todoUpdatedAfterSummary: boolean; action: string } {
  if (!marker || marker.invalid) {
    return {
      state: 'unavailable', lastSummarySeq: -1, todoUpdatedAfterSummary: false,
      action: 'Native compaction evidence unavailable; do not assert that compressed decisions were retained.',
    }
  }
  if (marker.summarySeq < 0) {
    return { state: 'none', lastSummarySeq: -1, todoUpdatedAfterSummary: false, action: '' }
  }
  const todoUpdatedAfterSummary = Number.isSafeInteger(todoSeq) && todoSeq > marker.summarySeq
  return {
    state: 'review', lastSummarySeq: marker.summarySeq, todoUpdatedAfterSummary,
    action: 'Native compaction occurred. Recheck the user constraints, technical decisions, unresolved failures and test evidence against the current Host Session and Git state; do not reconstruct missing facts from memory.',
  }
}
