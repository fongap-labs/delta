/** Delta Forge's read-only API adapter. It performs no network request until called. */
const API_DEFAULTS = Object.freeze({ github: 'https://api.github.com', gitlab: 'https://gitlab.com' })
const SEGMENT = /^[A-Za-z0-9_][A-Za-z0-9._-]{0,99}$/u
const MAX_BODY = 1024 * 1024

function projectParts(provider, project) {
  if (typeof project !== 'string' || project.length > 512) throw new TypeError('Invalid repository path')
  const parts = project.split('/')
  if ((provider === 'github' && parts.length !== 2) || (provider === 'gitlab' && (parts.length < 2 || parts.length > 20))
      || parts.some(part => !SEGMENT.test(part) || part === '.' || part === '..')) {
    throw new TypeError('Invalid repository path')
  }
  return parts
}

function apiRoot(provider, value) {
  const url = new URL(value ?? API_DEFAULTS[provider])
  if (url.protocol !== 'https:' || url.username || url.password || url.search || url.hash) {
    throw new TypeError('Forge API URL must be HTTPS without credentials, query, or fragment')
  }
  url.pathname = `${url.pathname.replace(/\/+$/u, '')}/`
  return url
}

async function boundedJson(response) {
  if (!response.body) throw new Error('Forge response has no body')
  const reader = response.body.getReader()
  const chunks = []
  let size = 0
  try {
    while (true) {
      const result = await reader.read()
      if (result.done) break
      size += result.value.byteLength
      if (size > MAX_BODY) throw new Error('Forge response exceeds the size limit')
      chunks.push(result.value)
    }
  } catch (error) {
    await reader.cancel().catch(() => {})
    throw error
  } finally { reader.releaseLock() }
  const bytes = new Uint8Array(size)
  let offset = 0
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength }
  try { return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)) }
  catch { throw new Error('Forge response is not valid JSON') }
}

function nonempty(value, field) {
  if (typeof value !== 'string' || value.trim().length === 0) throw new Error(`Forge response is missing ${field}`)
  return value
}

function boundedText(value, field, maximum) {
  return nonempty(value, field).slice(0, maximum)
}

/** Validate a provider request before prompting for approval or accessing the network. */
export function validateChangeRequest(provider, project, number) {
  if (!Object.hasOwn(API_DEFAULTS, provider)) throw new TypeError('Unsupported Forge provider')
  const parts = projectParts(provider, project)
  if (!Number.isSafeInteger(number) || number < 1) throw new TypeError('Invalid change request number')
  return Object.freeze({ provider, project: parts.join('/'), number })
}

/** Explicitly configured provider. Custom HTTPS origins are administrator-selected, never model-selected. */
export class ForgeClient {
  constructor(provider, { origin, fetchImpl = fetch, timeoutMs = 30000 } = {}) {
    if (!Object.hasOwn(API_DEFAULTS, provider)) throw new TypeError('Unsupported Forge provider')
    if (!Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 120000) throw new TypeError('Invalid Forge timeout')
    this.provider = provider
    this.root = apiRoot(provider, origin)
    this.fetch = fetchImpl
    this.timeoutMs = timeoutMs
  }

  /** Fetch one pull/merge request. The optional token is resolved by the caller for this operation only. */
  async readChange(project, number, { token, signal } = {}) {
    const validated = validateChangeRequest(this.provider, project, number)
    const parts = validated.project.split('/')
    if (token !== undefined && (typeof token !== 'string' || !/^[\x21-\x7e]+$/u.test(token))) {
      throw new TypeError('Invalid Forge credential')
    }
    const route = this.provider === 'github'
      ? `repos/${parts.map(encodeURIComponent).join('/')}/pulls/${number}`
      : `api/v4/projects/${encodeURIComponent(parts.join('/'))}/merge_requests/${number}`
    const url = new URL(route, this.root)
    const headers = { accept: 'application/json' }
    if (this.provider === 'github') {
      headers.accept = 'application/vnd.github+json'
      headers['x-github-api-version'] = '2022-11-28'
    }
    if (token !== undefined) headers.authorization = `Bearer ${token}`
    const timeout = AbortSignal.timeout(this.timeoutMs)
    const control = signal === undefined ? timeout : AbortSignal.any([signal, timeout])
    control.throwIfAborted()
    const response = await this.fetch(url, { method: 'GET', headers, signal: control, redirect: 'manual', credentials: 'omit' })
    if (response.status >= 300 && response.status < 400) {
      await response.body?.cancel()
      throw new Error('Forge redirects are not followed')
    }
    if (!response.ok) {
      await response.body?.cancel()
      throw new Error(`Forge request failed with HTTP ${response.status}`)
    }
    const data = await boundedJson(response)
    if (typeof data !== 'object' || data === null || Array.isArray(data)) throw new Error('Invalid Forge response')
    const source = this.provider === 'github' ? data.head?.ref : data.source_branch
    const target = this.provider === 'github' ? data.base?.ref : data.target_branch
    const sha = this.provider === 'github' ? data.head?.sha : data.sha
    return Object.freeze({
      provider: this.provider,
      project: parts.join('/'),
      number,
      title: boundedText(data.title, 'title', 1000),
      state: boundedText(data.state, 'state', 100),
      sourceBranch: boundedText(source, 'source branch', 1000),
      targetBranch: boundedText(target, 'target branch', 1000),
      headSha: boundedText(sha, 'head revision', 128),
      description: typeof (this.provider === 'github' ? data.body : data.description) === 'string'
        ? (this.provider === 'github' ? data.body : data.description).slice(0, 12000) : '',
    })
  }
}
