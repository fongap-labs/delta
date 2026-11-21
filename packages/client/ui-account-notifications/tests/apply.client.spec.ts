// @vitest-environment jsdom
/** Notification integration shares the account plugin's single state stream. */
import { afterEach, beforeEach, expect, onTestFinished, vi } from 'vitest'
import { ok } from '@deepseek-ai/dsh-remote-mock'
import { RemoteError } from '@deepseek-ai/dsh-typert-protocol'
import { createClientTest, type TestClient, webApp } from '@deepseek-ai/dsh-client-test-runtime/src/assembly/index.ts'
import type { AccountNotificationBatch, AccountNotificationId, AccountDetails, AccountView, SignInAttemptId } from '@deepseek-ai/dsh-deepseek-account/types'
import type { AccountSectionInjected } from '@deepseek-ai/dsh-client-ui-settings-account/client'
import type { AccountNotificationsInjected } from '../src/client/AccountNotifications.tsx'
import type { Context } from '@deepseek-ai/cordis'
import { syncedConfig } from '@deepseek-ai/dsh-client-test-runtime'
import { apply as notificationsApply, inject as notificationsInject } from '../src/client/index.ts'
import type { AccountNotificationsConfig } from '../src/types.ts'
import { notification } from './notification-fixture.ts'

const it = createClientTest({ roster: webApp })
/** The same composition with a one-second initial polling delay synced from the Host. */
const itWithInterval = createClientTest({ roster: webApp, provide: {
  '@deepseek-ai/dsh-client-ui-account-notifications': { inject: notificationsInject, apply: (ctx: Context) => {
    notificationsApply(ctx, syncedConfig<AccountNotificationsConfig>({ notificationPollIntervalMs: 1_000 }))
  } },
} })
const SELF = '@deepseek-ai/dsh-client-ui-settings-account'
const view: AccountView = { status: 'signed-out', attempt: null,
  links: { usageUrl: 'https://platform.deepseek.com/usage', topUpUrl: 'https://platform.deepseek.com/top_up' } }
const stored: AccountView = { ...view, status: 'credential-stored' }
function bonus(orderId: string, message = 'Awarded 5.00'): AccountNotificationBatch {
  return { nextPollAfterSeconds: 300, notifications: [notification(orderId, { body: message, displayUntil: '2099-01-01T00:00:00Z' })] }
}
function operations(c: TestClient): AccountSectionInjected {
  const injected: object = c.ctx.slots.entries('settings.launcher')[0]!.inject!()
  return injected as AccountSectionInjected
}
function notificationOperations(c: TestClient): AccountNotificationsInjected {
  const injected: object = c.ctx.slots.entries('settings.launcher.notifications')[0]!.inject!()
  return injected as AccountNotificationsInjected
}
beforeEach(() => { vi.stubEnv('DSH_CLIENT_VERSION', '0.0.0-test') })
afterEach(() => { vi.unstubAllGlobals(); vi.unstubAllEnvs(); vi.restoreAllMocks() })

it('starts one notification poll across the credential commit and login-success frames', async ({ start }) => {
  const c = await start()
  c.mock.remote.session.initializeDefaultModel.mockResolvedValue(ok(undefined))
  const reads = c.mock.remote.account.getNotifications
  const id = 'one-login' as SignInAttemptId
  c.mock.streams.push('account/watch', { type: 'snapshot', state: { ...stored, attempt: { id, phase: 'committing' } } })
  await c.flush()
  expect(reads).toHaveBeenCalledTimes(1)
  c.mock.streams.push('account/watch', { type: 'snapshot', state: { ...stored, attempt: { id, phase: 'succeeded' } } })
  await c.flush()
  expect(reads).toHaveBeenCalledTimes(1)
  c.mock.streams.push('account/watch', { type: 'snapshot', state: { ...stored, attempt: { id, phase: 'succeeded' } } })
  await c.flush()
  expect(reads).toHaveBeenCalledTimes(1)
}, 60_000)

for (const outcome of ['null', 'failure'] as const) it(`queries a replacement credential without adopting the old ${outcome} reply`, async ({ start }) => {
  const c = await start()
  const previous = Promise.withResolvers<ReturnType<typeof ok<AccountNotificationBatch | null>>>()
  const reads = c.mock.remote.account.getNotifications
  reads.mockReturnValueOnce(previous.promise)
  c.mock.streams.push('account/watch', { type: 'snapshot', state: stored })
  await c.flush()
  reads.mockResolvedValueOnce(ok(bonus('replacement-notice')))
  c.mock.streams.push('account/watch', { type: 'credentials-changed', state: stored })
  await c.flush()
  expect(reads).toHaveBeenCalledTimes(2)
  expect(notificationOperations(c).hooks.notices.getSnapshot()?.[0]?.id).toBe('replacement-notice')
  if (outcome === 'null') previous.resolve(ok(null))
  else previous.reject(new Error('obsolete credential'))
  await c.flush()
  expect(notificationOperations(c).hooks.notices.getSnapshot()?.[0]?.id).toBe('replacement-notice')
}, 60_000)

it('drops the previous account notice and stops reading after sign-out', async ({ start }) => {
  vi.stubGlobal('dshDesktop', {})
  const c = await start()
  const actions = operations(c)
  c.mock.remote.account.getNotifications.mockResolvedValue(ok(bonus('4c1b0000-0000-4000-8000-000000000001')))
  c.mock.streams.push('account/watch', { type: 'snapshot', state: stored })
  await vi.waitFor(() => {
    expect(notificationOperations(c).hooks.notices.getSnapshot()?.[0]?.body).toBe('Awarded 5.00')
  })
  c.mock.streams.push('account/watch', { type: 'snapshot', state: view })
  await vi.waitFor(() => { expect(actions.hooks.account.getSnapshot().view).toEqual(view) })
  expect(notificationOperations(c).hooks.notices.getSnapshot()?.[0]).toBeUndefined()
  // Signing out ends the lifecycle, so no further read follows.
  const reads = c.mock.remote.account.getNotifications.mock.calls.length
  await new Promise((resolve) => { setTimeout(resolve, 20) })
  expect(c.mock.remote.account.getNotifications.mock.calls.length).toBe(reads)
}, 60_000)

it('refreshes balances and notifications immediately on Settings entry', async ({ start }) => {
  vi.stubGlobal('dshDesktop', {})
  const c = await start()
  const actions = operations(c)
  const balance: AccountDetails['balance'] = {
    status: 'ready', value: [{ currency: 'CNY', balance: '12.34' }], bonusWallets: [{ currency: 'CNY', balance: '5.00' }],
  }
  c.mock.remote.account.getBalance.mockResolvedValue(ok(balance))
  c.mock.remote.account.getNotifications.mockResolvedValue(ok(null))
  c.mock.streams.push('account/watch', { type: 'snapshot', state: stored })
  // Account activation starts with an immediate read.
  await vi.waitFor(() => { expect(c.mock.remote.account.getNotifications).toHaveBeenCalledTimes(1) })
  // Signing in already read the wallet once through the details refresh.
  const balances = c.mock.remote.account.getBalance.mock.calls.length
  await actions.refreshAccount()
  expect(c.mock.remote.account.getBalance).toHaveBeenCalledTimes(balances + 1)
  expect(c.mock.remote.account.getNotifications).toHaveBeenCalledTimes(2)
  // The bonus read carries the active UI language.
  expect(c.mock.remote.account.getNotifications).toHaveBeenLastCalledWith(expect.objectContaining({ locale: 'en' }))
}, 60_000)

itWithInterval('replaces polling on startup, sign-in and Settings entry and stops on sign-out or unload', async ({ start }) => {
  const c = await start()
  c.mock.remote.account.getNotifications.mockResolvedValue(ok(null))
  c.mock.streams.push('account/watch', { type: 'snapshot', state: stored })
  await c.flush()
  vi.useFakeTimers()
  try {
    await c.reload(SELF)
    await c.flush()
    const actions = operations(c)
    const reads = c.mock.remote.account.getNotifications
    reads.mockClear()
    // Startup receives the stored account in its initial stream snapshot.
    c.mock.streams.push('account/watch', { type: 'snapshot', state: stored })
    await c.flush()
    expect(reads).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(500)
    await actions.refreshAccount()
    expect(reads).toHaveBeenCalledTimes(2)
    await vi.advanceTimersByTimeAsync(500)
    expect(reads).toHaveBeenCalledTimes(2)
    await vi.advanceTimersByTimeAsync(500)
    expect(reads).toHaveBeenCalledTimes(3)
    c.mock.streams.push('account/watch', { type: 'snapshot', state: view })
    await c.flush()
    await vi.advanceTimersByTimeAsync(2_000)
    expect(reads).toHaveBeenCalledTimes(3)
    c.mock.streams.push('account/watch', { type: 'snapshot', state: stored })
    await c.flush()
    expect(reads).toHaveBeenCalledTimes(4)
    await c.unload(SELF)
    await vi.advanceTimersByTimeAsync(2_000)
    expect(reads).toHaveBeenCalledTimes(4)
  } finally { vi.useRealTimers() }
}, 60_000)

it('publishes a failed balance from a Settings entry without dropping the bonus read', async ({ start }) => {
  vi.stubGlobal('dshDesktop', {})
  const c = await start()
  const actions = operations(c)
  c.mock.remote.account.getBalance.mockResolvedValueOnce({ ok: false, error: new RemoteError('gateway/internal', 'offline', {}) })
  c.mock.remote.account.getNotifications.mockResolvedValue(ok(null))
  c.mock.streams.push('account/watch', { type: 'snapshot', state: stored })
  await vi.waitFor(() => { expect(c.mock.remote.account.getNotifications).toHaveBeenCalledTimes(1) })
  await actions.refreshAccount()
  expect(actions.hooks.account.getSnapshot().details?.balance).toEqual({ status: 'failed' })
  // The bonus read still ran: a failed wallet read does not cancel it.
  expect(c.mock.remote.account.getNotifications).toHaveBeenCalledTimes(2)
}, 60_000)

it('retains notification content when the localization RPC is refused', async ({ start }) => {
  const c = await start()
  const id = '4c1b0000-0000-4000-8000-000000000000'
  const output = vi.spyOn(console, 'error').mockImplementation(() => {})
  c.mock.remote.account.getNotifications.mockResolvedValue(ok(bonus(id)))
  c.mock.remote.account.getNotificationContent.mockResolvedValue({ ok: false, error: new RemoteError('gateway/internal', 'offline', {}) })
  c.mock.streams.push('account/watch', { type: 'snapshot', state: stored })
  await c.flush()
  c.mock.remote.settings.mutate.mockResolvedValue(ok({
    ns: 'locale', autoGenerate: false, schema: { type: 'object' }, value: { preference: 'zh' },
    applies: 'live', secrets: [], revision: 1,
  }))
  c.ctx.locale.setLocale('zh')
  await vi.waitFor(() => { expect(output).toHaveBeenCalledWith('[deepseek-account] notification content read failed') })
  expect(notificationOperations(c).hooks.notices.getSnapshot()?.[0]?.body).toBe('Awarded 5.00')
}, 60_000)

it('clears notifications and stops polling when the account carrier fails', async ({ start }) => {
  const c = await start()
  const open = c.ctx.remote.$stream.bind(c.ctx.remote)
  let fail: (() => void) | undefined
  vi.spyOn(c.ctx.remote, '$stream').mockImplementation((options) => {
    if (options.name === 'account') fail = () => {
      options.carrierFailed?.(Object.assign(new Error('connection lost'), { code: 'gateway/carrier-lost' as const }))
    }
    return open(options)
  })
  await c.reload(SELF)
  c.mock.remote.account.getNotifications.mockResolvedValue(ok(bonus('4c1b0000-0000-4000-8000-000000000000')))
  c.mock.streams.push('account/watch', { type: 'snapshot', state: stored })
  await c.flush()
  expect(notificationOperations(c).hooks.notices.getSnapshot()).toHaveLength(1)
  const reads = c.mock.remote.account.getNotifications.mock.calls.length
  vi.useFakeTimers()
  try {
    fail?.()
    expect(notificationOperations(c).hooks.notices.getSnapshot()).toEqual([])
    await vi.advanceTimersByTimeAsync(600_000)
    expect(c.mock.remote.account.getNotifications).toHaveBeenCalledTimes(reads)
  } finally { vi.useRealTimers() }
}, 60_000)

it('reloads notifications without opening another account stream', async ({ start }) => {
  const c = await start()
  c.mock.remote.account.getNotifications.mockResolvedValue(ok(bonus('retained')))
  c.mock.streams.push('account/watch', { type: 'snapshot', state: stored })
  await c.flush()
  const stream = vi.spyOn(c.ctx.remote, '$stream')
  const before = c.mock.remote.account.getNotifications.mock.calls.length
  await c.reload('@deepseek-ai/dsh-client-ui-account-notifications')
  await c.flush()
  expect(stream).not.toHaveBeenCalled()
  expect(c.mock.remote.account.getNotifications).toHaveBeenCalledTimes(before + 1)
  expect(notificationOperations(c).hooks.notices.getSnapshot()[0]?.id).toBe('retained')
})

it('publishes notification changes through one stable source and releases its subscribers', async ({ start }) => {
  const c = await start()
  const source = notificationOperations(c).hooks.notices
  expect(notificationOperations(c).hooks.notices).toBe(source)
  const listener = vi.fn()
  const dispose = source.subscribe(listener)
  onTestFinished(dispose)
  c.mock.remote.account.getNotifications.mockResolvedValue(ok(bonus('first')))
  c.mock.streams.push('account/watch', { type: 'snapshot', state: stored })
  await c.flush()
  expect(source.getSnapshot()[0]?.id).toBe('first')
  expect(listener).toHaveBeenCalled()
  dispose()
  listener.mockClear()
  c.mock.streams.push('account/watch', { type: 'snapshot', state: view })
  await c.flush()
  expect(source.getSnapshot()).toEqual([])
  expect(listener).not.toHaveBeenCalled()
})
