/** Delta's native recovery dialog. No diagnostic persistence, exporting or telemetry. */
import type { MessageBoxOptions, MessageBoxReturnValue } from 'electron'
import type { DesktopMessages } from './locale.ts'
import { desktopErrorState } from './startup-error.ts'

interface RecoveryOperations {
  messages(): DesktopMessages
  show(options: MessageBoxOptions): Promise<MessageBoxReturnValue>
  stop(): Promise<void>
  disablePlugins(): Promise<void>
  exit(): void
  restart(): void
}
type FailureSource = 'host' | 'web-boot' | 'renderer' | 'main'
const DETAIL_BUDGET = 1200

export class DesktopFatalRecovery {
  private reported = false
  constructor(private readonly operations: RecoveryOperations) {}
  get active(): boolean { return this.reported }
  async report(error: unknown, _source: FailureSource): Promise<void> {
    if (this.reported) return
    this.reported = true
    const messages = this.operations.messages()
    let detail = desktopErrorState(error).message
    let message = messages.fatalSummary
    for (;;) {
      const addressInUse = /\blisten EADDRINUSE\b/u.test(detail)
      const buttons = addressInUse
        ? [messages.exitApplication, messages.restartApplication]
        : [messages.exitApplication, messages.restartApplication, messages.disableThirdPartyPlugins]
      const advice = addressInUse ? messages.startupAddressInUse : messages.startupReinstallAdvice
      const { response } = await this.operations.show({
        type: 'error', title: messages.startupFailed, message,
        detail: `${detail.slice(-DETAIL_BUDGET)}\n\n${advice}`,
        buttons, defaultId: 1, cancelId: 0, noLink: true,
      })
      if (response === 0) {
        try { await this.operations.stop() } catch (failure) { console.error('Delta recovery operation failed:', desktopErrorState(failure).message) }
        this.operations.exit()
        return
      }
      try {
        await this.operations.stop()
        if (response === 2) await this.operations.disablePlugins()
        this.operations.restart()
        return
      } catch (failure) {
        console.error('Delta recovery operation failed:', desktopErrorState(failure).message)
        message = messages.recoveryOperationFailed
        detail = desktopErrorState(failure).message
      }
    }
  }
}
