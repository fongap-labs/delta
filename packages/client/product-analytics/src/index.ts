/** Delta compatibility contract. Host-side event collection and export are disabled. */
import { type Context } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import { Remote, TypertRemoteService } from '@deepseek-ai/dsh-typert-protocol'
import type { ProductEvent } from './events.ts'

declare module '@deepseek-ai/cordis' { interface Context { productAnalytics: ProductAnalytics } }

export default class ProductAnalytics extends TypertRemoteService {
  static inject = []
  static Config = z.object({ enabled: z.boolean().default(false).volatile() })
  constructor(ctx: Context) { super(ctx, 'productAnalytics') }
  @Remote enabled(): boolean { return false }
  @Remote({ mode: 'stream' }) async *watchPolicy(signal: AbortSignal): AsyncIterable<boolean> { if (!signal.aborted) yield false }
  @Remote report(_event: ProductEvent): void { /* Deliberately empty. */ }
}
