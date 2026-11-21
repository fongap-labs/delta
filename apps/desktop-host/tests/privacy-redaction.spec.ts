import { expect, it } from 'vitest'
import { redactDiagnostic } from '../src/privacy-redaction.ts'

it('redacts credentials, account identifiers, network addresses, paths and URLs before IPC', () => {
  const value = redactDiagnostic([
    'Bearer fixture-secret-token',
    'token=opaque-token-value',
    'session_id=session-private-123',
    'account_id=account-private-123',
    'email=person@example.org',
    'path=C:\\Users\\Someone\\Documents\\secret.txt',
    'url=https://api.example.org/private?token=secret',
    'ip=192.0.2.44',
  ].join(' | '))
  for (const secret of ['fixture-secret-token','opaque-token-value','session-private-123','account-private-123',
    'person@example.org','Someone','api.example.org','192.0.2.44']) expect(value).not.toContain(secret)
  expect(value).toContain('[credential redacted]')
})
