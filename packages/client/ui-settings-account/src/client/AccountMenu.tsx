/** Sidebar foot: account avatar with a hover card, plus settings and sign-in/out icon buttons. */
import { useEffect, useRef, useState } from 'react'
import {
  FishLogo, Toast, Tooltip, IconSettingsOutlineRegular, IconUserOutlineRegular,
} from '@deepseek-ai/dsh-client-ui-primitives'
import type { InjectFace, PropsLocale, PropsRuntime } from '@deepseek-ai/dsh-client-ui-slots'
import type { AccountSectionInjected } from './AccountSection.tsx'
import { SignOutDialog } from './SignOutDialog.tsx'
import { SignInDialog } from './SignInDialog.tsx'
import { LogoutIcon } from './LogoutIcon.tsx'
import { AccountAvatar } from './AccountAvatar.tsx'
import { formatBalance } from './formatBalance.ts'
import type {} from './account-ui.ts'
import css from './AccountMenu.module.css'

/** Account launcher composed by the settings shell. */
export type AccountMenuProps = PropsRuntime<'settings.launcher'> & PropsLocale<'settings.account'> & InjectFace<AccountSectionInjected>

/**
 * Foot row: 24px avatar (signed in) or Delta mark (signed out) on the left, icon actions on the right.
 * Balance shows only in the hover card; nothing is persisted. No spend endpoint exists, so no spend row.
 * @param props - sidebar geometry, settings navigation and account operations.
 * @returns account foot row.
 */
export function AccountMenu({
  subscribeSessionExpired, subscribeModelSignInRequired, wide, settingsShortcut, openSettings, openOnboarding, settingsOpen, leading,
  useAccount, useTheme, signOut, hasRunningAccountTasks, refreshAccount,
  showLogin, start, cancel, avatarFailed, t,
}: AccountMenuProps) {
  const settingsWasOpen = useRef(false)
  useEffect(() => {
    if (settingsOpen && !settingsWasOpen.current) void refreshAccount()
    settingsWasOpen.current = settingsOpen
  }, [refreshAccount, settingsOpen])
  const account = useAccount(state => state)
  const colorScheme = useTheme(snapshot => snapshot.active.colorScheme)
  const signedIn = account.view?.status === 'credential-stored'
  const [signInNotice, setSignInNotice] = useState(0)
  useEffect(() => subscribeModelSignInRequired?.(() => { setSignInNotice(value => value + 1) }), [subscribeModelSignInRequired])
  const [expiryNotice, setExpiryNotice] = useState(false)
  useEffect(() => subscribeSessionExpired?.(() => { setExpiryNotice(true) }), [subscribeSessionExpired])
  const [signOutImpact, setSignOutImpact] = useState<boolean | 'unknown'>()
  const requestSignOut = async () => {
    try { setSignOutImpact(await hasRunningAccountTasks()) }
    catch (_error) { setSignOutImpact('unknown') }
  }
  const wallets = account.details?.balance?.status === 'ready' ? account.details.balance.value : undefined
  const action = signedIn ? t('signOut') : t('signIn')
  return <div className={css.root} data-wide={wide}>
    {signInNotice > 0 && <Toast key={signInNotice} text={t('modelSignInRequired')} onDone={() => { setSignInNotice(0) }} />}
    {expiryNotice && <Toast text={t('sessionExpired')} onDone={() => { setExpiryNotice(false) }} />}
    <div className={css.identity} tabIndex={signedIn ? 0 : undefined}>
      <span className={css.avatar} data-avatar-status={account.avatar?.status}>
        {signedIn ? <AccountAvatar avatar={account.avatar} avatarFailed={avatarFailed} /> : <FishLogo size={24} />}
      </span>
      {signedIn && <div className={css.card} role="tooltip">
        <div className={css.cardRow}><span>{t('balance')}</span>
          <span>{wallets !== undefined && wallets.length > 0
            ? wallets.map(w => formatBalance(w.balance, w.currency === 'CNY' ? '¥' : '$')).join(' ')
            : t('balanceUnavailable')}</span></div>
      </div>}
    </div>
    {wide && <div className={css.actions}>
      {leading}
      <Tooltip label={t('settings')} shortcutKeys={settingsShortcut?.keys} side="top">
        <button type="button" className={css.iconBtn} aria-label={t('settings')} onClick={() => { openSettings() }}>
          <IconSettingsOutlineRegular size={14} />
        </button>
      </Tooltip>
      <Tooltip label={action} side="top">
        <button type="button" className={css.iconBtn} aria-label={action}
          onClick={() => { if (signedIn) void requestSignOut(); else void start().catch(() => undefined) }}>
          {signedIn ? <LogoutIcon /> : <IconUserOutlineRegular size={14} />}
        </button>
      </Tooltip>
    </div>}
    {account.loginVisible && !account.onboarding && <SignInDialog account={account} colorScheme={colorScheme}
      start={start} cancel={cancel} t={t}
      close={() => { showLogin(false) }} useApiKey={() => { showLogin(false); openOnboarding('deepseek-official') }} />}
    {signedIn && signOutImpact !== undefined && <SignOutDialog running={signOutImpact} signOut={signOut}
      close={() => { setSignOutImpact(undefined) }} t={t} />}
  </div>
}
