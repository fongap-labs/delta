import { createHash, randomBytes, randomUUID, timingSafeEqual } from 'node:crypto'
import { mkdir, open, readFile, rename } from 'node:fs/promises'
import { dirname, isAbsolute } from 'node:path'

export const MIN_SHARE_TTL_MS = 60_000
export const DEFAULT_SHARE_TTL_MS = 24 * 60 * 60 * 1000
export const MAX_SHARE_TTL_MS = DEFAULT_SHARE_TTL_MS

const TOKEN_BYTES = 32
const SHARE_ID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u
const MAX_STORED_SHARES = 10_000
const MAX_STATE_BYTES = 16 * 1024 * 1024

function digest(value) {
  return createHash('sha256').update(value, 'utf8').digest('hex')
}

function equalDigest(left, right) {
  const a = Buffer.from(left, 'hex')
  const b = Buffer.from(right, 'hex')
  return a.length === b.length && timingSafeEqual(a, b)
}

function token() {
  return randomBytes(TOKEN_BYTES).toString('base64url')
}

function validOwnerCredential(value) {
  if (typeof value !== 'string' || !/^[A-Za-z0-9_-]{43}$/u.test(value)) return false
  const decoded = Buffer.from(value, 'base64url')
  return decoded.byteLength === TOKEN_BYTES && decoded.toString('base64url') === value
}

function validateState(value) {
  if (typeof value !== 'object' || value === null || value.version !== 1 || !Array.isArray(value.shares)) {
    throw new Error('LiveShare relay metadata is invalid')
  }
  if (value.shares.length > MAX_STORED_SHARES) throw new Error('LiveShare relay metadata exceeds its capacity')
  const shares = new Map()
  const viewerDigests = new Set()
  for (const item of value.shares) {
    if (typeof item !== 'object' || item === null || !SHARE_ID.test(item.id)
        || !/^[a-f0-9]{64}$/u.test(item.viewerDigest) || !/^[a-f0-9]{64}$/u.test(item.hostDigest)
        || !Number.isSafeInteger(item.expiresAt) || typeof item.revoked !== 'boolean'
        || !Number.isSafeInteger(item.createdAt)) {
      throw new Error('LiveShare relay metadata is invalid')
    }
    if (shares.has(item.id) || viewerDigests.has(item.viewerDigest)) {
      throw new Error('LiveShare relay metadata contains duplicate identities')
    }
    shares.set(item.id, Object.freeze({ ...item }))
    viewerDigests.add(item.viewerDigest)
  }
  return shares
}

async function writePrivateFile(path, content) {
  await mkdir(dirname(path), { recursive: true })
  const temp = `${path}.${randomUUID()}.tmp`
  const file = await open(temp, 'wx', 0o600)
  try {
    await file.writeFile(content, 'utf8')
    await file.sync()
  } finally {
    await file.close()
  }
  await rename(temp, path)
  try {
    const published = await open(path, 'r+')
    try { await published.chmod(0o600) } finally { await published.close() }
  } catch { /* Windows ACLs govern access. */ }
}

/**
 * Create the metadata owner for one self-hosted relay. Persisted records carry
 * token digests and expiry/revocation state only; the plaintext bodies and
 * credentials are never written to the state file.
 */
export async function createRelayRegistry({ ownerToken, statePath, now = Date.now } = {}) {
  if (!validOwnerCredential(ownerToken)) throw new TypeError('Relay owner credential must be a 32-byte base64url token')
  if (typeof statePath !== 'string' || statePath.length === 0 || !isAbsolute(statePath)) throw new TypeError('Relay statePath must be absolute')
  if (typeof now !== 'function') throw new TypeError('Relay clock must be a function')

  const ownerDigest = digest(ownerToken)
  let shares = new Map()
  try {
    const contents = await readFile(statePath)
    if (contents.byteLength > MAX_STATE_BYTES) throw new Error('LiveShare relay metadata exceeds its size limit')
    shares = validateState(JSON.parse(contents.toString('utf8')))
  } catch (error) {
    if (error?.code !== 'ENOENT') throw error
  }
  let updateChain = Promise.resolve()

  function update(operation) {
    const pending = updateChain.catch(() => {}).then(async () => {
      const result = operation(shares)
      if (result.next !== undefined) {
        const payload = JSON.stringify({ version: 1, shares: [...result.next.values()] })
        await writePrivateFile(statePath, payload)
        shares = result.next
      }
      return result.value
    })
    updateChain = pending.then(() => {}, () => {})
    return pending
  }

  function record(id) {
    return shares.get(id)
  }

  function isUsable(item, at = now()) {
    return item !== undefined && !item.revoked && item.expiresAt > at
  }

  return Object.freeze({
    /** Compare a supplied owner credential without a data-dependent string comparison. */
    authenticateOwner(value) {
      if (!validOwnerCredential(value)) return false
      return equalDigest(ownerDigest, digest(value))
    },

    /** Create a limited viewer credential and one-time host credential. */
    async createShare({ expiresInMs = DEFAULT_SHARE_TTL_MS } = {}) {
      if (!Number.isSafeInteger(expiresInMs) || expiresInMs < MIN_SHARE_TTL_MS || expiresInMs > MAX_SHARE_TTL_MS) {
        throw new RangeError('Share duration must be between one minute and 24 hours')
      }
      return update(current => {
        const createdAt = now()
        if (!Number.isSafeInteger(createdAt) || createdAt < 0 || createdAt + expiresInMs > Number.MAX_SAFE_INTEGER) {
          throw new RangeError('Relay clock is outside the supported range')
        }
        const viewerToken = token()
        const hostToken = token()
        const id = randomUUID()
        const item = Object.freeze({
          id,
          createdAt,
          expiresAt: createdAt + expiresInMs,
          viewerDigest: digest(viewerToken),
          hostDigest: digest(hostToken),
          revoked: false,
        })
        const next = new Map([...current].filter(([, share]) => !share.revoked && share.expiresAt > createdAt))
        if (next.size >= MAX_STORED_SHARES) throw new Error('LiveShare relay is at its share limit')
        next.set(id, item)
        return { next, value: Object.freeze({ id, viewerToken, hostToken, createdAt, expiresAt: item.expiresAt }) }
      })
    },

    /** Admit a viewer token only while its share is live. */
    authenticateViewer(value) {
      if (typeof value !== 'string' || value.length < 40 || value.length > 128) return undefined
      const valueDigest = digest(value)
      for (const item of shares.values()) {
        if (isUsable(item) && equalDigest(item.viewerDigest, valueDigest)) {
          return Object.freeze({ id: item.id, expiresAt: item.expiresAt })
        }
      }
      return undefined
    },

    /** Admit the one Desktop Host holding the one-time secret for this share. */
    authenticateHost(id, value) {
      if (!SHARE_ID.test(id) || typeof value !== 'string' || value.length < 40 || value.length > 128) return false
      const item = record(id)
      return isUsable(item) && equalDigest(item.hostDigest, digest(value))
    },

    /** Return only non-secret share metadata. */
    getShare(id) {
      const item = record(id)
      if (item === undefined) return undefined
      return Object.freeze({ id: item.id, createdAt: item.createdAt, expiresAt: item.expiresAt, revoked: item.revoked })
    },

    /** Revoke before any sockets are closed by the server. */
    async revoke(id) {
      return update(current => {
        const item = current.get(id)
        if (item === undefined) return { value: false }
        if (item.revoked) return { value: true }
        const next = new Map(current)
        next.set(id, Object.freeze({ ...item, revoked: true }))
        return { next, value: true }
      })
    },

    /** Fail closed after expiry and persist revocation before dropping transport state. */
    async expire(at = now()) {
      return update(current => {
        const next = new Map(current)
        const expired = []
        for (const [id, item] of current) {
          if (!item.revoked && item.expiresAt <= at) {
            next.set(id, Object.freeze({ ...item, revoked: true }))
            expired.push(id)
          }
        }
        return expired.length === 0 ? { value: expired } : { next, value: expired }
      })
    },

    /** A test and readiness view that never includes token digests. */
    listMetadata() {
      return Object.freeze([...shares.values()].map(item => Object.freeze({
        id: item.id,
        createdAt: item.createdAt,
        expiresAt: item.expiresAt,
        revoked: item.revoked,
      })))
    },
  })
}
