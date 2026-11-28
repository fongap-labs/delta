/** The generated legacy alias stylesheet stays fresh, loaded, and bounded by the frozen contract. */
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import { normalizeLegacyTokenName } from '../src/legacy-tokens.ts'
import { contractProblems, renderAliases } from '../src/legacy-aliases.ts'
import { parseRules } from './stylesheet-scan.ts'

const read = (path: string): string => readFileSync(fileURLToPath(new URL(path, import.meta.url)), 'utf8')
const contract = JSON.parse(read('../src/legacy-tokens.json')) as { global: string[]; element: string[] }
const aliases = read('../src/styles/legacy-plugin-aliases.css')
const stylesEntry = read('../src/client/styles.ts')

/** Size of the contract frozen at the `--delta-` rename; the list may only shrink. */
const FROZEN_CONTRACT_SIZE = 482

describe('legacy alias stylesheet', () => {
  const rules = parseRules(aliases)
  const declared = (selector: string): Map<string, string> =>
    new Map(rules.filter(rule => rule.selectors.includes(selector)).flatMap(rule => rule.declarations))

  it('is loaded with the theme styles and matches its generator', () => {
    expect(stylesEntry).toContain("import '../styles/legacy-plugin-aliases.css'")
    expect(aliases.replace(/\r\n/g, '\n')).toBe(renderAliases(contract))
    expect(contractProblems(contract)).toEqual([])
  })

  it('aliases every frozen name to its current token exactly once', () => {
    const global = declared('body')
    const element = declared('*')
    expect([...global.keys()]).toEqual(contract.global)
    expect([...element.keys()]).toEqual(contract.element)
    for (const [name, value] of [...global, ...element]) expect(value).toBe(`var(${normalizeLegacyTokenName(name)})`)
  })

  it('aliases body-scoped names on :root and body so dark-theme and inline overrides resolve', () => {
    const scopes = rules.filter(rule => rule.declarations.some(([name]) => contract.global.includes(name)))
    expect(scopes.map(rule => rule.selectors)).toEqual([[':root', 'body']])
  })

  it('resolves slot and component-redeclared tokens at each element', () => {
    for (const name of ['--dsh-chat-aside-gutter', '--dsh-composer-model-text-display', '--dsh-composer-model-icon-display',
      '--dsh-frame-leading-clearance', '--dsw-elevation-stroke', '--dsw-focus-ring-color', '--dsh-scrollbar-thumb']) {
      expect(contract.element).toContain(name)
    }
    expect(rules.find(rule => rule.selectors.includes('*'))?.selectors).toEqual(['*', '::before', '::after'])
  })

  it('keeps the frozen contract from growing', () => {
    expect(contract.global.length + contract.element.length).toBeLessThanOrEqual(FROZEN_CONTRACT_SIZE)
  })

  it('reports malformed contracts', () => {
    expect(contractProblems({ global: ['--b', '--a'], element: ['--dsw-x', '--dsw-x'] })).toEqual([
      'global names are not sorted', '--b has no legacy prefix', '--a has no legacy prefix', '--dsw-x is listed twice',
    ])
  })
})
