export type ComputerActionInput = {
  action: 'screenshot' | 'list_windows' | 'click' | 'type' | 'press' | 'scroll'
  screenId?: string
  x?: number
  y?: number
  button?: 'left' | 'right' | 'middle'
  count?: 1 | 2
  text?: string
  key?: string
  delta?: number
}

export type ComputerActionResult = {
  status: 'success' | 'denied' | 'cancelled' | 'error'
  action: ComputerActionInput['action']
  text: string
  screenId: string
  width: number
  height: number
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

export type ComputerProviderOptions = {
  attachments: { saveImage(input: { data: Uint8Array; mediaType: 'image/jpeg'; name: string }): Promise<ComputerActionResult['image']> }
  approval: { request(request: object): Promise<string> }
  runNative?: (input: object, signal: AbortSignal) => Promise<object>
}

export declare function validateComputerAction(value: unknown): ComputerActionInput
export declare function createComputerProvider(options: ComputerProviderOptions): {
  execute(value: unknown, exec: { agent?: { session: { id: string } }; callId?: string; signal: AbortSignal }): Promise<ComputerActionResult>
  dispose(): Promise<void>
}
export declare function apply(ctx: object, config?: object): void
