/** Delta compatibility contract. Client-side event collection is disabled. */
import { Service, type Context } from '@deepseek-ai/cordis'
import type { ProductEventMap } from '../events.ts'
export type {
  AnalyticsPreset, EntryAnalyticsRegion, ProductEvent, ProductEventMap,
  SidebarAnalyticsEntry, SidebarAnalyticsPosition, TrackProductEvent,
} from '../events.ts'
declare module '@deepseek-ai/cordis' { interface Context { productAnalytics: DesktopAnalytics } }
class DesktopAnalytics extends Service {
  constructor(ctx: Context) { super(ctx, 'productAnalytics') }
  get enabled(): boolean { return false }
  track<K extends keyof ProductEventMap>(_name: K, _attributes: ProductEventMap[K], _occurredAt?: number): void {}
}
export const inject: readonly string[] = []
export function apply(ctx: Context): void { ctx.plugin(DesktopAnalytics) }
