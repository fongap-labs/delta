/** Owner-facing share confirmation, link handoff, and immediate revocation. */

import { useState } from 'react'
import type { SessionId } from '@deepseek-ai/dsh-session/types'
import { Button, Modal } from '@deepseek-ai/dsh-client-ui-primitives'
import type { HostObservable, InjectFace, PropsHooks, PropsLocale, PropsRuntime } from '@deepseek-ai/dsh-client-ui-slots'
import type { LiveShareCreated, LiveShareEntry } from '../types.ts'

/** One requested session for the share dialog. */
export interface LiveShareDialogRequest {
  readonly sessionId: SessionId
  readonly title: string
}

/** The dialog's read-only state. */
export interface LiveShareDialogState {
  readonly request: LiveShareDialogRequest | null
  readonly shares: readonly LiveShareEntry[]
  readonly configured: boolean
  readonly listUnavailable: boolean
}

/** Owner actions handed to the overlay. */
export interface LiveShareDialogFace {
  hooks: { state: HostObservable<LiveShareDialogState> }
  settle: () => void
  create: (sessionId: SessionId, expiresInMs: number) => Promise<LiveShareCreated>
  revoke: (id: string, sessionId: SessionId) => Promise<void>
  copy: (url: string) => Promise<boolean>
}

/** Complete dialog props. */
export type LiveShareDialogProps = PropsRuntime<'shell.overlay'>
  & PropsLocale<'delta.liveshare'>
  & InjectFace<LiveShareDialogFace>
  & PropsHooks<LiveShareDialogFace['hooks']>

const ONE_HOUR = 60 * 60 * 1000
const ONE_DAY = 24 * ONE_HOUR

/** Render a sensitive sharing dialog only while the owner has selected a session. */
export function LiveShareDialog(props: LiveShareDialogProps) {
  const state = props.useState(value => value)
  if (state.request === null) return null
  return <LiveShareDialogForm key={state.request.sessionId} {...props} state={state} />
}

function LiveShareDialogForm(props: LiveShareDialogProps & { state: LiveShareDialogState }) {
  const { t, state } = props
  const [expiresInMs, setExpiresInMs] = useState(ONE_DAY)
  const [isPending, setIsPending] = useState(false)
  const [hasError, setHasError] = useState(false)
  const [created, setCreated] = useState<LiveShareCreated | null>(null)
  const [isCopied, setIsCopied] = useState(false)
  const [revoking, setRevoking] = useState<string | null>(null)
  const close = () => { if (!isPending && revoking === null) props.settle() }

  const create = () => {
    if (!state.configured || isPending) return
    setIsPending(true)
    setHasError(false)
    setCreated(null)
    props.create(state.request!.sessionId, expiresInMs).then(async value => {
      setCreated(value)
      setIsCopied(await props.copy(value.viewerUrl))
    }).catch(() => { setHasError(true) }).finally(() => { setIsPending(false) })
  }

  const revoke = (id: string) => {
    if (revoking !== null) return
    setRevoking(id)
    props.revoke(id, state.request!.sessionId)
      .then(() => { if (created?.id === id) setCreated(null) })
      .catch(() => { setHasError(true) })
      .finally(() => { setRevoking(null) })
  }

  const copy = () => {
    if (created === null) return
    void props.copy(created.viewerUrl).then(setIsCopied).catch(() => setIsCopied(false))
  }

  return (
    <Modal open onClose={close} closeLabel={t('close')} title={t('dialogTitle')}
      description={t('dialogDescription')}
      footer={(
        <>
          <Button variant="outline" disabled={isPending || revoking !== null} onClick={close}>{t('cancel')}</Button>
          {created === null && <Button variant="primary" disabled={!state.configured || isPending} onClick={create}>
            {isPending ? t('creating') : t('create')}
          </Button>}
        </>
      )}>
      <p><strong>{state.request!.title}</strong></p>
      <p>{t('trustBoundary')}</p>
      {!state.configured && <p role="status">{t('unavailableRelay')}</p>}
      {created === null && (
        <label>
          {t('expiresIn')}
          <select value={expiresInMs} disabled={!state.configured || isPending}
            onChange={event => { setExpiresInMs(Number(event.target.value)) }}>
            <option value={ONE_HOUR}>{t('oneHour')}</option>
            <option value={ONE_DAY}>{t('oneDay')}</option>
          </select>
        </label>
      )}
      {created !== null && (
        <div>
          <label htmlFor="delta-liveshare-viewer-link">{t('linkLabel')}</label>
          <input id="delta-liveshare-viewer-link" type="text" readOnly value={created.viewerUrl} />
          <Button variant="outline" onClick={copy}>{t('copy')}</Button>
          <p role="status">{isCopied ? t('copied') : t('copyFailed')}</p>
        </div>
      )}
      {hasError && <p role="alert">{t('unavailableRelay')}</p>}
      {state.listUnavailable && <p role="status">{t('listUnavailable')}</p>}
      <h4>{t('activeShares')}</h4>
      {state.shares.length === 0
        ? <p>{t('noShares')}</p>
        : <ul>
          {state.shares.map(share => (
            <li key={share.id}>
              {share.sessionId === undefined && <span>{t('orphanedShare')}</span>}
              <span>{t('expiresAt', { time: new Date(share.expiresAt).toLocaleString() })}</span>
              <span>{share.connected ? t('connected') : t('disconnected')}</span>
              <Button variant="outline" disabled={revoking !== null}
                onClick={() => { revoke(share.id) }}>{revoking === share.id ? t('revoking') : t('revoke')}</Button>
            </li>
          ))}
        </ul>}
    </Modal>
  )
}

