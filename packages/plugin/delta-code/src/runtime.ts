import type { CodeWorkerConfig, GitHubAccountConfig, LspServerConfig, Settings } from './types.js'
import { normalizeLanguageConventions } from './consistency.js'
import { DEFAULT_LSP_SERVERS } from './lsp-defaults.js'

export const TOOL_PREFIX = 'code_'
export const DEFAULT_GUIDANCE = [
  '## DSH Code Worker',
  'Prefer DSH native editing tools for ordinary changes. Use code_multi_edit only when a multi-file preflight or guarded batch edit offers a concrete benefit; avoid redundant tool calls.',
  'For coding work, choose procedures by user intent and actual repository evidence, not just keyword route hints. For unfamiliar or multi-domain work, inspect the read-only code_capabilities directory and use only relevant scenarios; load Skill bodies through the native Host provider when needed. This neither grants permissions nor runs another model.',
  'For high-risk fixes, consider code-quality-strength and code-diagnostic-readiness. Claim mutation or runtime evidence only when the approved project runner actually executed. Submit reusable experience through the existing governed Skill admission process.',
  'Use DSH native Git tools where available; code_git_status/code_git_diff are optional for Git worktrees, never prerequisites for ordinary code edits.',
  'Locate the actual implementation and running UI component before editing. Search narrowly with native Host tools; in large Git workspaces use code_workspace_map for tracked paths. Never start with unbounded **/* glob or enumerate build-output trees.',
  'If a Code Worker tool fails due to missing Host services or permission rejection, do not retry identical arguments. Switch to the permitted native Host file search or ask for the right repository; never bypass Host approval or read a giant file index.',
  'Run `code_verify` only when explicitly enabled; report test failures instead of claiming success.',
  'For substantial tasks keep the authoritative task list in DSH native todo_write. On continuation, inspect pending/in_progress items instead of blindly repeating work.',
  'On resumed multi-step coding work, consult native Todo/session history first; use code_resume only if native progress is missing and the Host exposes a compatible checkpoint. Reconcile real repository state before replaying edits.',
  'Use code_delivery_gate only for substantial Git-backed work when Host script execution is authorized, declared scripts exist, and implementation Todos are complete. Never create a self-referential Todo to pass the gate. In non-Git projects or when execution is denied, rely on permitted native evidence and explicitly report verification as incomplete; do not repeatedly call the gate or claim a pass.',
  'Use `code_git_worktree` for isolated branches when worktree creation is enabled. Pass the task worktree as cwd to context, Git, and verification tools; prepend it to file paths for multi-edit.',
  'Use `code_github_connection` to verify a repository-scoped GitHub account before remote operations. When CI fails, call code_github_jobs for the failed job ID, then code_github_job_logs to read the actual error; empty check summaries and absent artifacts do not mean logs are unavailable.',
  'Each GitHub credential belongs to exactly one account or organization. Unlimited repositories under that owner are allowed subject to GitHub permissions. Never route credentials across owners or expose tokens.',
  'GitHub write tools create changes on feature branches and open pull requests; do not bypass merge gates.',
  'Use code_lsp_probe only to diagnose missing language-server availability; it is not a substitute for Host LSP diagnostics.',
  'Always pass paths relative to the workspace root; absolute paths outside the workspace are rejected by the sandbox.',
].join('\n')

/** Render an unknown thrown value the way the JavaScript implementation did. */
export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message
  const message = (error as { message?: unknown } | null)?.message
  return message ? String(message) : String(error)
}

const GITHUB_REPOSITORY_BINDING = /^[a-z0-9](?:[a-z0-9-]{0,37}[a-z0-9])?\/(?!\.{1,2}$)[a-z0-9_.-]{1,100}$/i

/** Validate account routing without ever logging or returning a token to the agent. */
export function validateGitHubAccounts(accounts: unknown): GitHubAccountConfig[] {
  if (accounts === undefined) return []
  if (!Array.isArray(accounts)) throw new Error('github.accounts must be an array')
  const owners = new Set<string>()
  const credentialRefs = new Set<string>()
  const repositoryBindings = new Map<string, string>()
  const result: GitHubAccountConfig[] = []
  for (const entry of accounts) {
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
      throw new Error('invalid GitHub account')
    }
    const account = entry as Partial<GitHubAccountConfig>
    const owner = typeof account.owner === 'string' ? account.owner.trim().toLowerCase() : ''
    if (!/^[a-z0-9](?:[a-z0-9-]{0,37}[a-z0-9])?$/.test(owner)) {
      throw new Error('github.accounts[].owner must be a GitHub username or organization')
    }
    if (owners.has(owner)) throw new Error('duplicate GitHub owner mapping: ' + owner)
    owners.add(owner)
    if (Object.prototype.hasOwnProperty.call(entry, 'token')) {
      throw new Error('github.accounts[].token is not supported; migrate the credential to tokenEnv')
    }
    const tokenEnv = typeof account.tokenEnv === 'string' ? account.tokenEnv.trim() : ''
    if (!tokenEnv) {
      throw new Error('github.accounts[].tokenEnv is required for GitHub owner ' + owner)
    }
    if (!/^[A-Z_][A-Z0-9_]*$/.test(tokenEnv)) {
      throw new Error('github.accounts[].tokenEnv must name an uppercase environment variable')
    }
    if (credentialRefs.has(tokenEnv)) throw new Error('GitHub tokenEnv must be bound to only one account or organization')
    credentialRefs.add(tokenEnv)
    const rawRepositories = account.repositories === undefined ? [] : account.repositories
    if (!Array.isArray(rawRepositories)) {
      throw new Error('github.accounts[].repositories must be an array of repository names')
    }
    const repositories: string[] = []
    const localRepositories = new Set<string>()
    for (const rawRepository of rawRepositories) {
      const repository = typeof rawRepository === 'string' ? rawRepository.trim().toLowerCase() : ''
      if (!GITHUB_REPOSITORY_BINDING.test(repository)) {
        throw new Error('github.accounts[].repositories entries must be owner/repository')
      }
      if (repository.split('/')[0] !== owner) throw new Error('GitHub repository binding must match its account owner: ' + repository)
      if (localRepositories.has(repository) || repositoryBindings.has(repository)) {
        throw new Error('duplicate GitHub repository binding: ' + repository)
      }
      localRepositories.add(repository)
      repositoryBindings.set(repository, owner)
      repositories.push(repository)
    }
    result.push(repositories.length ? { owner, tokenEnv, repositories } : { owner, tokenEnv })
  }
  return result
}

export function normalizeConfig(config: CodeWorkerConfig | undefined): Settings {
  // Cordis volatile configuration nodes expose get(); project them into a
  // normal immutable snapshot before existing validation/default resolution.
  const unwrap = (value: any, active = new WeakSet<object>(), depth = 0): any => {
    if (!value || typeof value !== 'object') return value
    if (depth > 64) throw new Error('configuration nesting limit exceeded')
    if (active.has(value)) throw new Error('circular configuration reference')
    active.add(value)
    try {
      if (typeof value.get === 'function') return unwrap(value.get(), active, depth + 1)
      if (Array.isArray(value)) return value.map(item => unwrap(item, active, depth + 1))
      return Object.fromEntries(Object.entries(value).map(([key, item]) =>
        [key, unwrap(item, active, depth + 1)]))
    } finally {
      active.delete(value)
    }
  }
  const cfg: CodeWorkerConfig = unwrap(config && typeof config === 'object' ? config : {})
  const git = cfg.git && typeof cfg.git === 'object' ? cfg.git : {}
  const lsp = cfg.lsp && typeof cfg.lsp === 'object' ? cfg.lsp : {}
  const execution = cfg.execution && typeof cfg.execution === 'object' ? cfg.execution : {}
  const github = cfg.github && typeof cfg.github === 'object' ? cfg.github : {}
  const accounts = validateGitHubAccounts(github.accounts)
  const fsWriteContract = execution.fsWriteContract ?? 'legacy-overwrite'
  if (fsWriteContract !== 'legacy-overwrite' && fsWriteContract !== 'native-unguarded' &&
      fsWriteContract !== 'guarded-intent') {
    throw new Error('execution.fsWriteContract must be legacy-overwrite, native-unguarded or guarded-intent')
  }
  const servers =
    Array.isArray(lsp.servers) && lsp.servers.length > 0 ? lsp.servers : DEFAULT_LSP_SERVERS
  return {
    enabled: cfg.enabled !== false,
    autoRoutingEnabled: cfg.autoRouting !== false,
    workspaceRoot: typeof cfg.workspaceRoot === 'string' ? cfg.workspaceRoot.trim() : '',
    gitEnabled: git.enabled !== false,
    gitCommand:
      typeof git.command === 'string' && git.command.trim() !== '' ? git.command.trim() : 'git',
    allowWorktreeWrites: execution.allowWorktreeWrites === true,
    allowCommands: execution.allowCommands === true,
    fsWriteContract,
    languageConventions: normalizeLanguageConventions(execution.languageConventions),
    githubEnabled: github.enabled !== false,
    githubAllowWrites: github.allowWrites === true,
    githubAccounts: accounts,
    lspServers: servers.filter(
      (server): server is LspServerConfig =>
        Boolean(server) && typeof server.command === 'string',
    ),
    systemPromptText: typeof cfg.systemPromptText === 'string' ? cfg.systemPromptText : '',
  }
}

/** Legacy import surface. File I/O implementation is owned by Delta Adapter. */
export {
  resolveRoot, validateWriteTargetSafe, readTextSafe, readLocalTextAnchored,
  writeLocalTextAnchored, readLocalTextVerified, writeLocalTextVerified,
  writeLocalTextNoFollow, writeTextSafe,
} from './delta-code-adapter/workspace-files.js'
export type { GuardedWriteIntent } from './delta-code-adapter/workspace-files.js'
