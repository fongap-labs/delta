/**
 * DSH native Settings -> Delta Code page (also appears on the plugin's detail page).
 * Uses the official settings.section slot and ConfigForms; no DOM patching.
 * Config stores credential REFERENCES only. PATs flow solely through the Host
 * credentials Remote, and are never read back into a client config snapshot.
 */
// Self-contained classic script: DSH may evaluate multiple clients in one lexical scope.
;(() => {
interface ReactApi {
  createElement(type: string | ((props: any) => unknown), props?: Record<string, unknown> | null, ...children: unknown[]): unknown
  useState<T>(init: T | (() => T)): [T, (value: T | ((old: T) => T)) => void]
  useEffect(fn: () => void | (() => void), deps?: readonly unknown[]): void
}
type Op = { op: 'set'; path: string[]; value: unknown } | { op: 'unset'; path: string[] }
type Account = { owner: string; tokenEnv: string; repositories: string; sourceIndex: number; uiKey: string }
type Server = { language: string; command: string; args: string }
interface Draft {
  enabled: boolean
  autoRouting: boolean
  workspaceRoot: string
  gitEnabled: boolean
  gitCommand: string
  githubEnabled: boolean
  githubAllowWrites: boolean
  allowWorktreeWrites: boolean
  allowCommands: boolean
  fsWriteContract: 'legacy-overwrite' | 'native-unguarded' | 'guarded-intent'
  systemPromptText: string
  accounts: Account[]
  servers: Server[]
}
interface ConfigState { status: string; value?: any; revision?: number; writable: boolean }
interface CredentialInfo { configured: boolean; writable: boolean }
interface CredentialsRemote {
  describe(refs: string[]): Promise<Record<string, CredentialInfo>>
  set(ref: string, value: string): Promise<void>
  unset(ref: string): Promise<void>
}
interface ConfigPageProps {
  view: 'page' | 'summary'
  form?: { state: ConfigState; mutate(ops: Op[], revision?: number): Promise<boolean> }
  credentials?: CredentialsRemote
}
const NAME = '@fongap-labs/delta-code'
const OWNER = /^[a-z0-9](?:[a-z0-9-]{0,37}[a-z0-9])?$/i
const ENV = /^[A-Z_][A-Z0-9_]*$/
const REPOSITORY = /^[a-z0-9](?:[a-z0-9-]{0,37}[a-z0-9])?\/(?!\.{1,2}$)[a-z0-9_.-]{1,100}$/i
const defaultGithubTokenRef = (owner: string) => owner && OWNER.test(owner)
  ? 'DSH_DELTA_CODE_GITHUB_' + owner.toUpperCase().replace(/-/g, '_') + '_TOKEN' : ''
let nextAccountRow = 0
const repositoryList = (value: string) => value.split(',').map(item => item.trim()).filter(Boolean)
const inset: Record<string, string> = { display: 'grid', gap: '10px', margin: '12px 0' }
const inputStyle: Record<string, string> = {
  boxSizing: 'border-box', width: '100%', minWidth: '0', padding: '9px 11px',
  border: '1px solid var(--dsw-alias-border-l3, #aaa)', borderRadius: '9px',
  background: 'var(--dsw-alias-bg-layer-1, transparent)',
  color: 'var(--dsw-alias-label-primary, inherit)', font: 'inherit',
}
const actionStyle: Record<string, string> = {
  padding: '8px 12px', cursor: 'pointer', borderRadius: '9px',
  border: '1px solid var(--dsw-alias-border-l2, #aaa)', color: 'inherit',
  background: 'var(--dsw-alias-bg-layer-2, transparent)',
}
function readDraft(value: any): Draft {
  const git = value?.git ?? {}, github = value?.github ?? {}
  const execution = value?.execution ?? {}, lsp = value?.lsp ?? {}
  const defaults: Server[] = [
    { language: 'typescript', command: 'typescript-language-server', args: '--stdio' },
    { language: 'javascript', command: 'typescript-language-server', args: '--stdio' },
    { language: 'python', command: 'pyright-langserver', args: '--stdio' },
    { language: 'rust', command: 'rust-analyzer', args: '--stdio' },
    { language: 'go', command: 'gopls', args: 'serve' },
  ]
  return {
    enabled: value?.enabled !== false,
    autoRouting: value?.autoRouting !== false,
    workspaceRoot: value?.workspaceRoot ?? '',
    gitEnabled: git.enabled !== false, gitCommand: git.command ?? 'git',
    githubEnabled: github.enabled !== false, githubAllowWrites: github.allowWrites === true,
    allowWorktreeWrites: execution.allowWorktreeWrites === true,
    allowCommands: execution.allowCommands === true,
    fsWriteContract: execution.fsWriteContract === 'guarded-intent' ? 'guarded-intent'
      : execution.fsWriteContract === 'native-unguarded' ? 'native-unguarded' : 'legacy-overwrite',
    systemPromptText: value?.systemPromptText ?? '',
    accounts: (github.accounts ?? []).map((item: any, index: number) => ({
      owner: item.owner ?? '', tokenEnv: item.tokenEnv ?? '',
      repositories: Array.isArray(item.repositories) ? item.repositories.join(', ') : '',
      sourceIndex: index, uiKey: 'saved-' + index,
    })),
    servers: Array.isArray(lsp.servers)
      ? lsp.servers.map((server: any) => ({
          language: server.language ?? '', command: server.command ?? '',
          args: Array.isArray(server.args) ? server.args.join(' ') : '',
        }))
      : defaults,
  }
}
function validation(draft: Draft, _value: any): string | undefined {
  const owners = new Set<string>()
  const repositories = new Set<string>()
  const credentialRefs = new Set<string>()
  for (const account of draft.accounts) {
    const owner = account.owner.trim().toLowerCase()
    if (!OWNER.test(owner)) return 'GitHub owner must be a valid username or organization.'
    if (owners.has(owner)) return 'GitHub owner 不允许重复：' + owner
    owners.add(owner)
    if (!account.tokenEnv) return owner + '：需要 Token 环境变量。'
    if (!ENV.test(account.tokenEnv)) return owner + ': invalid token environment variable name.'
    if (credentialRefs.has(account.tokenEnv)) return '一个 Token 引用只能绑定一个 GitHub 账号或组织。'
    credentialRefs.add(account.tokenEnv)
    const assigned = repositoryList(account.repositories)
    for (const item of assigned) {
      if (!REPOSITORY.test(item)) return 'GitHub repository must be owner/name: ' + item
      if ((item.split('/')[0] ?? '').toLowerCase() !== owner) return '仓库必须属于对应的 GitHub 账号或组织：' + item
      const key = item.toLowerCase()
      if (repositories.has(key)) return 'Duplicate GitHub repository binding: ' + key
      repositories.add(key)
    }
  }
  if (!draft.gitCommand.trim()) return 'Git 命令不能为空。'
  for (const server of draft.servers) {
    if (!server.language.trim() || !server.command.trim()) return '语言服务器必须填写语言和命令。'
  }
  return undefined
}
function operations(draft: Draft, value: any): Op[] {
  const original = readDraft(value)
  const ops: Op[] = []
  const set = (path: string[], val: unknown) => { ops.push({ op: 'set', path, value: val }) }
  const scalars: Array<[keyof Draft, string[]]> = [
    ['enabled', ['enabled']], ['autoRouting', ['autoRouting']], ['workspaceRoot', ['workspaceRoot']],
    ['gitEnabled', ['git', 'enabled']], ['gitCommand', ['git', 'command']],
    ['githubEnabled', ['github', 'enabled']], ['githubAllowWrites', ['github', 'allowWrites']],
    ['allowWorktreeWrites', ['execution', 'allowWorktreeWrites']],
    ['allowCommands', ['execution', 'allowCommands']],
    ['fsWriteContract', ['execution', 'fsWriteContract']],
    ['systemPromptText', ['systemPromptText']],
  ]
  for (const [name, path] of scalars) {
    if (draft[name] !== original[name]) set(path, draft[name])
  }
  const current = value?.github?.accounts ?? []
  const seen = new Set(draft.accounts.filter(a => a.sourceIndex >= 0).map(a => a.sourceIndex))
  for (const a of draft.accounts.filter(a => a.sourceIndex >= 0)) {
    const prior = current[a.sourceIndex]
    const prefix = ['github', 'accounts', String(a.sourceIndex)]
    if (a.owner.trim() !== prior?.owner) set([...prefix, 'owner'], a.owner.trim())
    if (a.tokenEnv !== (prior?.tokenEnv ?? '')) {
      if (a.tokenEnv) {
        set([...prefix, 'tokenEnv'], a.tokenEnv)
      } else {
        ops.push({ op: 'unset', path: [...prefix, 'tokenEnv'] })
      }
    }
    const assigned = repositoryList(a.repositories)
    if (JSON.stringify(assigned) !== JSON.stringify(prior?.repositories ?? [])) {
      if (assigned.length) set([...prefix, 'repositories'], assigned)
      else ops.push({ op: 'unset', path: [...prefix, 'repositories'] })
    }
  }
  const removed = current.map((_a: unknown, i: number) => i).filter((i: number) => !seen.has(i))
  for (const index of removed.reverse()) ops.push({ op: 'unset', path: ['github', 'accounts', String(index)] })
  let index = current.length - removed.length
  for (const account of draft.accounts.filter(a => a.sourceIndex < 0)) {
    set(['github', 'accounts', String(index++)], {
      owner: account.owner.trim(), tokenEnv: account.tokenEnv,
      ...(repositoryList(account.repositories).length
        ? { repositories: repositoryList(account.repositories) } : {}),
    })
  }
  const servers = draft.servers.map(server => ({
    language: server.language.trim(), command: server.command.trim(),
    args: server.args.trim() ? server.args.trim().split(/\s+/) : [],
  }))
  if (JSON.stringify(servers) !== JSON.stringify(original.servers.map(s => ({
    language: s.language, command: s.command, args: s.args.trim() ? s.args.trim().split(/\s+/) : [],
  })))) set(['lsp', 'servers'], servers)
  return ops
}
function installClient(require: (id: string) => unknown) {
  const React = require('react') as ReactApi
  const h = React.createElement
  const uiLang = () => /^zh/i.test(document.documentElement.lang || navigator.language || 'zh')
  const t = (zh: string, en: string) => uiLang() ? zh : en
  const section = (title: string, ...children: unknown[]) => h('section', {
    style: { margin: '22px 0', borderTop: '1px solid var(--dsw-alias-border-l2, #ddd)', paddingTop: '15px' },
  }, h('h3', { style: { fontSize: '15px', margin: '0 0 10px' } }, title), ...children)
  const hint = (text: string) => h('p', { style: { fontSize: '12px', opacity: 0.75, lineHeight: 1.5, margin: '6px 0 10px' } }, text)

  function Page(props: ConfigPageProps) {
    const state = props.form?.state
    const [draft, setDraft] = React.useState<Draft>(() => readDraft(state?.value))
    const [saving, setSaving] = React.useState(false)
    const [notice, setNotice] = React.useState('')
    // Secrets live only in this transient input state. They never enter readDraft,
    // operations(), Host ConfigForm or the plugin's persisted settings.
    const [tokenInputs, setTokenInputs] = React.useState<Record<string, string>>({})
    const [credentialStates, setCredentialStates] = React.useState<Record<string, CredentialInfo>>({})
    const [credentialNotice, setCredentialNotice] = React.useState('')
    const refsKey = draft.accounts.map(a => a.tokenEnv).filter(Boolean).join('|')
    React.useEffect(() => {
      setDraft(previous => {
        const refreshed = readDraft(state?.value)
        for (const account of refreshed.accounts) {
          const before = previous.accounts.find(candidate => candidate.owner === account.owner &&
            candidate.tokenEnv === account.tokenEnv)
          if (before) account.uiKey = before.uiKey
        }
        return refreshed
      })
      setNotice('')
    }, [state?.revision])
    React.useEffect(() => {
      let isActive = true
      const refs = [...new Set(['GITHUB_TOKEN', ...draft.accounts.map(a => a.tokenEnv)].filter(ref => ENV.test(ref)))]
      if (!refs.length) { setCredentialStates({}); return () => { isActive = false } }
      if (!props.credentials?.describe) {
        setCredentialNotice(t('当前 Host 未提供凭据服务。', 'Host credential service is unavailable.'))
        return () => { isActive = false }
      }
      // Host caps one describe call at 64 refs. No secret values are returned.
      const chunks = Array.from({ length: Math.ceil(refs.length / 64) }, (_, i) => refs.slice(i * 64, (i + 1) * 64))
      void Promise.all(chunks.map(chunk => props.credentials!.describe(chunk))).then(views => {
        if (!isActive) return
        setCredentialStates(Object.assign({}, ...views))
        setCredentialNotice('')
      }).catch(() => { if (isActive) setCredentialNotice(t('凭据状态读取失败，请检查 Host 连接。', 'Could not read credential status.')) })
      return () => { isActive = false }
    }, [refsKey, props.credentials])
    if (props.view === 'summary') return t('Delta Code · 原生 GitHub 凭据与多账号', 'Delta Code · Native GitHub credentials and accounts')
    if (!props.form || state?.status !== 'ready') return hint(t('当前配置尚未由 Host 加载。', 'Host configuration is not available.'))
    const disabled = !state.writable || saving
    const field = (label: string, value: string, change: (v: string) => void, isSecret = false, placeholder = '') =>
      h('label', { style: { display: 'grid', gap: '5px', fontSize: '13px', minWidth: 0 } },
        h('span', null, label),
        h('input', { style: inputStyle, type: isSecret ? 'password' : 'text', value,
          autoComplete: isSecret ? 'new-password' : 'off', placeholder, disabled,
          onChange: (event: any) => change(event.target.value) }))
    const toggle = (label: string, isChecked: boolean, change: (v: boolean) => void) =>
      h('label', { style: { display: 'flex', gap: '8px', alignItems: 'center', fontSize: '13px' } },
        h('input', { type: 'checkbox', checked: isChecked, disabled, onChange: (e: any) => change(e.target.checked) }), label)
    const patch = (value: Partial<Draft>) => setDraft(old => ({ ...old, ...value }))
    const updateAccount = (idx: number, prop: 'owner' | 'repositories', value: string) => setDraft(old => ({
      ...old, accounts: old.accounts.map((a, i) => {
        if (i !== idx) return a
        const updated = { ...a, [prop]: value }
        // New rows get a deterministic reference; the user never needs to
        // know what an environment variable is. Existing references remain stable.
        if (prop === 'owner' && a.sourceIndex < 0) updated.tokenEnv = defaultGithubTokenRef(value.trim())
        return updated
      }),
    }))
    const updateServer = (idx: number, prop: keyof Server, value: string) => setDraft(old => ({
      ...old, servers: old.servers.map((s, i) => i === idx ? { ...s, [prop]: value } : s),
    }))
    const save = async () => {
      const error = validation(draft, state.value)
      if (error) { setNotice(error); return }
      const unconfigured = draft.accounts.find(account => account.sourceIndex < 0 &&
        !tokenInputs[account.uiKey]?.trim() && credentialStates[account.tokenEnv]?.configured !== true)
      if (unconfigured) {
        setNotice(t('新账号需要填写访问令牌。', 'Enter an access token for the new account.'))
        return
      }
      const ops = operations(draft, state.value)
      const updates = draft.accounts.flatMap(a => {
        const token = tokenInputs[a.uiKey]?.trim()
        return token ? [{ ref: a.tokenEnv, token }] : []
      })
      if (!ops.length && !updates.length) {
        setNotice(t('没有待保存的变更。', 'No changes to save.')); return
      }
      if (updates.length && !props.credentials?.set) {
        setNotice(t('当前 Host 不支持安全保存 Token，已阻止保存。', 'Host secure credential saving is unavailable.'))
        return
      }
      setSaving(true)
      setNotice('')
      let isConfigSaved = false
      try {
        if (ops.length) {
          const ok = await props.form!.mutate(ops, state.revision)
          if (!ok) {
            setNotice(t('保存被 Host 拒绝。请刷新后重试，Token 未发送。',
              'Host rejected the configuration. Refresh and retry; tokens were not sent.'))
            return
          }
          isConfigSaved = true
        }
        // Never send token values through settings.mutate or its revision log.
        for (const update of updates) await props.credentials!.set(update.ref, update.token)
        setTokenInputs({})
        if (updates.length) {
          const refs = [...new Set(updates.map(update => update.ref))]
          const descriptions = await props.credentials!.describe(refs)
          setCredentialStates(old => ({ ...old, ...descriptions }))
        }
        setNotice(t('已保存。Token 仅存于 Host 凭据服务，界面不会回显。',
          'Saved. Token values are held by Host credentials and never echoed.'))
      } catch {
        setNotice(isConfigSaved
          ? t('账号配置已保存，但 Token 写入或状态读取失败。请检查凭据服务并重试。',
              'Account settings saved, but token write or status lookup failed. Check credentials and retry.')
          : t('保存失败，请检查 Host 连接及日志。', 'Save failed. Check the Host connection.'))
      } finally { setSaving(false) }
    }
    return h('div', { style: { display: 'grid', gap: '6px', minWidth: 0, paddingBottom: '16px' }, 'data-code-worker-settings': 'true' },
      hint(t('设置通过 DSH 原生配置服务保存。关闭页面不会保存草稿；执行权限默认关闭。',
        'Saved through the native DSH configuration service. Changes are staged; execution is off by default.')),
      section(t('GitHub 连接', 'GitHub connections'),
        toggle(t('启用 GitHub 工具', 'Enable GitHub tools'), draft.githubEnabled, v => patch({ githubEnabled: v })),
        toggle(t('允许创建分支、提交和 PR（默认关闭）', 'Allow branches, commits and PRs (off by default)'), draft.githubAllowWrites,
          v => patch({ githubAllowWrites: v })),
        hint(t('一个 Token 只绑定一个账号或组织，该归属下仓库数量不限，实际授权由 GitHub 决定。原生 GITHUB_TOKEN 仅用于其登录用户自己的仓库，组织仓库须单独配置对应账号和 Token。',
          'Each token binds to one account or organization, with unlimited repositories under that owner. Native GITHUB_TOKEN is limited to its authenticated user repositories; organizations need their own configured credentials.')),
        hint(credentialStates.GITHUB_TOKEN?.configured
          ? t('原生 GITHUB_TOKEN 已配置（实际仓库权限由 GitHub 验证）。', 'Native GITHUB_TOKEN configured (GitHub verifies repository permissions).')
          : t('原生 GITHUB_TOKEN 尚未在 Host 凭据服务中配置。', 'Native GITHUB_TOKEN is not configured in Host credentials.')),
        h('a', { href: 'https://github.com/settings/personal-access-tokens/new', target: '_blank', rel: 'noopener noreferrer', style: { fontSize: '12px' } },
          t('在 GitHub 创建访问令牌', 'Create a GitHub access token')),
        hint(t('安全提示：Token 输入已隐藏且不会写入插件配置。静态加密能力由 Host 凭据提供方决定；默认本地凭据文件并非加密文件。',
          'Security: tokens are masked and excluded from plugin settings. At-rest encryption depends on the Host provider; the default local credential file is not encrypted.')),
        credentialNotice && hint(credentialNotice),
        ...draft.accounts.map((a, i) => h('div', { key: a.sourceIndex >= 0 ? 'existing-' + a.sourceIndex : 'new-' + i,
          style: { ...inset, padding: '12px', border: '1px solid var(--dsw-alias-border-l2, #ddd)', borderRadius: '10px' } },
          field(t('GitHub 用户名或组织名', 'GitHub username or organization'), a.owner, v => updateAccount(i, 'owner', v)),
          h('div', { style: { fontSize: '12px', opacity: 0.8 } },
            credentialStates[a.tokenEnv] === undefined
              ? t('正在确认凭据状态', 'Checking credential status')
              : credentialStates[a.tokenEnv]?.configured
                ? t('Token 已保存（尚未验证仓库权限）', 'Token saved (repository permissions not verified)')
                : t('Token 尚未配置', 'Token not configured')),
          field(t('访问令牌（Token，留空表示不更换）', 'Access token (leave blank to keep existing)'),
            tokenInputs[a.uiKey] ?? '', value => setTokenInputs(old => ({ ...old, [a.uiKey]: value })),
            true, t('粘贴 GitHub Token', 'Paste GitHub token')),
          field(t('本账号下的仓库（可选，无数量上限）', 'Repositories under this owner (optional, unlimited)'),
            a.repositories, v => updateAccount(i, 'repositories', v), false, 'org/backend, org/frontend'),
          h('details', null, h('summary', { style: { fontSize: '12px', cursor: 'pointer' } },
            t('高级：凭据引用', 'Advanced: credential reference')),
            h('code', { style: { fontSize: '11px', overflowWrap: 'anywhere' } }, a.tokenEnv || t('填写用户名后自动生成', 'Generated after entering a username'))),
          h('button', { type: 'button', disabled, style: actionStyle,
            onClick: () => {
              patch({ accounts: draft.accounts.filter((_, j) => j !== i) })
              setTokenInputs(old => { const next = { ...old }; delete next[a.uiKey]; return next })
            } },
            t('删除账号', 'Remove account')))),
        h('button', { type: 'button', disabled, style: actionStyle,
          onClick: () => patch({ accounts: [...draft.accounts, { owner: '', tokenEnv: '', repositories: '', sourceIndex: -1,
            uiKey: 'new-' + (++nextAccountRow) }] }) },
          t('添加 GitHub 账号', 'Add GitHub account'))),
      h('details', { style: { marginTop: '8px' } },
        h('summary', { style: { cursor: 'pointer', fontWeight: 600, padding: '10px 0' } },
          t('高级设置：Git、执行权限与语言服务器', 'Advanced: Git, execution and language servers')),
      section(t('基本与 Git', 'General & Git'),
        toggle(t('启用编程工具', 'Enable coding tools'), draft.enabled, v => patch({ enabled: v })),
        toggle(t('自动路由编程请求', 'Auto-route coding requests'), draft.autoRouting, v => patch({ autoRouting: v })),
        h('div', { style: inset },
          field(t('工作区路径（留空跟随 Host）', 'Workspace root (blank = Host)'), draft.workspaceRoot, v => patch({ workspaceRoot: v })),
          toggle(t('启用 Git', 'Enable Git'), draft.gitEnabled, v => patch({ gitEnabled: v })),
          field(t('Git 可执行文件', 'Git command'), draft.gitCommand, v => patch({ gitCommand: v })),
        )),
      section(t('执行与写入权限', 'Execution & write permissions'),
        toggle(t('允许创建 Worktree', 'Allow worktree writes'), draft.allowWorktreeWrites, v => patch({ allowWorktreeWrites: v })),
        toggle(t('允许执行项目脚本', 'Allow project commands'), draft.allowCommands, v => patch({ allowCommands: v })),
        h('label', { style: { display: 'grid', gap: '5px', fontSize: '13px' } },
          h('span', null, t('DSH 文件写入协议', 'DSH file write contract')),
          h('select', { style: inputStyle, disabled, value: draft.fsWriteContract,
            onChange: (e: any) => patch({ fsWriteContract: e.target.value }) },
            h('option', { value: 'legacy-overwrite' }, t('兼容旧式覆盖（默认）', 'Legacy overwrite (default)')),
            h('option', { value: 'native-unguarded' }, t('原生无条件写入', 'Native unconditional write')),
            h('option', { value: 'guarded-intent' }, t('原生版本保护写入', 'Native guarded write')))),
        hint(t('请选择宿主已提供的写入接口语义，不按 DSH 版本判断；不会通过试写探测，拒绝自动降级。',
          'Select the supported Host write semantics, not a DSH version. No probing writes or unsafe fallback.')),
        hint(t('执行脚本仅适用于可信工作区。', 'Only enable project commands in trusted workspaces.'))),
      section(t('语言服务器', 'Language servers'),
        ...draft.servers.map((s, i) => h('div', { key: 'server-' + i,
          style: { ...inset, padding: '12px', border: '1px solid var(--dsw-alias-border-l2, #ddd)', borderRadius: '10px' } },
          field(t('语言', 'Language'), s.language, v => updateServer(i, 'language', v)),
          field(t('启动命令', 'Executable'), s.command, v => updateServer(i, 'command', v)),
          field(t('参数（空格分隔）', 'Arguments (space-separated)'), s.args, v => updateServer(i, 'args', v)),
          h('button', { type: 'button', disabled, style: actionStyle,
            onClick: () => patch({ servers: draft.servers.filter((_, j) => j !== i) }) },
            t('移除语言服务器', 'Remove language server')))),
        h('button', { type: 'button', disabled, style: actionStyle,
          onClick: () => patch({ servers: [...draft.servers, { language: '', command: '', args: '' }] }) },
          t('添加语言服务器', 'Add language server'))),
      section(t('高级', 'Advanced'),
        field(t('系统提示词补充（留空使用默认值）', 'Additional system prompt (blank for default)'),
          draft.systemPromptText, v => patch({ systemPromptText: v })))
      ),
      notice && h('p', { role: 'status', style: { fontSize: '13px', overflowWrap: 'anywhere' } }, notice),
      !state.writable && hint(t('当前 Profile 为只读，无法保存。', 'The active profile is read-only.')),
      h('div', { style: { display: 'flex', gap: '10px', marginTop: '10px' } },
        h('button', { type: 'button', disabled, style: actionStyle,
          onClick: () => { setDraft(readDraft(state.value)); setTokenInputs({}); setNotice('') } },
          t('放弃修改', 'Discard')),
        h('button', { type: 'button', disabled, onClick: () => { void save() },
          style: { ...actionStyle, background: 'var(--dsw-alias-brand-primary, #286F78)', color: 'white' } },
          saving ? t('保存中…', 'Saving…') : t('保存配置', 'Save settings'))),
    )
  }
  return {
    inject: ['slots', 'configForms', 'remote', 'remote.credentials'],
    apply(ctx: any) {
      // The plugin row's Cordis config namespace is its patch id, not its
      // package name. The Settings section uses the same revision-fenced form.
      const form = ctx.configForms.get('dsh-delta-code')
      const credentials: CredentialsRemote = ctx.remote.credentials
      function NativePage() {
        const [snapshot, setSnapshot] = React.useState<ConfigState>(() => form.getSnapshot())
        React.useEffect(() => form.subscribe(() => setSnapshot(form.getSnapshot())), [])
        return h(Page, { view: 'page', form: {
          state: snapshot, mutate: (ops: Op[], revision?: number) => form.mutate(ops, revision),
        }, credentials })
      }
      // settings.section is shared by native Settings and the Plugins page.
      // Do not register plugins.row.config as well, which would hide this page
      // in the plugin manager and create two competing configuration surfaces.
      ctx.effect(() => ctx.slots.inject('settings.section', () => ctx.slots.register({
        name: 'settings.section', id: 'delta-code', order: 65,
        label: () => t('Delta Code', 'Delta Code'),
      }, NativePage)), 'dsh-delta-code: native settings section')
    },
  }
}
(window as unknown as { __ModuleLoader__: { load(entry: { id: string; factory: (require: (id: string) => unknown) => unknown }): void } })
  .__ModuleLoader__.load({ id: NAME, factory: installClient })

})()
