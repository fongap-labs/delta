import type { LanguageConventionRule } from './consistency.js'
import type { DeltaFileWriteContract } from './delta-code-adapter/contracts/files.js'

/** Host-independent public configuration and result types. */
export type HostFsWriteContract = DeltaFileWriteContract

/** One configured language server. `args` is accepted for config compatibility. */
export interface LspServerConfig {
  language: string
  command: string
  args?: string[]
}

/** Optional credential identity; its Token permissions, not repository bindings, determine access. */
export interface GitHubAccountConfig {
  /** GitHub username or organization name (left side of owner/repository). */
  owner: string
  /** Environment variable name that holds the GitHub personal access token. */
  tokenEnv: string
  /** Optional routing preference for ambiguous multi-account setups, never an access allowlist. */
  repositories?: string[]
}

/** Plugin configuration as accepted from the Cordis patch file. */
export interface CodeWorkerConfig {
  enabled?: boolean
  autoRouting?: boolean
  workspaceRoot?: string
  git?: {
    enabled?: boolean
    command?: string
  }
  execution?: {
    allowWorktreeWrites?: boolean
    allowCommands?: boolean
    fsWriteContract?: HostFsWriteContract
    languageConventions?: LanguageConventionRule[]
  }
  github?: {
    enabled?: boolean
    allowWrites?: boolean
    accounts?: GitHubAccountConfig[]
  }
  lsp?: {
    servers?: LspServerConfig[]
  }
  systemPromptText?: string
}

/** Normalized settings with every default resolved. */
export interface Settings {
  enabled: boolean
  autoRoutingEnabled: boolean
  workspaceRoot: string
  gitEnabled: boolean
  gitCommand: string
  allowWorktreeWrites: boolean
  allowCommands: boolean
  fsWriteContract: HostFsWriteContract
  languageConventions: LanguageConventionRule[]
  githubEnabled: boolean
  githubAllowWrites: boolean
  githubAccounts: GitHubAccountConfig[]
  lspServers: LspServerConfig[]
  systemPromptText: string
}

/** One edit applied by `code_multi_edit`. */
export interface EditResult {
  path: string
  action: 'overwrite' | 'replace' | 'skipped'
  ok: boolean
  note?: string
}

/** Parsed result of `git status --porcelain=v1 --branch`. */
export interface GitStatusResult {
  branch: string
  ahead: number
  behind: number
  staged: string[]
  unstaged: string[]
  untracked: string[]
  clean: boolean
}

/** One language-server probe result. */
export interface LspProbeResult {
  language: string
  command: string
  available: boolean
  note: string
}
