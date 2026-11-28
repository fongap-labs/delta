import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { collectConfigSourceOwnershipViolations } from './verify-config-source-ownership.ts'
import { COMPAT_FILES, legacyTokenHits } from './verify-token-prefix.ts'

const roots: string[] = []

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true })
})

describe('configuration source ownership gate', () => {
  it('rejects inline endpoints in shipped bundle patches', () => {
    const root = mkdtempSync(join(tmpdir(), 'dsh-config-source-ownership-'))
    roots.push(root)
    const directory = join(root, 'packages/subagent/subagent-claude-code')
    mkdirSync(directory, { recursive: true })
    writeFileSync(
      join(directory, 'cordis.patch.yml'),
      'config:\n  baseURL: !!js process.env.DEEPSEEK_SEARCH_BASE_URL\n',
    )

    expect(collectConfigSourceOwnershipViolations(root)).toEqual([
      'packages/subagent/subagent-claude-code/cordis.patch.yml:2: inlines a credential or endpoint from the environment.'
      + ' The adapter resolves apiKeyEnv through ctx.credentials and the endpoint through the'
      + ' environment snapshot; inlining here bypasses both ladders.',
    ])
  })
})

describe('verify-token-prefix', () => {
  it('reports each legacy token name with its line', () => {
    expect(legacyTokenHits('packages/a/src/x.css', '.a { color: var(--dsw-alias-bg-base); }\n.b { --dsl-gap: 1px; --ds-ease-in-out: x; }'))
      .toEqual([
        { file: 'packages/a/src/x.css', line: 1, name: '--dsw-alias-bg-base' },
        { file: 'packages/a/src/x.css', line: 2, name: '--dsl-gap' },
        { file: 'packages/a/src/x.css', line: 2, name: '--ds-ease-in-out' },
      ])
  })

  it('ignores current tokens, bare prefixes in prose, and Delta CLI options', () => {
    expect(legacyTokenHits('README.md', 'Use `--delta-alias-bg-base`, not the `--dsw-` prefix. Run `dsh --dsh-home ~/x --dsh-workspace .`.')).toEqual([])
  })

  it('exempts only compatibility files and skipped paths', () => {
    const text = 'var(--dsh-frame-leading-clearance)'
    for (const file of Object.keys(COMPAT_FILES)) expect(legacyTokenHits(file, text)).toEqual([])
    for (const file of ['vendor/cordis/x.ts', 'packages/a/lib/x.js', 'CHANGELOG.md', 'packages/a/node_modules/b/c.css']) {
      expect(legacyTokenHits(file, text)).toEqual([])
    }
    expect(legacyTokenHits('packages/client/ui-theme/src/styles/base.css', text)).toHaveLength(1)
  })

  it('keeps the exemption list to the compatibility layer', () => {
    expect(Object.keys(COMPAT_FILES)).toHaveLength(7)
  })
})
