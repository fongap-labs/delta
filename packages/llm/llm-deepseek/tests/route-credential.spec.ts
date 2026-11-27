/** The adapter refuses a request whose provider releases no credential, with the provider's diagnostic when it names one. */
import { afterEach, expect, it, vi } from 'vitest'
import { LlmError } from '@deepseek-ai/dsh-llm'
import { approvedDeepSeekAuthHeaders } from '../src/auth-headers.ts'
import { DeepSeekAdapter, resolveAdapterOptions } from '../src/index.ts'
import type { DeepSeekAdapterOptions } from '../src/index.ts'
import { chunks, options as request, prepareExtensions } from './helpers.ts'

afterEach(() => { vi.unstubAllGlobals() })

const connection = () => resolveAdapterOptions({ baseURL: 'https://route.deepseek.test/anthropic' })

/** A direct adapter without a credential, failing requests through `missingCredential` when given. */
function adapterWithout(missingCredential?: DeepSeekAdapterOptions['missingCredential']): DeepSeekAdapter {
  return new DeepSeekAdapter({
    options: connection,
    resolveAuth: () => Promise.resolve(undefined),
    prepareExtensions,
    ...missingCredential === undefined ? {} : { missingCredential },
  })
}

it('fails a request without a credential before any network I/O, with the default diagnostic', async () => {
  const fetchMock = vi.fn()
  vi.stubGlobal('fetch', fetchMock)
  await expect(chunks(adapterWithout().stream(request({ provider: 'fixture-route' })))).rejects.toMatchObject({
    code: 'MISSING_CREDENTIAL', message: 'llm-deepseek: no credential for provider route "fixture-route"',
  })
  expect(fetchMock).not.toHaveBeenCalled()
})

it('fails a request without a credential with the provider\'s own diagnostic', async () => {
  vi.stubGlobal('fetch', vi.fn())
  const missing = adapterWithout((_connection, provider) => new LlmError(`sign in for ${provider}`, 'ACCOUNT_SIGN_IN_REQUIRED'))
  await expect(chunks(missing.stream(request({ provider: 'fixture-route' })))).rejects.toMatchObject({
    code: 'ACCOUNT_SIGN_IN_REQUIRED', message: 'sign in for fixture-route',
  })
})

it('allows only required credential headers with normalized names', () => {
  expect(approvedDeepSeekAuthHeaders({ Authorization: 'Bearer fixture', 'X-API-Key': 'fixture', 'x-dsh-auth-token': 'fixture' })).toEqual({
    authorization: 'Bearer fixture', 'x-api-key': 'fixture', 'x-dsh-auth-token': 'fixture',
  })
})

it.each(['x-device-id', 'x-session-id', 'x-plugin-inventory', 'cookie', 'user-agent'])('rejects unapproved header %s', (name) => {
  expect(() => approvedDeepSeekAuthHeaders({ [name]: 'fixture' })).toThrow('Delta privacy policy')
})

it('rejects duplicate credentials and line breaks', () => {
  expect(() => approvedDeepSeekAuthHeaders({ Authorization: 'fixture', authorization: 'fixture' })).toThrow('Delta privacy policy')
  expect(() => approvedDeepSeekAuthHeaders({ authorization: 'fixture\r\nextra: fixture' })).toThrow('Delta privacy policy')
})
