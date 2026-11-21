// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest'
import type { AccountNotificationBatch } from '@deepseek-ai/dsh-deepseek-account/types'
import { createNotificationController } from '../src/client/notification-controller.ts'
import { notification } from './notification-fixture.ts'

const live = (id: string): AccountNotificationBatch => ({
  nextPollAfterSeconds: 300,
  notifications: [notification(id, { displayUntil: '2099-01-01T00:00:00Z' })],
})
const controllers: Array<{ end(): void }> = []
afterEach(() => {
  for (const controller of controllers) controller.end()
  controllers.length = 0
  vi.restoreAllMocks()
  vi.useRealTimers()
})

function fixture() {
  const read = vi.fn<() => Promise<AccountNotificationBatch | null>>(async () => live('local-notice'))
  const readContent = vi.fn(async () => live('local-notice').notifications[0]!)
  const publish = vi.fn()
  const controller = createNotificationController({
    locale: 'en', pollIntervalMs: () => 300_000, read, readContent, publish,
  })
  controllers.push(controller)
  return { controller, read, readContent, publish }
}

it('reads notifications on login but never accepts or emits behavior events', async () => {
  const { controller, read, publish } = fixture()
  expect(Object.keys(controller)).not.toContain('ctaClicked')
  controller.begin()
  await vi.waitFor(() => { expect(read).toHaveBeenCalledOnce() })
  await vi.waitFor(() => { expect(publish).toHaveBeenCalled() })
  expect(publish.mock.lastCall?.[0]).toHaveLength(1)
  controller.shown('local-notice' as never)
  expect(publish.mock.lastCall?.[0]).toHaveLength(1)
  controller.dismiss('local-notice' as never)
  expect(publish.mock.lastCall?.[0]).toEqual([])
})

it('preserves displayed foreground notifications across local refreshes', async () => {
  const { controller, read, publish } = fixture()
  controller.begin()
  await vi.waitFor(() => { expect(publish).toHaveBeenCalled() })
  controller.shown('local-notice' as never)
  read.mockResolvedValue({ nextPollAfterSeconds: 300, notifications: [] })
  await controller.refresh()
  expect(publish.mock.lastCall?.[0]).toHaveLength(1)
  controller.dismiss('local-notice' as never)
  expect(publish.mock.lastCall?.[0]).toEqual([])
})

it('clears local notification state when the account lifecycle ends', async () => {
  const { controller, publish } = fixture()
  controller.begin()
  await vi.waitFor(() => { expect(publish).toHaveBeenCalled() })
  controller.end()
  expect(publish.mock.lastCall?.[0]).toEqual([])
})
