/** LiveShare session menu, explicit disclosure dialog, and Relay credentials page. */

import type {} from '@deepseek-ai/dsh-api-remotes/client'
import type {} from '@deepseek-ai/dsh-client-ui-context-menu/client'
import type {} from '@deepseek-ai/dsh-client-ui-plugin-manager/client'
import type {} from '@deepseek-ai/dsh-client-ui-renderer/client'
import type {} from '@deepseek-ai/dsh-client-ui-settings/client'
import type { Context } from '@deepseek-ai/cordis'
import { createSnapshotStore, type SnapshotStore } from '@deepseek-ai/dsh-client-store'
import { SettingsFormModel, settingsTextField } from '@deepseek-ai/dsh-client-ui-primitives'
import type { ConfigForm } from '@deepseek-ai/dsh-client-ui-settings/client'
import type { SessionId } from '@deepseek-ai/dsh-session/types'
import { en, zh, type LiveShareLocaleKey } from './locales.ts'
import { LiveShareCard } from './settings.tsx'
import type { LiveShareCardFace, LiveShareSettingsState } from './settings.tsx'
import { LiveShareDialog, type LiveShareDialogRequest, type LiveShareDialogFace } from './share-dialog.tsx'
import type { LiveShareEntry } from '../types.ts'

/** Locale namespace owned by this client contribution. */
export const NS = 'delta.liveshare'
/** Host profile settings namespace, matching the base bundle row id. */
const CONFIG_NS = 'delta-liveshare'
/** Relay credential never travels in config YAML. */
const OWNER_TOKEN_REF = 'DELTA_LIVESHARE_OWNER'

declare module '@deepseek-ai/dsh-client-ui-slots' {
  interface LocaleNamespaceMap { /** Relay setup and share copy. */ 'delta.liveshare': LiveShareLocaleKey }
}

/** Required services of the settings, menu, Remote, and overlay surfaces. */
export const inject = [
  'slots', 'locale', 'remote', 'remote.liveshare', 'remote.credentials', 'contextMenus', 'configForms',
]

interface MenuDialogState {
  readonly request: LiveShareDialogRequest | null
  readonly shares: readonly LiveShareEntry[]
  readonly configured: boolean
  readonly listUnavailable: boolean
}

/** Mount LiveShare UI and its bounded Host Remote calls. */
export function apply(ctx: Context): void {
  const t = ctx.locale.bind(NS)
  ctx.effect(() => ctx.locale.register(NS, { en, zh }), 'delta-liveshare: dictionaries')
  const settings = new LiveShareSettingsController(ctx.configForms.get(CONFIG_NS), ctx)
  ctx.effect(() => () => settings.dispose(), 'delta-liveshare: settings form')

  const menuState = createSnapshotStore<MenuDialogState>({ request: null, shares: [], configured: false, listUnavailable: false })
  const refreshShares = async (sessionId: SessionId): Promise<void> => {
    const response = await ctx.remote.liveshare.list()
    if (!response.ok) {
      menuState.set({ ...menuState.getSnapshot(), shares: [], configured: settings.state.getSnapshot().configured, listUnavailable: true })
      return
    }
    const state = settings.state.getSnapshot()
    menuState.set({ request: menuState.getSnapshot().request,
      shares: response.value.filter(item => item.sessionId === sessionId || item.sessionId === undefined),
      configured: state.configured, listUnavailable: false })
  }
  ctx.effect(() => ctx.contextMenus.register({
    id: 'delta-liveshare', target: 'session', order: 520,
    observe: () => [ctx.locale],
    resolve: target => [{
      id: 'share', label: t('menuShare'), separatorBefore: true,
      run: () => {
        menuState.set({ request: { sessionId: target.sessionId, title: target.displayTitle }, shares: [],
          configured: settings.state.getSnapshot().configured, listUnavailable: false })
        void refreshShares(target.sessionId)
      },
    }],
  }), 'delta-liveshare: session menu')

  const dialogFace: LiveShareDialogFace = {
    hooks: { state: menuState },
    settle: () => { menuState.set({ request: null, shares: [], configured: settings.state.getSnapshot().configured, listUnavailable: false }) },
    create: async (sessionId, expiresInMs) => {
      const response = await ctx.remote.liveshare.create({ sessionId, expiresInMs })
      if (!response.ok) throw new Error('LiveShare is unavailable')
      await refreshShares(sessionId)
      return response.value
    },
    revoke: async (id, sessionId) => {
      const response = await ctx.remote.liveshare.revoke({ id })
      if (!response.ok) throw new Error('LiveShare is unavailable')
      await refreshShares(sessionId)
    },
    copy: async url => await writeClipboard(url),
  }
  ctx.slots.inject('shell.overlay', () => ctx.slots.register({
    name: 'shell.overlay', id: 'delta-liveshare.dialog', locale: NS, inject: () => dialogFace,
  }, LiveShareDialog))

  const cardFace: LiveShareCardFace = {
    hooks: { state: settings.state },
    edit: (field, value) => settings.edit(field, value),
    reset: () => settings.reset(),
    save: () => settings.save(),
    discard: () => settings.discard(),
  }
  ctx.effect(() => ctx.configForms.whileServed([CONFIG_NS], () => ctx.slots.inject('plugins.item', () => ctx.slots.register({
    name: 'plugins.item', id: 'delta-liveshare', order: 50, label: () => t('title'), locale: NS, inject: () => cardFace,
  }, LiveShareCard))), 'delta-liveshare: settings page')
  ctx.effect(() => {
    const unsubscribe = settings.state.subscribe(() => {
      const current = menuState.getSnapshot()
      if (current.request !== null) menuState.set({ ...current, configured: settings.state.getSnapshot().configured })
    })
    return unsubscribe
  }, 'delta-liveshare: settings availability')
}

/** Settings page controller combines the staged Relay origin and write-only Owner Token control. */
class LiveShareSettingsController {
  readonly state: SnapshotStore<LiveShareSettingsState>
  private readonly form: SettingsFormModel<{ relayOrigin?: string }>
  private readonly unsubscribe: () => void
  private credential = { configured: false, writable: true }
  private disposed = false

  constructor(private readonly scope: ConfigForm<{ relayOrigin?: string }>, private readonly ctx: Context) {
    this.form = new SettingsFormModel(scope, [{
      ...settingsTextField('relayOrigin'),
      parse: text => {
        const value = text.trim()
        if (value === '') return { kind: 'clear' }
        try {
          const url = new URL(value)
          if (url.protocol !== 'https:' || url.username || url.password || url.pathname !== '/' || url.search || url.hash) return undefined
          return { kind: 'set', value: url.origin }
        } catch { return undefined }
      },
    }], [{
      field: 'ownerToken',
      write: async value => {
        const response = await this.ctx.remote.credentials.set(OWNER_TOKEN_REF, value)
        if (!response.ok) return false
        await this.refreshCredential()
        return this.credential.configured
      },
    }])
    this.state = this.form.bind(() => ({
      ...this.form.shell(),
      relayOrigin: this.form.field('relayOrigin'),
      ownerToken: this.form.field('ownerToken'),
      configured: this.credential.configured && (this.scope.getSnapshot().value?.relayOrigin?.trim() ?? '') !== '',
      credentialConfigured: this.credential.configured,
      credentialWritable: this.credential.writable,
    })) as SnapshotStore<LiveShareSettingsState>
    // The form's scope subscription publishes form edits. Credential changes have their own event.
    this.unsubscribe = ctx.remote.$on('credentials/reference-updated', ref => {
      if (ref === OWNER_TOKEN_REF) void this.refreshCredential()
    })
    void this.refreshCredential()
  }

  edit(field: 'relayOrigin' | 'ownerToken', value: string): void { this.form.actions().edit(field, value) }
  reset(): void { this.form.actions().resetField('relayOrigin') }
  save(): void { this.form.actions().save() }
  discard(): void { this.form.actions().discard() }
  dispose(): void { this.disposed = true; this.unsubscribe(); this.form.dispose() }

  private async refreshCredential(): Promise<void> {
    const response = await this.ctx.remote.credentials.describe([OWNER_TOKEN_REF])
    if (this.disposed || !response.ok) return
    const view = response.value[OWNER_TOKEN_REF]
    this.credential = { configured: view?.configured ?? false, writable: view?.writable ?? true }
    const current = this.state.getSnapshot()
    this.state.set({ ...current,
      configured: this.credential.configured && (this.scope.getSnapshot().value?.relayOrigin?.trim() ?? '') !== '',
      credentialConfigured: this.credential.configured,
      credentialWritable: this.credential.writable,
    })
  }
}

async function writeClipboard(value: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText !== undefined) { await navigator.clipboard.writeText(value); return true }
  } catch { /* The explicit link field remains available. */ }
  const input = document.createElement('textarea')
  input.value = value
  input.style.position = 'fixed'
  input.style.opacity = '0'
  document.body.append(input)
  input.select()
  try { return document.execCommand('copy') } catch { return false } finally { input.remove() }
}
