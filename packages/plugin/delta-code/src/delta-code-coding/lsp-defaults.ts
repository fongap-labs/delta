import type { LspServerConfig } from '../types.js'

/** The single default catalog used by Cordis settings and runtime fallback. */
export const DEFAULT_LSP_SERVERS: LspServerConfig[] = [
  { language: 'typescript', command: 'typescript-language-server', args: ['--stdio'] },
  { language: 'javascript', command: 'typescript-language-server', args: ['--stdio'] },
  { language: 'python', command: 'pyright-langserver', args: ['--stdio'] },
  { language: 'rust', command: 'rust-analyzer', args: ['--stdio'] },
  { language: 'go', command: 'gopls', args: ['serve'] },
]
