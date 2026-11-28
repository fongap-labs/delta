/**
 * Reject token names with the prefixes that preceded `--delta-`
 * (`--dsw-`, `--dsh-`, `--dsl-`, `--ds-`). Host code declares and reads only
 * `--delta-*`; legacy names survive solely in the compatibility layer that
 * keeps third-party plugins working, and in the Delta CLI's own option names.
 */

import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')

/** Legacy token-name pattern; a bare prefix in prose (`--dsw-`) does not match. */
export const LEGACY_TOKEN = /--(?:dsw|dsh|dsl|ds)-[A-Za-z0-9-]*[A-Za-z0-9]/g

/** Delta CLI options that share the `--dsh-` spelling but are not tokens. */
export const CLI_OPTIONS = /^--dsh-(?:version|home|bin|platform-origin|welcome-locale|workspace)$/

/** Files that own the compatibility contract, with the reason each may name legacy tokens. */
export const COMPAT_FILES: Readonly<Record<string, string>> = Object.freeze({
  'packages/client/ui-theme/src/legacy-tokens.json': 'frozen legacy name list',
  'packages/client/ui-theme/src/legacy-tokens.ts': 'explicit legacy renames',
  'packages/client/ui-theme/src/styles/legacy-plugin-aliases.css': 'generated compatibility aliases',
  'packages/client/ui-theme/tests/legacy-tokens.client.spec.ts': 'compatibility runtime spec',
  'packages/client/ui-theme/tests/legacy-aliases.client.spec.ts': 'compatibility stylesheet spec',
  'scripts/verify-token-prefix.ts': 'this gate',
  'scripts/verify-token-prefix.spec.ts': 'this gate\'s spec',
})

/** Paths outside the gate: third-party sources, history, and generated or binary artifacts. */
const SKIPPED = /^(?:vendor|native|patches)\/|(?:^|\/)(?:node_modules|lib)\/|(?:^|\/)CHANGELOG(?:\.[a-z]+)?\.md$|^pnpm-lock\.yaml$/
const BINARY = /\.(?:png|jpe?g|gif|webp|ico|woff2?|ttf|otf|zip|gz|pdf|node|exe|dll|wasm|mp[34]|lockb|icns)$/i

/** One legacy token occurrence. */
export interface PrefixHit {
  readonly file: string
  readonly line: number
  readonly name: string
}

/**
 * Find legacy token names in one file's text.
 * @param file - repository-relative path with `/` separators.
 * @param text - file contents.
 * @returns occurrences outside the CLI options; empty for compatibility files and skipped paths.
 */
export function legacyTokenHits(file: string, text: string): PrefixHit[] {
  if (Object.hasOwn(COMPAT_FILES, file) || SKIPPED.test(file) || BINARY.test(file)) return []
  const hits: PrefixHit[] = []
  for (const match of text.matchAll(LEGACY_TOKEN)) {
    if (CLI_OPTIONS.test(match[0])) continue
    hits.push({ file, line: text.slice(0, match.index).split('\n').length, name: match[0] })
  }
  return hits
}

function main(): void {
  const files = execFileSync('git', ['ls-files', '-co', '--exclude-standard'], { cwd: root, encoding: 'utf8', maxBuffer: 1 << 28 })
    .split('\n').map(line => line.trim()).filter(Boolean)
  const hits: PrefixHit[] = []
  for (const file of files) {
    if (SKIPPED.test(file) || BINARY.test(file)) continue
    let text: string
    try { text = readFileSync(resolve(root, file), 'utf8') } catch { continue }
    if (!text.includes('--ds')) continue
    hits.push(...legacyTokenHits(file, text))
  }
  if (hits.length === 0) {
    console.log('verify-token-prefix: no legacy token names outside the compatibility layer.')
    return
  }
  console.error(`verify-token-prefix: ${String(hits.length)} legacy token name(s); use the --delta- prefix:`)
  for (const hit of hits.slice(0, 200)) console.error(`  ${hit.file}:${String(hit.line)}  ${hit.name}`)
  process.exitCode = 1
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === resolve(import.meta.filename)) main()
