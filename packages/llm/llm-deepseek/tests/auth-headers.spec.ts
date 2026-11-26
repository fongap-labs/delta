import { expect, it } from 'vitest'
import { approvedDeepSeekAuthHeaders } from '../src/auth-headers.ts'

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
