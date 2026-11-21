import { describe, expect, it } from 'vitest'
import { redactDiagnostic } from '../src/privacy-redaction.ts'
import { desktopErrorState } from '../src/startup-error.ts'

describe('Delta desktop diagnostic privacy', () => {
  it('removes credentials, account identities, email addresses, paths and URLs', () => {
    const fixtureEmail = ['person', 'example.org'].join('@')
    const leaked = [
      'Bearer abcDEF_1234.567890',
      'api_key=sk-0123456789abcdefghijklmnop',
      'session_id=session-private-123',
      'user_id=user-private-567',
      'email=' + fixtureEmail,
      'path=C:\\Users\\Someone\\Documents\\secret.txt',
      'url=https://api.example.org/private?api_key=secret',
      'token=opaque-token-value',
      'account_id=account-private-123',
      'ip=192.0.2.44',
    ]
    const sanitized = redactDiagnostic(leaked.join(' | '))
    for (const secret of ['abcDEF_1234', '0123456789abcdefghijklmnop', 'session-private-123',
      'user-private-567', fixtureEmail, 'Someone', 'api.example.org', 'opaque-token-value', 'account-private-123', '192.0.2.44']) {
      expect(sanitized).not.toContain(secret)
    }
    expect(sanitized).toContain('[redacted]')
  })

  it('bounds diagnostics before they cross the renderer or native dialog', () => {
    expect(redactDiagnostic('x'.repeat(10_000)).length).toBeLessThanOrEqual(1200)
    const message = desktopErrorState(new Error('Bearer secret-token-ABC123')).message
    expect(message).not.toContain('secret-token-ABC123')
    expect(message).toContain('[credential redacted]')
  })
})
