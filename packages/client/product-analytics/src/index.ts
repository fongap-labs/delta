/** Delta compatibility boundary: telemetry collection and external reporting are removed. */
import type { Context } from '@deepseek-ai/cordis'
import { Remote, TypertRemoteService } from '@deepseek-ai/dsh-typert-protocol'
import type { ProductEvent } from './events.ts'

/** Retained only to satisfy existing generated RPC type contracts. */
export default class ProductAnalytics extends TypertRemoteService {
  static inject: string[] = []
  constructor(ctx: Context) { super(ctx, 'productAnalytics') }
  @Remote enabled(): boolean { return false }
  @Remote({ mode: 'stream' }) async *watchPolicy(_signal: AbortSignal): AsyncIterable<boolean> { yield false }
  @Remote async report(_event: ProductEvent): Promise<void> {}
}
