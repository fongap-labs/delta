export type BrowserActionInput = {
  action: 'open' | 'inspect' | 'click' | 'fill' | 'press' | 'screenshot' | 'close'
  url?: string
  selector?: string
  text?: string
  key?: string
}

export type BrowserActionResult = {
  status: 'success' | 'denied' | 'cancelled' | 'error'
  action: BrowserActionInput['action']
  url: string
  title: string
  text: string
  image: null | {
    attachmentId: string
    mediaType: 'image/png' | 'image/jpeg' | 'image/webp' | 'image/gif'
    bytes: number
    width: number
    height: number
    name?: string
    originalDimensions?: { width: number; height: number }
  }
}

export type BrowserProviderOptions = {
  root: string
  allowLocalNetwork?: boolean
  channel?: 'msedge' | 'chrome'
  chromium?: { launchPersistentContext(userDataDir: string, options: object): Promise<unknown> }
  lookup?: (hostname: string) => Promise<Array<{ address: string; family: number }>>
  attachments: { saveImage(input: { data: Uint8Array; mediaType: 'image/jpeg'; name: string }): Promise<BrowserActionResult['image']> }
  approval: { request(request: object): Promise<string> }
}

export declare function validateBrowserAction(value: unknown): BrowserActionInput
export declare function createBrowserProvider(options: BrowserProviderOptions): {
  execute(value: unknown, exec: { agent?: { session: { id: string } }; callId?: string; signal: AbortSignal }): Promise<BrowserActionResult>
  dispose(): Promise<void>
}
export declare function apply(ctx: object, config: object): void
