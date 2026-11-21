import { expect, it, vi } from 'vitest'
import type { MessageBoxOptions } from 'electron'
import { DesktopFatalRecovery } from '../src/fatal-recovery.ts'
import { resolveDesktopLocale } from '../src/locale.ts'

function fixture() {
  const choice = Promise.withResolvers<{ response: number; checkboxChecked: boolean }>()
  const operations = {
    messages: () => resolveDesktopLocale('en').messages,
    show: vi.fn((_options: MessageBoxOptions) => choice.promise),
    stop: vi.fn(async () => {}),
    disablePlugins: vi.fn(async () => {}),
    exit: vi.fn(), restart: vi.fn(),
  }
  return { choice, operations, recovery: new DesktopFatalRecovery(operations) }
}

it('shows fatal errors only in the native dialog', async () => {
  const { choice, operations, recovery } = fixture()
  const job = recovery.present(new Error('local backend failure'), 'host')
  await vi.waitFor(() => { expect(operations.show).toHaveBeenCalledOnce() })
  const shown = operations.show.mock.calls[0]![0]
  expect(shown.detail).toContain('local backend failure')
  expect(shown.buttons).toHaveLength(3)
  expect(shown.buttons).toEqual([operations.messages().exitApplication, operations.messages().restartApplication, operations.messages().disableThirdPartyPlugins])
  choice.resolve({ response: 0, checkboxChecked: false })
  await job
  expect(operations.exit).toHaveBeenCalledOnce()
})

it('allows local recovery from a listener conflict', async () => {
  const { choice, operations, recovery } = fixture()
  const job = recovery.present(new Error('listen EADDRINUSE'), 'main')
  await vi.waitFor(() => { expect(operations.show).toHaveBeenCalledOnce() })
  expect(operations.show.mock.calls[0]![0].buttons).toHaveLength(2)
  choice.resolve({ response: 1, checkboxChecked: false })
  await job
  expect(operations.restart).toHaveBeenCalledOnce()
})

it('deduplicates failures in the current process', async () => {
  const { choice, operations, recovery } = fixture()
  const first = recovery.present(new Error('first failure'), 'renderer')
  await vi.waitFor(() => { expect(operations.show).toHaveBeenCalledOnce() })
  await recovery.present(new Error('second failure'), 'main')
  expect(operations.show.mock.calls[0]![0].detail).not.toContain('second failure')
  choice.resolve({ response: 0, checkboxChecked: false })
  await first
})
