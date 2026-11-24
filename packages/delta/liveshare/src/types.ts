import type { SessionId } from '@deepseek-ai/dsh-session/types'

/** A newly created, time-limited read-only share. The viewer URL is a bearer credential. */
export interface LiveShareCreated {
  readonly id: string
  readonly sessionId: SessionId
  readonly expiresAt: number
  readonly viewerUrl: string
}

/** Request for one session share. */
export interface LiveShareCreateRequest {
  readonly sessionId: SessionId
  /** One hour or 24 hours. */
  readonly expiresInMs: number
}

/** Share metadata safe to show in the owner's local UI. */
export interface LiveShareEntry {
  readonly id: string
  /** Unknown after a Desktop restart; the Relay stores no session identity. */
  readonly sessionId?: SessionId
  readonly expiresAt: number
  readonly connected: boolean
}
