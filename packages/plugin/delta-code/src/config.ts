/**
 * The Host projects volatile fields into the native Plugins configuration form.
 * GitHub tokens are never persisted in plugin config: only tokenEnv references
 * are stored. The native credentials service owns values, with masked UI.
 */
import Schema from '@deepseek-ai/schemastery'
import { DEFAULT_LSP_SERVERS } from './lsp-defaults.js'

const Account = Schema.object({
  owner: Schema.string().required(),
  tokenEnv: Schema.string().required(),
  repositories: Schema.array(Schema.string()).default([]),
})
const Server = Schema.object({
  language: Schema.string().required(),
  command: Schema.string().required(),
  args: Schema.array(Schema.string()).default([]),
})
const LanguageConvention = Schema.object({
  directory: Schema.string().required(),
  language: Schema.union(['typescript', 'javascript']).required(),
})

// The native schema resolves Volatile<T> references, while the runtime
// CodeWorkerConfig describes the unwrapped values passed to the tools.
// Keep the exported schema type generic to avoid leaking private inferred
// Schemastery / cosmokit types through the generated declaration.
export const Config: Schema<Record<string, unknown>> = Schema.object({
  enabled: Schema.boolean().default(true).volatile(),
  autoRouting: Schema.boolean().default(true).volatile(),
  workspaceRoot: Schema.string().default('').volatile(),
  git: Schema.object({
    enabled: Schema.boolean().default(true),
    command: Schema.string().default('git'),
  }).volatile(),
  execution: Schema.object({
    allowWorktreeWrites: Schema.boolean().default(false),
    allowCommands: Schema.boolean().default(false),
    fsWriteContract: Schema.string().default('legacy-overwrite'),
    languageConventions: Schema.array(LanguageConvention).default([]),
  }).volatile(),
  github: Schema.object({
    enabled: Schema.boolean().default(true),
    allowWrites: Schema.boolean().default(false),
    accounts: Schema.array(Account).default([]),
  }).volatile(),
  lsp: Schema.object({
    servers: Schema.array(Server).default(DEFAULT_LSP_SERVERS),
  }).volatile(),
  systemPromptText: Schema.string().default('').volatile(),
})
