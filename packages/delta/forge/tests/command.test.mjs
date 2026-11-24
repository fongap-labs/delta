import { test } from 'node:test'
import assert from 'node:assert/strict'
import { apply } from '../src/command.mjs'

function setup(config = {}) {
  let command
  const references = []
  let token = 'first-secret'
  const ctx = {
    credentials: { async resolve(ref) { references.push(ref); return token ? { value: token } : undefined } },
    commands: { register(value) { command = value; return () => {} } },
    effect(make) { const iterator = make(); iterator.next(); iterator.next() },
  }
  apply(ctx, config)
  return { command, references, setToken(value) { token = value } }
}

test('explicit commands resolve the latest credential without persisting input or returning secrets', async () => {
  const prior = globalThis.fetch
  const requests = []
  globalThis.fetch = async (...args) => { requests.push(args); return Response.json({ title: 'Public title', state: 'open', body: 'Details', head: { ref: 'topic', sha: 'abc' }, base: { ref: 'main' } }) }
  try {
    const fixture = setup()
    assert.equal(fixture.command.recordInput, false)
    assert.equal(fixture.command.definitionId, '@delta/forge')
    const invoke = rawInput => fixture.command.handler({ rawInput, signal: new AbortController().signal })
    const first = await invoke('github owner/repo 12')
    fixture.setToken('rotated-secret')
    const second = await invoke('github owner/repo 13')
    assert.deepEqual(fixture.references, ['DELTA_GITHUB_TOKEN', 'DELTA_GITHUB_TOKEN'])
    assert.equal(requests[0][1].headers.authorization, 'Bearer first-secret')
    assert.equal(requests[1][1].headers.authorization, 'Bearer rotated-secret')
    assert.equal(first.kind, 'success')
    assert.equal(second.kind, 'success')
    assert.doesNotMatch(JSON.stringify([first, second]), /secret/)
    assert.equal((await invoke('github owner/repo pasted-secret')).kind, 'error')
    assert.equal(requests.length, 2)
  } finally { globalThis.fetch = prior }
})

test('a custom origin does not implicitly receive a public-service token', async () => {
  const prior = globalThis.fetch
  let headers
  globalThis.fetch = async (_url, options) => { headers = options.headers; return Response.json({ title: 'Title', state: 'opened', sha: 'abc', source_branch: 'x', target_branch: 'y' }) }
  try {
    const fixture = setup({ gitlab: { origin: 'https://git.example.com' } })
    assert.equal((await fixture.command.handler({ rawInput: 'gitlab group/repo 1', signal: new AbortController().signal })).kind, 'success')
    assert.equal(headers.authorization, undefined)
    assert.deepEqual(fixture.references, [])
    const bound = setup({ gitlab: { origin: 'https://git.example.com', tokenRef: 'PRIVATE_GITLAB_TOKEN' } })
    await bound.command.handler({ rawInput: 'gitlab group/repo 1', signal: new AbortController().signal })
    assert.deepEqual(bound.references, ['PRIVATE_GITLAB_TOKEN'])
    assert.equal(headers.authorization, 'Bearer first-secret')
  } finally { globalThis.fetch = prior }
})

test('request failures never propagate credential-bearing errors into the result', async () => {
  const prior = globalThis.fetch
  globalThis.fetch = async () => { throw new Error('first-secret: simulated transport error') }
  try {
    const fixture = setup()
    const result = await fixture.command.handler({ rawInput: 'github owner/repo 1', signal: new AbortController().signal })
    assert.equal(result.kind, 'error')
    assert.doesNotMatch(result.text, /secret/)
  } finally { globalThis.fetch = prior }
})
