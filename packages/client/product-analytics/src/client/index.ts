/** Compatibility-only type surface: Delta does not mount a tracking service. */
import type { Context } from '@deepseek-ai/cordis'
import type { ProductEventMap } from '../events.ts'
export type {
  AnalyticsPreset, EntryAnalyticsRegion, ProductEvent, ProductEventMap,
  SidebarAnalyticsEntry, SidebarAnalyticsPosition, TrackProductEvent,
} from '../events.ts'

class DisabledAnalytics {
  readonly enabled = false
  track<K extends keyof ProductEventMap>(_name: K, _attributes: ProductEventMap[K], _timestamp?: number): void {}
}
declare module '@deepseek-ai/cordis' {
  interface Context { productAnalytics: DisabledAnalytics }
}
/** No registration, sender, policy stream, queue, identifier or remote call. */
export function apply(_ctx: Context): void {}
