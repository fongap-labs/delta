/** Delta-owned LiveShare Host capability and its HTTPS Relay deployment entry. */

import type { Context } from '@deepseek-ai/cordis'
import type { SessionId } from '@deepseek-ai/dsh-session/types'
import type { RemoteAccessResourceOf } from '@deepseek-ai/dsh-api-remote-access/types'
import type {} from '@deepseek-ai/dsh-api-remote-access'
import type {} from '@deepseek-ai/dsh-api-session-controller'
import { credentialRef } from '@deepseek-ai/dsh-credentials'
import { Remote, RemoteError, TypertRemoteService } from '@deepseek-ai/dsh-typert-protocol'
import z from '@deepseek-ai/schemastery'
import WebSocket from 'ws'
import type { SessionHistoryRecord } from '@deepseek-ai/dsh-api-session-controller/types'
import type { RemoteAccessRole } from '@deepseek-ai/dsh-api-remote-access/types'
import type { LiveShareCreateRequest, LiveShareCreated, LiveShareEntry } from './types.ts'
import { projectPublicSessionMessage } from './session-public-projection.ts'

export type * from './types.ts'

const OWNER_TOKEN_REF = credentialRef('DELTA_LIVESHARE_OWNER')
const MAX_MESSAGES = 100
const MAX_TEXT_BYTES = 128 * 1024
const MAX_SNAPSHOT_BYTES = 128 * 1024
const MAX_RELAY_FRAME_BYTES = 900 * 1024
const MAX_OUTBOUND_BUFFER_BYTES = 2 * 1024 * 1024
const MAX_OWNER_RESPONSE_BYTES = 2 * 1024 * 1024
const MIN_SHARE_TTL_MS = 60 * 60 * 1000
const DEFAULT_SHARE_TTL_MS = 24 * 60 * 60 * 1000
const MAX_SHARE_TTL_MS = 24 * 60 * 60 * 1000
const READER_ACTIONS = ['session/follow'] as const
const READER_ROLE = ({ name: 'delta-liveshare-read-only', resource: 'session', actions: READER_ACTIONS } as const) satisfies RemoteAccessRole<'session'>

declare module '@deepseek-ai/dsh-typert-protocol' {
  interface PrincipalMap {
    /** One Relay viewer's temporary authority, bound to exactly one Session. */
    'delta-liveshare': { readonly kind: 'delta-liveshare'; readonly shareId: string; readonly sessionId: SessionId }
  }
}

declare module '@deepseek-ai/cordis' {
  interface Context {
    /** Delta-owned LiveShare Remote service. */
    deltaLiveShare: LiveShareService
  }
}

/** Delta LiveShare settings. Secrets are held through the credentials service, never in this document. */
export interface Config {
  /** HTTPS origin of the user's self-hosted Relay, without a path. */
  readonly relayOrigin?: string
}

const Config = z.object({ relayOrigin: z.string().default('') })
interface ActiveShare {
  readonly id: string
  readonly sessionId: SessionId
  readonly expiresAt: number
  readonly hostToken: string
  readonly principal: { readonly kind: 'delta-liveshare'; readonly shareId: string; readonly sessionId: SessionId }
  readonly requests: Map<string, Map<string, AbortController>>
  socket: WebSocket | undefined
  revoked: boolean
  reconnectDelayMs: number
  reconnectTimer: ReturnType<typeof setTimeout> | undefined
  reconnecting: Promise<void> | undefined
}

interface RelayShareResponse {
  readonly id: string
  readonly expiresAt: number
  readonly viewerUrl: string
  readonly hostToken: string
}

interface RelayShareSummary {
  readonly id: string
  readonly expiresAt: number
  readonly connected: boolean
}

/** Remote methods for managing one-session LiveShare links. */
export class LiveShareService extends TypertRemoteService {
  static inject = ['credentials', 'remoteAccess', 'sessionController', 'typert']
  static Config = Config

  private readonly relayOrigin: string
  private readonly shares = new Map<string, ActiveShare>()
  private readonly stopped = new AbortController()

  constructor(ctx: Context, config: Config = {}) {
    super(ctx, 'deltaLiveShare', { namespace: 'liveshare' })
    this.relayOrigin = relayOriginOf(Config(config).relayOrigin)
    ctx.remoteAccess.registerPolicy('delta-liveshare', {
      bindings: principal => {
        const share = this.shares.get(principal.shareId)
        if (share === undefined || !this.active(share) || share.sessionId !== principal.sessionId) return []
        const resource: RemoteAccessResourceOf<'session'> = { kind: 'session', id: share.sessionId }
        return [{ role: READER_ROLE, resource }]
      },
    })
    ctx.effect(() => {
      const timer = setInterval(() => this.expireLocalShares(), 1000)
      timer.unref?.()
      return () => {
        clearInterval(timer)
        this.stopped.abort(new Error('LiveShare service stopped'))
        for (const share of this.shares.values()) this.closeLocal(share)
        this.shares.clear()
      }
    }, 'delta-liveshare: active shares')
  }

  /** List active shares without returning any viewer or Host credential. */
  @Remote
  async list(signal: AbortSignal): Promise<readonly LiveShareEntry[]> {
    this.assertConfigured()
    const credentials = await this.ctx.credentials.resolve(OWNER_TOKEN_REF)
    if (credentials?.value === undefined || !isOwnerToken(credentials.value)) {
      throw new RemoteError('gateway/bad-request', 'Set the LiveShare Relay owner token in Settings before listing shares', {})
    }
    const response = await ownerRequest<{ readonly shares: readonly RelayShareSummary[] }>(
      this.relayOrigin, credentials.value, { method: 'GET', path: '/v1/shares', signal },
    )
    if (!Array.isArray(response.shares)) throw new RemoteError('gateway/internal', 'Relay returned invalid share metadata', {})
    return response.shares.flatMap(item => {
      if (!UUID.test(item.id) || !Number.isSafeInteger(item.expiresAt) || typeof item.connected !== 'boolean'
        || item.expiresAt <= Date.now()) return []
      const local = this.shares.get(item.id)
      return [{
        id: item.id,
        ...(local === undefined ? {} : { sessionId: local.sessionId }),
        expiresAt: item.expiresAt,
        connected: item.connected && local?.socket?.readyState === WebSocket.OPEN,
      }]
    })
  }

  /** Create a one-session read-only share and establish its outbound Relay connection. */
  @Remote
  async create(request: LiveShareCreateRequest, signal: AbortSignal): Promise<LiveShareCreated> {
    this.assertConfigured()
    if (!Number.isSafeInteger(request.expiresInMs)
      || request.expiresInMs < MIN_SHARE_TTL_MS || request.expiresInMs > MAX_SHARE_TTL_MS) {
      throw new RemoteError('gateway/bad-request', 'Share duration must be one hour or 24 hours', {})
    }
    signal.throwIfAborted()
    this.stopped.signal.throwIfAborted()
    const credentials = await this.ctx.credentials.resolve(OWNER_TOKEN_REF)
    if (credentials?.value === undefined || !isOwnerToken(credentials.value)) {
      throw new RemoteError('gateway/bad-request', 'Set the LiveShare Relay owner token in Settings before creating a share', {})
    }

    await this.ctx.sessionController.events({ address: { kind: 'session', sessionId: request.sessionId }, maxMessages: 1 }, signal)

    const relayResponse = await ownerRequest<RelayShareResponse>(this.relayOrigin, credentials.value, {
      method: 'POST', path: '/v1/shares', body: { expiresInMs: request.expiresInMs }, signal,
    })
    validateRelayShareResponse(relayResponse, this.relayOrigin)
    const share: ActiveShare = {
      id: relayResponse.id,
      sessionId: request.sessionId,
      expiresAt: relayResponse.expiresAt,
      hostToken: relayResponse.hostToken,
      principal: { kind: 'delta-liveshare', shareId: relayResponse.id, sessionId: request.sessionId },
      requests: new Map(),
      socket: undefined,
      revoked: false,
      reconnectDelayMs: 1000,
      reconnectTimer: undefined,
      reconnecting: undefined,
    }
    this.shares.set(share.id, share)
    try {
      await this.connect(share, signal)
    } catch (error) {
      share.revoked = true
      this.closeLocal(share)
      this.shares.delete(share.id)
      await ownerRequest(this.relayOrigin, credentials.value, { method: 'DELETE', path: `/v1/shares/${share.id}`, signal: AbortSignal.timeout(5000) }).catch(() => {})
      throw new RemoteError('gateway/internal', 'Could not connect to the LiveShare Relay', {}, { cause: error })
    }
    return { id: share.id, sessionId: share.sessionId, expiresAt: share.expiresAt, viewerUrl: relayResponse.viewerUrl }
  }

  /** Revoke one active share at the Relay and terminate every Host-side stream. */
  @Remote
  async revoke(request: { readonly id: string }, signal: AbortSignal): Promise<void> {
    this.assertConfigured()
    if (!UUID.test(request.id)) throw new RemoteError('gateway/bad-request', 'Invalid share id', {})
    const credentials = await this.ctx.credentials.resolve(OWNER_TOKEN_REF)
    if (credentials?.value === undefined || !isOwnerToken(credentials.value)) {
      throw new RemoteError('gateway/bad-request', 'Set the LiveShare Relay owner token in Settings before revoking a share', {})
    }
    const share = this.shares.get(request.id)
    try {
      await ownerRequest(this.relayOrigin, credentials.value, { method: 'DELETE', path: `/v1/shares/${request.id}`, signal })
    } finally {
      if (share !== undefined) {
        share.revoked = true
        this.closeLocal(share)
        this.shares.delete(share.id)
        this.ctx.remoteAccess.invalidate(share.principal)
      }
    }
  }

  private assertConfigured(): void {
    if (this.relayOrigin.length === 0) {
      throw new RemoteError('gateway/bad-request', 'Set the self-hosted HTTPS Relay address in LiveShare Settings first', {})
    }
  }

  private active(share: ActiveShare): boolean {
    return !share.revoked && share.expiresAt > Date.now() && !this.stopped.signal.aborted
  }

  private async connect(share: ActiveShare, signal?: AbortSignal): Promise<void> {
    if (!this.active(share)) throw new Error('share expired')
    const url = new URL(`/v1/host/${share.id}`, this.relayOrigin)
    url.protocol = 'wss:'
    const socket = new WebSocket(url, { headers: { authorization: `Bearer ${share.hostToken}` }, perMessageDeflate: false, maxPayload: 1024 * 1024 })
    share.socket = socket
    const connectionSignal = signal === undefined
      ? this.stopped.signal
      : AbortSignal.any([signal, this.stopped.signal])
    try {
      await new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error('Relay connection timed out')), 10_000)
        const finish = (error?: unknown) => {
          clearTimeout(timer)
          socket.off('open', opened)
          socket.off('error', failed)
          connectionSignal.removeEventListener('abort', aborted)
          if (error === undefined) resolve()
          else reject(error)
        }
        const opened = () => finish()
        const failed = () => finish(new Error('Relay connection failed'))
        const aborted = () => { socket.close(); finish(connectionSignal.reason) }
        socket.once('open', opened)
        socket.once('error', failed)
        if (connectionSignal.aborted) aborted()
        else connectionSignal.addEventListener('abort', aborted, { once: true })
      })
    } catch (error) {
      if (share.socket === socket) share.socket = undefined
      socket.terminate()
      throw error
    }
    share.reconnectDelayMs = 1000
    socket.on('message', (data, binary) => { void this.receive(share, socket, data, binary) })
    socket.once('close', () => {
      if (share.socket === socket) share.socket = undefined
      this.abortRequests(share)
      if (this.active(share)) this.scheduleReconnect(share)
    })
    socket.on('error', () => {})
  }

  private scheduleReconnect(share: ActiveShare): void {
    if (share.reconnecting !== undefined || share.reconnectTimer !== undefined || !this.active(share)) return
    share.reconnectTimer = setTimeout(() => {
      share.reconnectTimer = undefined
      const operation = this.reconnect(share)
      share.reconnecting = operation
      void operation.finally(() => { if (share.reconnecting === operation) share.reconnecting = undefined })
    }, share.reconnectDelayMs)
    share.reconnectDelayMs = Math.min(share.reconnectDelayMs * 2, 30_000)
    share.reconnectTimer.unref?.()
  }

  private async reconnect(share: ActiveShare): Promise<void> {
    try { await this.connect(share) }
    catch { this.scheduleReconnect(share) }
  }

  private async receive(share: ActiveShare, socket: WebSocket, data: WebSocket.RawData, binary: boolean): Promise<void> {
    if (share.socket !== socket || !this.active(share) || binary) return
    const text = Buffer.isBuffer(data) ? data.toString('utf8')
      : Array.isArray(data) ? Buffer.concat(data).toString('utf8') : Buffer.from(data).toString('utf8')
    if (Buffer.byteLength(text) > 2048) return
    let frame: Record<string, unknown>
    try {
      const decoded: unknown = JSON.parse(text)
      if (typeof decoded !== 'object' || decoded === null || Array.isArray(decoded)) return
      frame = decoded as Record<string, unknown>
    } catch { return }

    if (frame.type === 'disconnect' && typeof frame.viewerId === 'string') {
      for (const task of share.requests.get(frame.viewerId)?.values() ?? []) task.abort(new Error('viewer disconnected'))
      share.requests.delete(frame.viewerId)
      return
    }
    if (frame.type === 'cancel' && typeof frame.id === 'string' && typeof frame.viewerId === 'string') {
      share.requests.get(frame.viewerId)?.get(frame.id)?.abort(new Error('viewer cancelled'))
      return
    }
    if (frame.type !== 'request' || frame.op !== 'follow'
      || typeof frame.id !== 'string' || !/^[A-Za-z0-9_-]{1,64}$/u.test(frame.id)
      || typeof frame.viewerId !== 'string' || !UUID.test(frame.viewerId)) return
    const requests = share.requests.get(frame.viewerId) ?? new Map()
    if (requests.has(frame.id)) return
    if (requests.size >= 1) {
      this.send(socket, { type: 'response', id: frame.id, viewerId: frame.viewerId, ok: false, error: 'request refused' })
      return
    }
    const controller = new AbortController()
    requests.set(frame.id, controller)
    share.requests.set(frame.viewerId, requests)
    try {
      await this.follow(share, frame.viewerId, frame.id, controller.signal)
    } catch {
      if (this.active(share) && socket.readyState === WebSocket.OPEN) {
        this.send(socket, { type: 'response', id: frame.id, viewerId: frame.viewerId, ok: false, error: 'request refused' })
      }
    } finally {
      requests.delete(frame.id)
      if (requests.size === 0) share.requests.delete(frame.viewerId)
    }
  }

  private async follow(share: ActiveShare, viewerId: string, id: string, signal: AbortSignal): Promise<void> {
    const action = 'session/follow'
    if (!this.authorized(share, action)) throw new Error('share authorization was revoked')
    const address = { kind: 'session' as const, sessionId: share.sessionId }
    let isOpened = false
    for await (const frame of this.ctx.sessionController.follow({ address, maxMessages: MAX_MESSAGES, activate: false }, signal)) {
      signal.throwIfAborted()
      if (!this.active(share) || !this.authorized(share, action)) throw new Error('share authorization was revoked')
      const socket = share.socket
      if (socket?.readyState !== WebSocket.OPEN) throw new Error('Relay disconnected')
      if (frame.type === 'snapshot') {
        isOpened = true
        this.send(socket, {
          type: 'response', id, viewerId, ok: true,
          data: { messages: publicMessages(frame.records) },
        })
        continue
      }
      if (!isOpened || frame.type !== 'event') continue
      const message = projectPublicSessionMessage(frame, MAX_TEXT_BYTES)
      if (message !== undefined) this.send(socket, { type: 'event', id, viewerId, data: { type: 'message', message } })
    }
    if (!isOpened) throw new Error('Session history is unavailable')
  }

  private authorized(share: ActiveShare, action: 'session/follow'): boolean {
    if (!this.active(share)) return false
    const resource: RemoteAccessResourceOf<'session'> = { kind: 'session', id: share.sessionId }
    return this.ctx.remoteAccess.decide({ principal: share.principal, action, resources: [resource] }).kind === 'allow'
  }

  private send(socket: WebSocket, value: unknown): void {
    if (socket.readyState !== WebSocket.OPEN) return
    const payload = JSON.stringify(value)
    if (Buffer.byteLength(payload) > MAX_RELAY_FRAME_BYTES
      || socket.bufferedAmount + Buffer.byteLength(payload) > MAX_OUTBOUND_BUFFER_BYTES) {
      socket.close(1013, 'share stream exceeded transport limits')
      return
    }
    socket.send(payload)
  }

  private abortRequests(share: ActiveShare): void {
    for (const requests of share.requests.values()) for (const task of requests.values()) task.abort(new Error('Relay disconnected'))
    share.requests.clear()
  }

  private closeLocal(share: ActiveShare): void {
    if (share.reconnectTimer !== undefined) clearTimeout(share.reconnectTimer)
    share.reconnectTimer = undefined
    this.abortRequests(share)
    const socket = share.socket
    share.socket = undefined
    if (socket !== undefined && socket.readyState < WebSocket.CLOSING) socket.close(1001, 'share ended')
  }

  private expireLocalShares(): void {
    for (const [id, share] of this.shares) {
      if (this.active(share)) continue
      share.revoked = true
      this.closeLocal(share)
      this.shares.delete(id)
      this.ctx.remoteAccess.invalidate(share.principal)
    }
  }
}

/** Host-side Cordis entry. */
export const name = 'delta-liveshare'
export const inject = LiveShareService.inject
export function apply(ctx: Context, config: Config = {}): void {
  ctx.plugin(LiveShareService, config)
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u

function relayOriginOf(value: string): string {
  if (value === '') return ''
  const url = new URL(value)
  if (url.protocol !== 'https:' || url.username || url.password || url.pathname !== '/' || url.search || url.hash) {
    throw new TypeError('LiveShare Relay address must be an HTTPS origin without path, credentials, query, or fragment')
  }
  return url.origin
}

function isOwnerToken(value: string): boolean {
  if (!/^[A-Za-z0-9_-]{43}$/u.test(value)) return false
  const bytes = Buffer.from(value, 'base64url')
  return bytes.byteLength === 32 && bytes.toString('base64url') === value
}

async function ownerRequest<T = void>(
  origin: string,
  ownerToken: string,
  request: { readonly method: 'GET' | 'POST' | 'DELETE'; readonly path: string; readonly body?: unknown; readonly signal: AbortSignal },
): Promise<T> {
  request.signal.throwIfAborted()
  const url = new URL(request.path, origin)
  const response = await fetch(url, {
    method: request.method,
    headers: {
      authorization: `Bearer ${ownerToken}`,
      ...(request.body === undefined ? {} : { 'content-type': 'application/json' }),
    },
    ...(request.body === undefined ? {} : { body: JSON.stringify(request.body) }),
    redirect: 'manual',
    credentials: 'omit',
    signal: AbortSignal.any([request.signal, AbortSignal.timeout(10_000)]),
  })
  if (response.status >= 300 && response.status < 400) {
    await response.body?.cancel()
    throw new Error('Relay redirects are not followed')
  }
  if (!response.ok) {
    await response.body?.cancel()
    throw new Error(`Relay request returned HTTP ${response.status}`)
  }
  if (response.status === 204) return undefined as T
  const lengthHeader = response.headers.get('content-length')
  const length = lengthHeader === null ? undefined : Number(lengthHeader)
  if (length !== undefined && (!Number.isSafeInteger(length) || length < 0 || length > MAX_OWNER_RESPONSE_BYTES)) {
    await response.body?.cancel()
    throw new Error('Relay response is too large')
  }
  const reader = response.body?.getReader()
  if (reader === undefined) throw new Error('Relay returned an empty response')
  const chunks: Buffer[] = []
  let size = 0
  while (true) {
    const { done, value } = await reader.read()
    if (done) break
    size += value.byteLength
    if (size > MAX_OWNER_RESPONSE_BYTES) {
      await reader.cancel().catch(() => {})
      throw new Error('Relay response is too large')
    }
    chunks.push(Buffer.from(value))
  }
  const text = Buffer.concat(chunks, size).toString('utf8')
  return JSON.parse(text) as T
}

function validateRelayShareResponse(value: RelayShareResponse, origin: string): void {
  const url = new URL(value.viewerUrl)
  if (!UUID.test(value.id) || !Number.isSafeInteger(value.expiresAt) || !isOwnerToken(value.hostToken)
    || url.protocol !== 'https:' || url.username || url.password
    || url.origin !== origin || url.pathname !== '/share/' || url.search || !isOwnerToken(url.hash.slice(1))) {
    throw new Error('Relay returned invalid share data')
  }
}

function publicMessages(records: readonly SessionHistoryRecord[]): readonly { role: 'user' | 'assistant'; text: string }[] {
  const rows: { role: 'user' | 'assistant'; text: string }[] = []
  let textBytes = 0
  for (let index = records.length - 1; index >= 0 && rows.length < MAX_MESSAGES; index -= 1) {
    const record = records[index]
    if (record === undefined) continue
    const value = projectPublicSessionMessage(record, MAX_TEXT_BYTES)
    if (value === undefined) continue
    const nextBytes = Buffer.byteLength(value.text)
    if (textBytes + nextBytes > MAX_SNAPSHOT_BYTES) break
    rows.push(value)
    textBytes += nextBytes
  }
  return rows.reverse()
}

export { DEFAULT_SHARE_TTL_MS, MIN_SHARE_TTL_MS, MAX_SHARE_TTL_MS }
