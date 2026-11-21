/** External GitHub HTTP and credential boundary. Repository rules belong to the feature. */
import { GitHubApiFailure, GitHubWriteUncertain } from '../../contracts/github.js'
import type { DeltaGitHubRequest, DeltaGitHubResponse, DeltaGitHubTransport, DeltaGitHubJobLogsRequest, DeltaGitHubJobLogsResponse } from '../../contracts/github.js'
import type { GitHubAccountConfig } from '../../../types.js'

const API = 'https://api.github.com'

/** The Host credentials service is authoritative whenever it is available.
 * No credential is cached or copied into Delta settings or model-visible state. */
interface GitHubCredentialHost {
  get(name: 'credentials'): unknown
}

async function credentialForRequest(host: GitHubCredentialHost | undefined, ref: string, account: string): Promise<string> {
  const provider = host?.get('credentials') as { resolve?: (key: string) => Promise<{ value?: string } | undefined> } | undefined
  if (provider !== undefined) {
    if (typeof provider.resolve !== 'function') throw new Error('GitHub Host credential service is incomplete')
    const resolved = await provider.resolve(ref)
    if (typeof resolved?.value !== 'string' || !resolved.value.trim()) {
      throw new Error('GitHub credential not configured for account ' + account)
    }
    return resolved.value.trim()
  }
  // Compatibility with older Hosts lacking the credentials service.
  // This branch cannot override a mounted-but-empty authoritative provider.
  const ambient = process.env[ref]
  if (!ambient?.trim()) throw new Error('GitHub credential not configured for account ' + account)
  return ambient.trim()
}

/** Native credentials are account-scoped; organizations use explicit owner bindings. */
async function assertNativeGitHubOwner(
  account: string, repository: string, token: string, signal: AbortSignal,
): Promise<void> {
  if (account !== 'native') return
  let identity: Response
  try {
    identity = await fetch(API + '/user', {
      method: 'GET',
      headers: {
        Authorization: 'Bearer ' + token,
        Accept: 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28',
        'User-Agent': 'dsh-delta-code',
      },
      signal,
      redirect: 'error',
    })
  } catch (error) {
    throw new Error('native GitHub credential identity could not be verified', { cause: error })
  }
  if (!identity.ok) throw new GitHubApiFailure(identity.status, repository, 'GET')
  const login = (await identity.json())?.login
  if (typeof login !== 'string' || login.toLowerCase() !== repository.split('/')[0]!.toLowerCase()) {
    throw new Error('native GITHUB_TOKEN owner does not match repository owner; configure a dedicated account or organization credential')
  }
}

async function requestGitHubApi(input: DeltaGitHubRequest, tokenEnv: string, host?: GitHubCredentialHost): Promise<DeltaGitHubResponse> {
  const { repository, account, method, path, data, signal } = input
  const token = await credentialForRequest(host, tokenEnv, account)
  await assertNativeGitHubOwner(account, repository, token, signal)
  const url = API + path
  if (!url.startsWith(API + '/') || !new URL(url).hostname.endsWith('github.com')) {
    throw new Error('invalid GitHub API URL')
  }
  let response: Response
  try {
    response = await fetch(url, {
      method,
      headers: {
        Authorization: 'Bearer ' + token,
        Accept: 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28',
        'User-Agent': 'dsh-delta-code',
        ...(data === undefined ? {} : { 'Content-Type': 'application/json' }),
      },
      ...(data === undefined ? {} : { body: JSON.stringify(data) }),
      signal,
      redirect: 'error',
    })
  } catch (error) {
    if (method !== 'GET') throw new GitHubWriteUncertain(method, repository)
    throw error
  }
  if (!response.ok) throw new GitHubApiFailure(response.status, repository, method)
  return { repository, account, data: response.status === 204 ? null : await response.json() }
}

/**
 * Explicitly public-only, bounded discovery channel. Unlike requestGitHubApi,
 * this endpoint NEVER resolves, forwards, or falls back to any GitHub Token.
 * GitHub's REST code search requires authentication; use public repository
 * search, then verify SKILL.md paths in pinned public Git trees instead.
 */
async function publicGitHubMetadata(path: string, signal: AbortSignal): Promise<unknown> {
  if (typeof path !== 'string' || !path.startsWith('/')) throw new Error('invalid public GitHub discovery path')
  const endpoint = new URL(API + path)
  if (endpoint.origin !== API || endpoint.username || endpoint.password || endpoint.hash) {
    throw new Error('invalid public GitHub discovery URL')
  }
  const query = endpoint.searchParams
  const searchQuery = query.get('q')
  const searchSuffix = ' skills in:name,description,readme'
  const searchTerms = searchQuery?.endsWith(searchSuffix)
    ? searchQuery.slice(0, -searchSuffix.length) : ''
  const isSearch = endpoint.pathname === '/search/repositories' &&
    query.size === 2 && query.get('per_page') === '8' &&
    /^[\p{L}\p{N}_ .+\-]{2,80}$/u.test(searchTerms)
  const isOwner = /^\/users\/[A-Za-z0-9-]{1,39}$/.test(endpoint.pathname) && !endpoint.search
  const isCommit = /^\/repos\/[A-Za-z0-9-]{1,39}\/[A-Za-z0-9_.-]{1,100}\/commits\/[A-Za-z0-9._%-]{1,200}$/.test(endpoint.pathname) && !endpoint.search
  const isTree = /^\/repos\/[A-Za-z0-9-]{1,39}\/[A-Za-z0-9_.-]{1,100}\/git\/trees\/[a-f0-9]{40}$/.test(endpoint.pathname) &&
    query.size === 1 && query.get('recursive') === '1'
  // Only immutable blob SHA reads, never moving branches or arbitrary paths.
  const isBlob = /^\/repos\/[A-Za-z0-9-]{1,39}\/[A-Za-z0-9_.-]{1,100}\/git\/blobs\/[a-f0-9]{40}$/.test(endpoint.pathname) && !endpoint.search
  if (!isSearch && !isOwner && !isCommit && !isTree && !isBlob) {
    throw new Error('public GitHub Skill discovery endpoint not permitted')
  }
  signal.throwIfAborted()
  const response = await fetch(endpoint.toString(), {
    method: 'GET', redirect: 'error', credentials: 'omit', referrerPolicy: 'no-referrer',
    headers: {
      Accept: 'application/vnd.github+json',
      'X-GitHub-Api-Version': '2022-11-28',
      'User-Agent': 'dsh-delta-code-public-discovery',
    },
    signal,
  })
  if (!response.ok) throw new GitHubApiFailure(response.status, 'public-skill-discovery', 'GET')
  const maxBytes = 1_000_000
  const hintedLength = Number(response.headers.get('content-length'))
  if (Number.isFinite(hintedLength) && hintedLength > maxBytes) {
    throw new Error('public GitHub discovery response exceeds size budget')
  }
  const reader = response.body?.getReader()
  if (!reader) throw new Error('public GitHub discovery response has no body')
  const chunks: Uint8Array[] = []
  let size = 0
  try {
    while (true) {
      signal.throwIfAborted()
      const part = await reader.read()
      if (part.done) break
      size += part.value.byteLength
      if (size > maxBytes) {
        await reader.cancel()
        throw new Error('public GitHub discovery response exceeds size budget')
      }
      chunks.push(part.value)
    }
  } finally {
    reader.releaseLock()
  }
  const bytes = new Uint8Array(size)
  let cursor = 0
  for (const chunk of chunks) { bytes.set(chunk, cursor); cursor += chunk.length }
  return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes))
}

/** GitHub Actions job logs are a signed, redirected text download, never JSON.
 * GitHub's credential is sent only to api.github.com, never to the signed URL. */
const MAX_JOB_LOG_BYTES = 1024 * 1024
const LOG_EXCERPT_CHARS = 12_000
const LOG_CONTEXT_CHARS = 7_000

function allowedLogRedirect(location: string): URL {
  const url = new URL(location)
  const host = url.hostname.toLowerCase()
  if (url.protocol !== 'https:' || url.username || url.password ||
      !(host === 'githubusercontent.com' || host.endsWith('.githubusercontent.com') ||
        host.endsWith('.blob.core.windows.net'))) {
    throw new Error('GitHub Actions log redirect destination is not an approved HTTPS storage host')
  }
  return url
}

function redactGitHubLog(text: string, token: string): string {
  return text.replaceAll(token, '[REDACTED]')
    .replace(/github_pat_[A-Za-z0-9_]+/g, '[REDACTED]')
    .replace(/gh[pousr]_[A-Za-z0-9_]+/g, '[REDACTED]')
    .replace(/(Bearer\s+)[A-Za-z0-9._~+/-]+/gi, '$1[REDACTED]')
    .replace(/((?:GITHUB_TOKEN|GH_TOKEN|PASSWORD|SECRET|API_KEY)\s*[=:]\s*)[^\s]+/gi, '$1[REDACTED]')
    .replace(/-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----/g, '[REDACTED PRIVATE KEY]')
}

async function boundedLogExcerpt(response: Response, signal: AbortSignal, token: string): Promise<{
  excerpt: string; truncated: boolean; bytesRead: number
}> {
  const reader = response.body?.getReader()
  if (!reader) throw new Error('GitHub job log has no readable body')
  const bytes: Uint8Array[] = []
  let total = 0
  let isTruncated = false
  try {
    while (true) {
      signal.throwIfAborted()
      const next = await reader.read()
      if (next.done) break
      const available = MAX_JOB_LOG_BYTES - total
      if (next.value.byteLength > available) {
        if (available > 0) bytes.push(next.value.subarray(0, available))
        total += Math.max(0, available)
        isTruncated = true
        await reader.cancel()
        break
      }
      bytes.push(next.value)
      total += next.value.byteLength
    }
  } finally {
    reader.releaseLock()
  }
  const buffer = new Uint8Array(total)
  let offset = 0
  for (const item of bytes) { buffer.set(item, offset); offset += item.byteLength }
  const log = new TextDecoder('utf-8', { fatal: false }).decode(buffer)
  const lines = log.split('\n')
  // Root failures typically precede the generic "Process completed with exit code 1".
  // Preserve the first concrete failure and independent later failures instead of
  // displaying only the final exit-code wrapper.
  const concreteFailure = /\bnot ok\s+\d+\s+-|\bfailureType:|\bAssertionError\b|\b(?:error|fatal):\s*\S|\b(?:error|exception)\s+TS\d{4}:|\bTraceback\b|##\[error\](?!Process completed with exit code)/i
  const hits: number[] = []
  for (let i = 0; i < lines.length; i++) if (concreteFailure.test(lines[i]!)) hits.push(i)
  let selection: string
  if (!hits.length) {
    selection = log.slice(-LOG_EXCERPT_CHARS)
  } else {
    const anchors = hits.filter((index, position) => position === 0 || index - hits[position - 1]! > 40).slice(0, 3)
    const last = hits.at(-1)!
    if (last - anchors.at(-1)! > 40 && anchors.length < 3) anchors.push(last)
    const windows = anchors.map((index) =>
      '[Failure evidence, log line ' + (index + 1) + ']\n' +
      lines.slice(Math.max(0, index - 5), Math.min(lines.length, index + 48)).join('\n').slice(0, LOG_CONTEXT_CHARS))
    const tail = lines.slice(-12).join('\n').slice(-1500)
    selection = windows.join('\n\n---\n\n').slice(0, LOG_EXCERPT_CHARS - 1600) +
      '\n\n[Final job outcome]\n' + tail
  }
  return { excerpt: redactGitHubLog(selection, token), truncated: isTruncated || selection.length < log.length, bytesRead: total }
}

async function requestGitHubJobLogs(
  input: DeltaGitHubJobLogsRequest, tokenEnv: string, host?: GitHubCredentialHost,
): Promise<DeltaGitHubJobLogsResponse> {
  const { repository, account, jobId, signal } = input
  if (!Number.isSafeInteger(jobId) || jobId <= 0) throw new Error('invalid GitHub job ID')
  const token = await credentialForRequest(host, tokenEnv, account)
  await assertNativeGitHubOwner(account, repository, token, signal)
  const path = repository.split('/').map(encodeURIComponent).join('/')
  const response = await fetch(API + '/repos/' + path + '/actions/jobs/' + jobId + '/logs', {
    method: 'GET',
    headers: {
      Authorization: 'Bearer ' + token,
      Accept: 'application/vnd.github+json',
      'X-GitHub-Api-Version': '2022-11-28',
      'User-Agent': 'dsh-delta-code',
    },
    signal,
    redirect: 'manual',
  })
  if (response.status !== 302 && !response.ok) throw new GitHubApiFailure(response.status, repository, 'GET')
  let logResponse = response
  if (response.status === 302) {
    const location = response.headers.get('location')
    if (!location) throw new Error('GitHub Actions log redirect is missing')
    const signedUrl = allowedLogRedirect(location)
    // Never forward GitHub credentials or cookies to the storage endpoint.
    logResponse = await fetch(signedUrl, { method: 'GET', signal, redirect: 'error' })
    if (!logResponse.ok) throw new Error('GitHub Actions log download failed with HTTP ' + logResponse.status)
  }
  const bounded = await boundedLogExcerpt(logResponse, signal, token)
  return { repository, account, ...bounded }
}

/** Provider implementation supplied only by the composition root. */
export function createGitHubTransport(accounts: readonly GitHubAccountConfig[], host?: GitHubCredentialHost): DeltaGitHubTransport {
  // Snapshot approved bindings: feature modules only use an opaque account key.
  const bindings = new Map<string, string>(accounts.map(entry => [entry.owner.toLowerCase(), entry.tokenEnv] as const))
  // The native Host credential is the default when no plugin identities are configured.
  // This does not import the separate gh CLI login store.
  if (!accounts.length) bindings.set('native', 'GITHUB_TOKEN')
  return {
    publicRead: publicGitHubMetadata,
    request(input) {
      const credentialEnv = bindings.get(input.account.toLowerCase())
      if (!credentialEnv) throw new Error('GitHub account binding unavailable for ' + input.account)
      return requestGitHubApi(input, credentialEnv, host)
    },
    jobLogs(input) {
      const credentialEnv = bindings.get(input.account.toLowerCase())
      if (!credentialEnv) throw new Error('GitHub account binding unavailable for ' + input.account)
      return requestGitHubJobLogs(input, credentialEnv, host)
    },
  }
}
