/** Notification destinations are independent of Platform account and browser destinations. */
import { mkdtemp, rm } from 'node:fs/promises'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { afterEach, expect, it, vi } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import AuthorizationService from '@deepseek-ai/dsh-authorization'
import { LocalCredentialProvider } from '@deepseek-ai/dsh-credentials-local'
import { credentialKey } from '@deepseek-ai/dsh-credentials'
import { DeepSeekAccount, type Config } from '../src/index.ts'

const cleanups: Array<() => Promise<unknown>> = []
afterEach(async () => {
  while (cleanups.length) await cleanups.pop()!()
  vi.restoreAllMocks()
})

async function fixture(issuer: string, config: Config = {}) {
  const home = await mkdtemp(join(tmpdir(), 'dsh-account-origin-'))
  cleanups.push(() => rm(home, { recursive: true, force: true }))
  const ctx = new Context()
  cleanups.push(() => ctx.fiber.dispose())
  await ctx.plugin(LocalCredentialProvider, { path: join(home, 'credentials.yaml'), watch: false })
  await ctx.plugin(AuthorizationService)
  await ctx.credentials.modifyRecord(credentialKey('deepseek-account-platform', 'default'), () => Promise.resolve({
    kind: 'grant', payload: { version: 1, token: 'fixture-token', issuer },
  }))
  await ctx.plugin(DeepSeekAccount, config)
  return ctx.deepseekAccount
}

it('keeps private grants and embedded pages on Platform while allowing a notification origin override', async () => {
  const rejected = await fixture('https://platform.deepseek.com', { platformOrigin: 'https://private.example' })
  expect(await rejected.getState()).toMatchObject({ status: 'signed-out' })
  const account = await fixture('https://private.example', {
    platformOrigin: 'https://private.example', harnessOrigin: 'https://notifications.example',
    requestHeaders: { 'x-deployment': 'base' }, accountRequestHeaders: { 'x-account': 'account' },
  })
  expect(await account.getPlatformSession()).toMatchObject({
    origin: 'https://private.example', token: 'fixture-token',
    requestHeaders: { 'x-deployment': 'base', 'x-account': 'account' },
  })
  const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
    expect(input).toBe('https://notifications.example/api/v0/users/notifications/list')
    expect(init?.headers).toMatchObject({ 'x-deployment': 'base', 'x-account': 'account', 'x-dsh-auth-token': 'fixture-token' })
    return new Response(JSON.stringify({ code: 0, data: { biz_code: 0, biz_data: { notifications: [], next_poll_after: 600 } } }))
  })
  await account.getNotifications({ version: 'test', locale: 'en', timezoneOffsetSeconds: 0 })
  expect(fetcher).toHaveBeenCalledOnce()
})

it.each(['http://remote.example', 'https://notifications.example/path'])(
  'rejects the invalid notification origin %s during initialization', async (harnessOrigin) => {
    await expect(fixture('https://platform.deepseek.com', { harnessOrigin })).rejects.toThrow('harnessOrigin')
  },
)
