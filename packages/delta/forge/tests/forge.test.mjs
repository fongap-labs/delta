import { test } from 'node:test'
import assert from 'node:assert/strict'
import { ForgeClient } from '../src/index.mjs'

const response = value => new Response(JSON.stringify(value), { status: 200 })

test('reads a GitHub pull request with one credential-bound HTTPS request', async () => {
  const calls = []
  const client = new ForgeClient('github', { fetchImpl: async (...args) => {
    calls.push(args)
    return response({ title: 'Fix', state: 'open', body: '用户描述', head: { ref: 'feature', sha: 'abc' }, base: { ref: 'main' } })
  } })
  const change = await client.readChange('owner/project', 15, { token: 'secret-token' })
  assert.equal(String(calls[0][0]), 'https://api.github.com/repos/owner/project/pulls/15')
  assert.equal(calls[0][1].headers.authorization, 'Bearer secret-token')
  assert.equal(calls[0][1].redirect, 'manual')
  assert.equal(calls[0][1].credentials, 'omit')
  assert.deepEqual(change, { provider: 'github', project: 'owner/project', number: 15, title: 'Fix', state: 'open', sourceBranch: 'feature', targetBranch: 'main', headSha: 'abc', description: '用户描述' })
  assert.ok(!JSON.stringify(change).includes('secret-token'))
})

test('reads a nested GitLab project from a configured HTTPS origin', async () => {
  let request
  const client = new ForgeClient('gitlab', { origin: 'https://git.example.com/prefix', fetchImpl: async (...args) => {
    request = args
    return response({ title: 'Merge', state: 'opened', source_branch: 'feature', target_branch: 'main', sha: 'def', description: null })
  } })
  const change = await client.readChange('group/sub/project', 7)
  assert.equal(String(request[0]), 'https://git.example.com/prefix/api/v4/projects/group%2Fsub%2Fproject/merge_requests/7')
  assert.equal(request[1].headers.authorization, undefined)
  assert.equal(change.description, '')
  assert.equal(change.headSha, 'def')
})

test('rejects redirects, oversized content, malformed responses and unsafe input', async () => {
  assert.throws(() => new ForgeClient('toString'), /Unsupported/)
  assert.throws(() => new ForgeClient('gitlab', { origin: 'http://example.com' }), /HTTPS/)
  assert.throws(() => new ForgeClient('github', { origin: 'https://token@example.com' }), /HTTPS/)
  let calls = 0
  const client = new ForgeClient('github', { fetchImpl: async () => { calls++; return new Response('', { status: 302, headers: { location: 'https://other.example/' } }) } })
  await assert.rejects(client.readChange('owner/../project', 1), /Invalid repository/)
  await assert.rejects(client.readChange('owner/project', 0), /Invalid change/)
  await assert.rejects(client.readChange('owner/project', 1, { token: 'one\r\ntwo' }), /Invalid Forge credential/)
  assert.equal(calls, 0)
  await assert.rejects(client.readChange('owner/project', 1, { signal: AbortSignal.abort(new Error('cancelled')) }), /cancelled/)
  assert.equal(calls, 0)
  await assert.rejects(client.readChange('owner/project', 1, { token: 'secret' }), /redirects are not followed/)
  assert.equal(calls, 1)
  const big = new ForgeClient('gitlab', { fetchImpl: async () => new Response('x'.repeat(1024 * 1024 + 1)) })
  await assert.rejects(big.readChange('group/project', 1), /size limit/)
  const malformed = new ForgeClient('gitlab', { fetchImpl: async () => response({ title: 'incomplete' }) })
  await assert.rejects(malformed.readChange('group/project', 1), /missing state/)
})

test('does not expose response bodies or credentials in HTTP errors', async () => {
  const client = new ForgeClient('github', { fetchImpl: async () => new Response('secret-token', { status: 401 }) })
  await assert.rejects(client.readChange('owner/project', 2, { token: 'secret-token' }), error => {
    assert.match(error.message, /HTTP 401/)
    assert.doesNotMatch(error.message, /secret-token/)
    return true
  })
})

test('bounds externally supplied fields before they enter the session or model context', async () => {
  const client = new ForgeClient('github', { fetchImpl: async () => response({
    title: 't'.repeat(2000), state: 'open', body: 'd'.repeat(13000),
    head: { ref: 's'.repeat(1200), sha: 'a'.repeat(200) }, base: { ref: 'target', sha: 'base' },
  }) })
  const change = await client.readChange('owner/repo', 1)
  assert.equal(change.title.length, 1000)
  assert.equal(change.description.length, 12000)
  assert.equal(change.sourceBranch.length, 1000)
  assert.equal(change.headSha.length, 128)
})
