/**
 * Generate `packages/client/ui-theme/src/styles/legacy-plugin-aliases.css`
 * from the frozen legacy token contract in
 * `packages/client/ui-theme/src/legacy-tokens.json`. Each legacy name reads
 * as its current `--delta-*` token, so third-party plugin styles written
 * against the earlier prefixes keep resolving. Rendering lives in
 * `packages/client/ui-theme/src/legacy-aliases.ts`.
 *
 * `--check` verifies the committed stylesheet is fresh.
 */

import { readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { contractProblems, renderAliases, type LegacyTokenContract } from '../packages/client/ui-theme/src/legacy-aliases.ts'

const root = resolve(import.meta.dirname, '..')
const SOURCE = 'packages/client/ui-theme/src/legacy-tokens.json'
const OUT = 'packages/client/ui-theme/src/styles/legacy-plugin-aliases.css'

/** Read the committed contract. */
function readContract(): LegacyTokenContract {
  const raw = JSON.parse(readFileSync(resolve(root, SOURCE), 'utf8')) as LegacyTokenContract
  return { global: raw.global, element: raw.element }
}

function main(): void {
  const contract = readContract()
  const problems = contractProblems(contract)
  if (problems.length > 0) {
    throw new Error(`gen-legacy-token-aliases: ${SOURCE} is malformed:\n${problems.map(problem => `  ${problem}`).join('\n')}`)
  }
  const next = renderAliases(contract)
  const path = resolve(root, OUT)
  if (process.argv.includes('--check')) {
    let current = ''
    try { current = readFileSync(path, 'utf8') } catch { /* missing counts as stale */ }
    if (current.replace(/\r\n/g, '\n') === next) {
      console.log(`gen-legacy-token-aliases: ${OUT} is up to date.`)
      return
    }
    console.error(`gen-legacy-token-aliases: stale — ${OUT}. Run \`pnpm run gen-legacy-token-aliases\` and commit the result.`)
    process.exitCode = 1
    return
  }
  writeFileSync(path, next)
  console.log(`gen-legacy-token-aliases: wrote ${OUT}.`)
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === resolve(import.meta.filename)) main()
