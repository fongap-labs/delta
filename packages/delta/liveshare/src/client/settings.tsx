/** Relay connection settings shown on the Plugins settings page. */

import {
  SettingsForm, SettingsSecretField, SettingsValueField,
  type SettingsFieldState, type SettingsFormShell,
} from '@deepseek-ai/dsh-client-ui-primitives'
import type { HostObservable, InjectFace, PropsHooks, PropsLocale, PropsRuntime } from '@deepseek-ai/dsh-client-ui-slots'
import { formLabels, type LiveShareLocaleKey } from './locales.ts'

/** Settings-card state; the token field never receives an existing secret. */
export interface LiveShareSettingsState extends SettingsFormShell {
  readonly relayOrigin: SettingsFieldState
  readonly ownerToken: SettingsFieldState
  readonly configured: boolean
  readonly credentialConfigured: boolean
  readonly credentialWritable: boolean
}

/** Browser-side actions and selector for the settings page. */
export interface LiveShareCardFace {
  hooks: { state: HostObservable<LiveShareSettingsState> }
  edit: (field: 'relayOrigin' | 'ownerToken', value: string) => void
  reset: () => void
  save: () => void
  discard: () => void
}

/** Complete settings page props. */
export type LiveShareCardProps = PropsRuntime<'plugins.item'>
  & PropsLocale<'delta.liveshare'>
  & InjectFace<LiveShareCardFace>
  & PropsHooks<LiveShareCardFace['hooks']>

/** Render LiveShare setup controls and the shared save boundary. */
export function LiveShareCard(props: LiveShareCardProps) {
  const state = props.useState(value => value)
  if (props.view === 'summary') return props.t('description')
  const disabled = !state.writable || state.saving
  return (
    <SettingsForm labels={formLabels(props.t as (key: LiveShareLocaleKey) => string)} state={state}
      onSave={props.save} onDiscard={props.discard}>
      <section aria-labelledby="delta-liveshare-settings-title">
        <h4 id="delta-liveshare-settings-title">{props.t('title')}</h4>
        <SettingsValueField id="delta-liveshare-relay-origin" label={props.t('relayOrigin')}
          hint={props.t('relayOriginHint')} overriddenLabel={props.t('overridden')} resetLabel={props.t('reset')}
          invalidLabel={props.t('invalidRelay')} disabled={disabled} {...state.relayOrigin}
          onEdit={value => props.edit('relayOrigin', value)} onReset={props.reset} />
        <SettingsSecretField id="delta-liveshare-owner-token" label={props.t('ownerToken')}
          hint={props.t('ownerTokenHint')} configured={state.credentialConfigured}
          stateLabel={state.credentialConfigured ? props.t('credentialSet') : props.t('credentialUnset')}
          disabled={disabled || !state.credentialWritable} text={state.ownerToken.text}
          onEdit={value => props.edit('ownerToken', value)} />
      </section>
    </SettingsForm>
  )
}
