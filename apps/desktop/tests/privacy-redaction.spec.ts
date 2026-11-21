import { expect, it } from 'vitest'
import { redactDiagnostic as redactDesktop } from '../src/privacy-redaction.ts'
import { redactDiagnostic as redactHost } from '../../desktop-host/src/privacy-redaction.ts'

it('redacts credentials, account identifiers, network addresses, paths and URLs before IPC', () => {
  const fixtureEmail = ['person', 'example.org'].join('@')
  for (const redactDiagnostic of [redactDesktop, redactHost]) {
    const value = redactDiagnostic([
    'Bearer fixture-secret-token',
    'token=opaque-token-value',
    'session_id=session-private-123',
    'account_id=account-private-123',
    'email=' + fixtureEmail,
    'path=C:\\Users\\Someone\\Documents\\secret.txt',
    'url=https://api.example.org/private?token=secret',
    'ip=192.0.2.44',
  ].join(' | '))
  for (const secret of ['fixture-secret-token','opaque-token-value','session-private-123','account-private-123',
    fixtureEmail,'Someone','api.example.org','192.0.2.44']) expect(value).not.toContain(secret)
    expect(value).toContain('[credential redacted]')
  }
})
