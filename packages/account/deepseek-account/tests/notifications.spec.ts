/** Account notification reads over the Host HTTP path; no interaction-event egress. */
import { afterEach, expect, it, vi } from 'vitest'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http'
import { Context } from '@deepseek-ai/cordis'
import AuthorizationService from '@deepseek-ai/dsh-authorization'
import { LocalCredentialProvider } from '@deepseek-ai/dsh-credentials-local'
import { credentialKey } from '@deepseek-ai/dsh-credentials'
import type { AccountNotificationBatch, AccountNotificationId, AccountClientMetadata } from '@deepseek-ai/dsh-deepseek-account'
import { DeepSeekAccount } from '../src/index.ts'

const cleanups: Array<() => Promise<unknown>> = []
afterEach(async () => { while (cleanups.length) await cleanups.pop()!() })

const ORDER = '4c1b0000-0000-4000-8000-000000000000'
const SECOND_ORDER = '5d2c0000-0000-4000-8000-000000000000'
const USER = 'test-user'

/** Client identity supplied with each account operation; only the locale's primary subtag selects the wire locale. */
const clientMetadata = (locale = 'en-US'): AccountClientMetadata => ({ version: '1.2.3', locale, timezoneOffsetSeconds: 28_800 })

interface Recorded {
  readonly method: string | undefined
  readonly url: string | undefined
  readonly token: string | undefined
  readonly locale: string | undefined
  readonly bundleId: string | undefined
  readonly platform: string | undefined
  readonly version: string | undefined
  readonly timezoneOffset: string | undefined
  readonly contentType: string | undefined
  readonly body: string | undefined
}

/** Read one request body; the acknowledgement carries its order id in JSON, so the fixture keeps the exact bytes. */
function requestBody(req: IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    let body = ''
    req.setEncoding('utf8')
    req.on('data', (chunk: string) => { body += chunk })
    req.on('error', reject)
    req.on('end', () => { resolve(body) })
  })
}

/** @param id - opaque notification ID. @param fields - wire fields overridden by the scenario. @returns a complete notification. */
function wire(id: string, fields: Record<string, unknown> = {}) {
  return { id, created_at: '2026-10-21T08:00:00Z', display_until: null,
    title: 'Product news', body: 'New features are available.', actions: [], ...fields }
}

async function fixture() {
  const home = await mkdtemp(join(tmpdir(), 'dsh-bonus-'))
  cleanups.push(() => rm(home, { recursive: true, force: true }))
  const requests: Recorded[] = []
  let profileId: string | null = USER
  let profileBizCode = 0
  let profileStatus = 200
  let bonusBizCode = 0
  let bonusStatus = 200
  let nextPollAfter: unknown = 300
  let bonuses: unknown = [wire(ORDER), wire(SECOND_ORDER)]
  let content: unknown = wire(ORDER)
  let contentStatus = 200
  let contentBizCode = 0
  let beforeListResponse: (() => Promise<void>) | undefined
  let beforeContentResponse: (() => Promise<void>) | undefined
  let beforeProfileResponse: (() => Promise<void>) | undefined
  let origin = ''
  const handle = async (req: IncomingMessage, res: ServerResponse): Promise<void> => {
    expect(req.headers.authorization).toBeUndefined()
    const body = await requestBody(req)
    requests.push({ method: req.method, url: req.url, token: req.headers['x-dsh-auth-token'] as string | undefined,
      locale: req.headers['x-client-locale'] as string | undefined,
      bundleId: req.headers['x-client-bundle-id'] as string | undefined,
      platform: req.headers['x-client-platform'] as string | undefined,
      version: req.headers['x-client-version'] as string | undefined,
      timezoneOffset: req.headers['x-client-timezone-offset'] as string | undefined,
      contentType: req.headers['content-type'], body: body === '' ? undefined : body })
    res.setHeader('content-type', 'application/json')
    if (req.url === '/auth-api/v0/users/current') {
      await beforeProfileResponse?.()
      res.writeHead(profileStatus).end(JSON.stringify({ code: profileStatus === 200 ? 0 : 1, data: { biz_code: profileBizCode,
        biz_data: { id: profileId, email: 't***@example.invalid', id_profile: { name: 'Test', picture: null } } } }))
      return
    }
    if (req.url === '/api/v0/users/notifications/list') {
      await beforeListResponse?.()
      res.writeHead(bonusStatus).end(JSON.stringify({ code: bonusStatus === 200 ? 0 : 1,
        data: { biz_code: bonusBizCode, biz_data: { next_poll_after: nextPollAfter, notifications: bonuses } } }))
      return
    }
    if (req.url?.startsWith('/api/v0/users/notifications/content?')) {
      await beforeContentResponse?.()
      res.writeHead(contentStatus).end(JSON.stringify({ code: contentStatus === 200 ? 0 : 1,
        data: { biz_code: contentBizCode, biz_data: content } }))
      return
    }
    res.writeHead(404).end('{}')
  }
  const server = createServer((req, res) => { void handle(req, res).catch(() => { res.writeHead(500).end('{}') }) })
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve))
  const address = server.address()
  if (!address || typeof address === 'string') throw new Error('missing listener')
  origin = `http://127.0.0.1:${address.port}`
  cleanups.push(async () => {
    await new Promise<void>((resolve) => { server.close(() => { resolve() }); server.closeAllConnections() })
  })
  const ctx = new Context()
  const credentials = ctx.plugin(LocalCredentialProvider, { path: join(home, 'credentials.yaml'), watch: false })
  await credentials
  const authorization = ctx.plugin(AuthorizationService)
  await authorization
  // The composed client identity is this fixture's choice; `null` is the web profile's documented setting.
  const provider = ctx.plugin(DeepSeekAccount, {
    platformOrigin: origin, harnessOrigin: origin, allowLoopbackHttp: true, requestTimeoutMs: 5_000, desktopPlatform: null,
  })
  await provider
  cleanups.push(async () => { await provider.dispose(); await authorization.dispose(); await credentials.dispose() })
  const key = credentialKey('deepseek-account-platform', 'default')
  return {
    account: ctx.deepseekAccount, ctx, requests, key,
    grant: async (token: string) => {
      await ctx.credentials.modifyRecord(key, () => Promise.resolve({
        kind: 'grant', payload: { version: 1, token, issuer: origin },
      }))
    },
    signOut: async () => {
      await ctx.credentials.deleteRecord(key)
    },
    profileId: (value: string | null) => { profileId = value },
    failProfile: (bizCode = 17, status = 200) => { profileBizCode = bizCode; profileStatus = status },
    nextPollAfter: (value: unknown) => { nextPollAfter = value },
    notifications: (value: unknown) => { bonuses = value },
    content: (value: unknown) => { content = value },
    failContent: (bizCode = 0, status = 200) => { contentBizCode = bizCode; contentStatus = status },
    onContent: (hook: () => Promise<void>) => { beforeContentResponse = hook },
    failBonuses: (bizCode = 17, status = 200) => { bonusBizCode = bizCode; bonusStatus = status },
    onList: (hook: () => Promise<void>) => { beforeListResponse = hook },
    onProfile: (hook: () => Promise<void>) => { beforeProfileResponse = hook },
  }
}

it('does not reuse a cached identity from another grant', async () => {
  const f = await fixture()
  await f.grant('test-account-token')
  expect(await f.account.getProfile(clientMetadata())).toMatchObject({ status: 'ready' })
  await f.grant('rotated-token')
  await f.account.getNotifications(clientMetadata('en'))
  expect(f.requests.map(request => [request.url, request.token])).toEqual([
    ['/auth-api/v0/users/current', 'test-account-token'],
    ['/api/v0/users/notifications/list', 'rotated-token'],
  ])
})

it('leaves profile identity unknown when only notifications were queried', async () => {
  const f = await fixture()
  await f.grant('test-account-token')
  await f.account.getNotifications(clientMetadata())
  expect(await f.account.getPlatformSession()).toMatchObject({ userId: null, token: 'test-account-token' })
  expect(f.requests.map(request => request.url)).toEqual(['/api/v0/users/notifications/list'])
})

it.each([undefined, null, 0, -1, '300', 2_147_483.648])('rejects an invalid next_poll_after value: %s', async (delay) => {
  const f = await fixture()
  await f.grant('test-account-token')
  f.nextPollAfter(delay)
  await expect(f.account.getNotifications(clientMetadata())).rejects.toMatchObject({ code: 'protocol' })
})

it.each([0.5, 300, 2_147_483.647])('preserves the server polling delay in seconds: %s', async (delay) => {
  const f = await fixture()
  await f.grant('test-account-token')
  f.nextPollAfter(delay)
  expect((await f.account.getNotifications(clientMetadata()))?.nextPollAfterSeconds).toBe(delay)
})

it('reads generic notifications in server order with account and client headers', async () => {
  const f = await fixture()
  await f.grant('test-account-token')
  const result = await f.account.getNotifications(clientMetadata('zh-CN'))
  expect(result).toEqual<AccountNotificationBatch>({ nextPollAfterSeconds: 300, notifications: [
    { id: ORDER as AccountNotificationId, createdAt: '2026-10-21T08:00:00Z',
      displayUntil: null, title: 'Product news', body: 'New features are available.', actions: [] },
    { id: SECOND_ORDER as AccountNotificationId, createdAt: '2026-10-21T08:00:00Z',
      displayUntil: null, title: 'Product news', body: 'New features are available.', actions: [] },
  ] })
  expect(f.requests).toEqual([
    { method: 'GET', url: '/api/v0/users/notifications/list', token: 'test-account-token', locale: 'zh_CN',
      bundleId: '', platform: 'web', version: '1.2.3', timezoneOffset: '28800' },
  ])
})

it.each(['list', 'content'] as const)('accepts a %s notification without a business classification', async (operation) => {
  const f = await fixture()
  await f.grant('test-account-token')
  const value = wire(ORDER)
  f.notifications([value])
  f.content(value)
  const notification = operation === 'list' ? (await f.account.getNotifications(clientMetadata()))?.notifications[0]
    : await f.account.getNotificationContent(ORDER as AccountNotificationId, clientMetadata())
  expect(notification).toMatchObject({ id: ORDER, title: 'Product news', body: 'New features are available.', actions: [] })
  expect(notification).not.toHaveProperty('type')
})

it('keeps action targets and business expiry separate from the display deadline', async () => {
  const f = await fixture()
  await f.grant('test-account-token')
  const actions = [{ label: 'View balance', style: 'primary', target: { type: 'app_route', name: 'account' } },
    { label: 'Got it', style: 'secondary', target: { type: 'dismiss' } }]
  const data = { order_id: 'business-order', expires_at: '2026-11-21T08:00:00Z', amount: '5.00' }
  f.notifications([wire(ORDER, { display_until: '2026-10-28T08:00:00Z', actions, data })])
  expect((await f.account.getNotifications(clientMetadata()))?.notifications).toEqual([
    { id: ORDER, createdAt: '2026-10-21T08:00:00Z', displayUntil: '2026-10-28T08:00:00Z',
      title: 'Product news', body: 'New features are available.', actions, data },
  ])
})

it.each([
  ['zh-CN', 'zh_CN'],
  ['en-US', 'en_US'],
])('selects %s copy through the locale header and never a language query', async (locale, wireLocale) => {
  const f = await fixture()
  await f.grant('test-account-token')
  await f.account.getNotifications(clientMetadata(locale))
  expect(f.requests.map(request => request.locale)).toEqual([wireLocale])
  expect(f.requests.map(request => request.url)).toEqual([
    '/api/v0/users/notifications/list',
  ])
})

it.each([
  ['a non-string title', [wire(ORDER, { title: 42 })]],
  ['a null title', [wire(ORDER, { title: null })]],
  ['a non-string ID', [wire(ORDER, { id: 42 })]],
  ['an invalid display deadline', [wire(ORDER, { display_until: '' })]],
  ['an invalid creation time', [wire(ORDER, { created_at: 'yesterday' })]],
  ['a missing body', [wire(ORDER, { body: undefined })]],
  ['a non-array actions field', [wire(ORDER, { actions: null })]],
  ['an unsupported action style', [wire(ORDER, { actions: [{ label: 'Open', style: 'danger', target: { type: 'dismiss' } }] })]],
  ['an unsupported route', [wire(ORDER, { actions: [{ label: 'Open', style: 'primary', target: { type: 'app_route', name: 'other' } }] })]],
  ['an unsupported target', [wire(ORDER, { actions: [{ label: 'Open', style: 'primary', target: { type: 'url', url: 'https://example.invalid' } }] })]],
])('skips malformed or unsupported notifications without losing valid neighbors: %s', async (_name, payload) => {
  const f = await fixture()
  await f.grant('test-account-token')
  const log = vi.spyOn(console, 'error').mockImplementation(() => {})
  try {
    f.notifications([wire('before'), ...payload, wire('after')])
    const result = await f.account.getNotifications(clientMetadata())
    expect(result?.notifications.map(value => value.id)).toEqual(['before', 'after'])
    expect(result?.nextPollAfterSeconds).toBe(300)
    expect(log).toHaveBeenCalledOnce()
    expect(log.mock.calls[0]?.[0]).toBe('[deepseek-account] skipping unsupported notification')
    expect(log.mock.calls[0]?.[1]).toMatchObject({ index: 1 })
  } finally { log.mockRestore() }
})

it('returns an empty batch when every notification is unsupported', async () => {
  const f = await fixture()
  await f.grant('test-account-token')
  const log = vi.spyOn(console, 'error').mockImplementation(() => {})
  try {
    f.notifications([wire(ORDER, { actions: [{ label: 'Open', style: 'primary', target: { type: 'future' } }] })])
    expect(await f.account.getNotifications(clientMetadata())).toEqual({ notifications: [], nextPollAfterSeconds: 300 })
  } finally { log.mockRestore() }
})

it('rejects a notification payload that is not a list', async () => {
  const f = await fixture()
  await f.grant('test-account-token')
  f.notifications({})
  await expect(f.account.getNotifications(clientMetadata())).rejects.toThrow('account: protocol')
})

it('streams credential changes with current state and leaves profile reads silent', async () => {
  const f = await fixture()
  const lifetime = new AbortController()
  const iterator = f.account.watch(lifetime.signal)[Symbol.asyncIterator]()
  try {
    expect(await iterator.next()).toMatchObject({ value: { type: 'snapshot', state: { status: 'signed-out' } } })
    await f.grant('first-token')
    expect(await iterator.next()).toMatchObject({ value: { type: 'credentials-changed', state: { status: 'credential-stored' } } })
    await f.account.getProfile(clientMetadata())
    const next = iterator.next()
    await f.grant('second-token')
    expect(await next).toMatchObject({ value: { type: 'credentials-changed', state: { status: 'credential-stored' } } })
    await f.signOut()
    expect(await iterator.next()).toMatchObject({ value: { type: 'credentials-changed', state: { status: 'signed-out' } } })
  } finally {
    lifetime.abort()
    await iterator.return?.()
  }
})

it('keeps a queued credential change ahead of a later profile snapshot', async () => {
  const f = await fixture()
  const lifetime = new AbortController()
  const iterator = f.account.watch(lifetime.signal, true)[Symbol.asyncIterator]()
  try {
    expect(await iterator.next()).toMatchObject({ value: { type: 'snapshot' } })
    await f.grant('first-token')
    await f.account.getProfile(clientMetadata())
    expect(await iterator.next()).toMatchObject({ value: { type: 'credentials-changed', state: { status: 'credential-stored' } } })
  } finally {
    lifetime.abort()
    await iterator.return?.()
  }
})

it.each(['replacement', 'abort'] as const)('discards an opening snapshot invalidated by %s during the read', async (change) => {
  const f = await fixture()
  const lifetime = new AbortController()
  const reading = Promise.withResolvers<undefined>()
  const release = Promise.withResolvers<undefined>()
  const readState = f.account.getState.bind(f.account)
  vi.spyOn(f.account, 'getState').mockImplementationOnce(async () => {
    const state = await readState()
    reading.resolve(undefined)
    await release.promise
    return state
  })
  const iterator = f.account.watch(lifetime.signal)[Symbol.asyncIterator]()
  const first = iterator.next()
  try {
    await reading.promise
    if (change === 'replacement') await f.grant('replacement-token')
    else lifetime.abort()
    release.resolve(undefined)
    if (change === 'replacement') {
      expect(await first).toMatchObject({ value: { type: 'credentials-changed', state: { status: 'credential-stored' } } })
    } else expect(await first).toMatchObject({ done: true })
  } finally {
    release.resolve(undefined)
    lifetime.abort()
    await first
    await iterator.return?.()
  }
})

it('discards a list response invalidated by a credential change', async () => {
  const f = await fixture()
  await f.grant('test-account-token')
  f.onList(async () => { await f.grant('rotated-token') })
  expect(await f.account.getNotifications(clientMetadata())).toBeNull()
  expect(f.requests.map(request => [request.url, request.token])).toEqual([
    ['/api/v0/users/notifications/list', 'test-account-token'],
  ])
})

it('reads localized content of a consumed notification without listing or acknowledging it', async () => {
  const f = await fixture()
  await f.grant('test-account-token')
  f.notifications([])
  f.content(wire(ORDER, { title: '产品公告', body: '新功能已上线', actions: [
    { label: '知道了', style: 'secondary', target: { type: 'dismiss' } },
  ] }))
  const content = await f.account.getNotificationContent(ORDER as AccountNotificationId, clientMetadata('zh'))
  expect(content).toMatchObject({ id: ORDER, title: '产品公告', body: '新功能已上线',
    actions: [{ label: '知道了', style: 'secondary', target: { type: 'dismiss' } }] })
  expect(f.requests.map(request => request.url)).toEqual([
    `/api/v0/users/notifications/content?id=${ORDER}`,
  ])
  expect(f.requests.at(-1)).toMatchObject({ method: 'GET', token: 'test-account-token', locale: 'zh_CN' })
})

it('does not query notification content while signed out', async () => {
  const f = await fixture()
  expect(await f.account.getNotificationContent(ORDER as AccountNotificationId, clientMetadata())).toBeNull()
  expect(f.requests).toEqual([])
})

it.each([
  ['a mismatched ID', wire(SECOND_ORDER)],
  ['malformed content', { title: 'private-content' }],
])('rejects %s returned by the content endpoint', async (_name, content) => {
  const f = await fixture()
  await f.grant('test-account-token')
  f.content(content)
  await expect(f.account.getNotificationContent(ORDER as AccountNotificationId, clientMetadata()))
    .rejects.toThrow('account: protocol')
  expect(await f.account.getState()).toMatchObject({ status: 'credential-stored' })
})

it.each([401, 500])('handles a content HTTP %s without discarding a usable grant', async (status) => {
  const f = await fixture()
  await f.grant('test-account-token')
  f.failContent(0, status)
  const result = f.account.getNotificationContent(ORDER as AccountNotificationId, clientMetadata())
  if (status === 401) {
    expect(await result).toBeNull()
    expect(await f.account.getState()).toMatchObject({ status: 'signed-out' })
  } else {
    await expect(result).rejects.toThrow('account: network')
    expect(await f.account.getState()).toMatchObject({ status: 'credential-stored' })
  }
})

it('discards content when the credential changes before its response', async () => {
  const f = await fixture()
  await f.grant('test-account-token')
  f.onContent(async () => { await f.grant('replacement-token') })
  expect(await f.account.getNotificationContent(ORDER as AccountNotificationId, clientMetadata())).toBeNull()
  expect(await f.ctx.credentials.readRecord(f.key)).toMatchObject({ payload: { token: 'replacement-token' } })
})


it('never creates a notification interaction reporting capability', async () => {
  const f = await fixture()
  await f.grant('test-account-token')
  await f.account.getNotifications(clientMetadata())
  expect('reportNotificationEvent' in f.account).toBe(false)
  expect(f.requests.map(request => request.url)).toEqual(['/api/v0/users/notifications/list'])
})
