/** Tests for the package-tree scan helpers that the stylesheet contracts range over. */
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterEach, describe, expect, it } from 'vitest'
import { packageFiles, packageStylesheets, parseRules } from './stylesheet-scan.ts'
import { normalizeLegacyTokenName } from '../src/legacy-tokens.ts'
import { contractProblems, renderAliases } from '../src/legacy-aliases.ts'

const roots: string[] = []

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true })
})

it('skips dot directories holding transient roots', () => {
  const root = mkdtempSync(join(tmpdir(), 'dsh-stylesheet-scan-'))
  roots.push(root)
  mkdirSync(join(root, 'pkg', 'nested'), { recursive: true })
  writeFileSync(join(root, 'pkg', 'package.json'), '{}')
  writeFileSync(join(root, 'pkg', 'nested', 'package.json'), '{}')
  mkdirSync(join(root, '.typert-worker-failure-probe'))
  writeFileSync(join(root, '.typert-worker-failure-probe', 'package.json'), '{}')

  // The helper reports `/`-separated paths on every platform, so the Windows
  // temporary root's separators must not reach the expectation. Traversal
  // reports the nested manifest before its parent, and directory order is not
  // guaranteed, so both sides are compared sorted.
  const normalized = root.replaceAll('\\', '/')
  expect(packageFiles(name => name === 'package.json', root).sort()).toEqual([
    `${normalized}/pkg/nested/package.json`,
    `${normalized}/pkg/package.json`,
  ])
})

/** Feature stylesheets take weights, layers, spacing, type sizes and durations from ui-theme tokens. */
const EXEMPT_FILES: Record<string, string> = {
  'packages/client/web/src/boot-page.module.css': 'boot page paints before ui-theme loads',
  'packages/client/web/src/base.css': 'shell fallbacks before ui-theme loads',
}

/** Pixel steps of the `--delta-space-*` 4px scale. */
const SPACE_STEPS = new Set([2, 4, 6, 8, 10, 12, 14, 16, 20, 24, 28, 32, 40, 48])
/** Pixel sizes of the fixed `--delta-font-size-*` scale. */
const FONT_SIZES = new Set([10, 11, 12, 13, 14, 15, 16, 18, 20, 24, 28])
/** Pixel and unitless values of the `--delta-line-height-*` scale. */
const LINE_HEIGHTS = new Set(['16px', '18px', '20px', '22px', '24px', '28px', '32px', '1', '1.4', '1.5', '1.6', '1.7'])
const SPACING = /^(?:padding|margin)(?:-(?:top|right|bottom|left|inline|block)(?:-(?:start|end))?)?$|^(?:row-|column-)?gap$/
const MOTION = /^(?:transition|animation)(?:-duration)?$/
const MOTION_TIME = /(?<![\w-])(\d*\.?\d+)(ms|s)\b/g

function onScaleSpace(token: string): boolean {
  const match = /^-?(\d+)px$/.exec(token)
  return match !== null && SPACE_STEPS.has(Number(match[1]))
}

function literal(property: string, value: string): boolean {
  const bare = value.replace(/\s*!important$/, '').trim()
  if (property === 'font-weight') return /^\d+$/.test(bare)
  if (property === 'z-index') return /^\d+$/.test(bare) && Number(bare) >= 100
  if (property === 'font-size') return /^\d+px$/.test(bare) && FONT_SIZES.has(Number.parseInt(bare, 10))
  if (property === 'line-height') return LINE_HEIGHTS.has(bare)
  if (MOTION.test(property)) {
    return [...bare.matchAll(MOTION_TIME)].some(([, amount = '', unit = '']) => {
      const milliseconds = Number(amount) * (unit === 's' ? 1000 : 1)
      return milliseconds !== 0 && milliseconds !== 0.01
    })
  }
  if (SPACING.test(property)) {
    const tokens = bare.split(/\s+/)
    return tokens.every(token => /^-?\d*\.?\d+px$|^0$|^auto$/.test(token)) && tokens.some(onScaleSpace)
  }
  return false
}

function literalViolations(css: string): string[] {
  return parseRules(css).flatMap(rule => rule.declarations
    .filter(([property, value]) => literal(property, value))
    .map(([property, value]) => `${property}: ${value.replace(/\s*!important$/, '').trim()}`))
}

const featureStylesheets = packageStylesheets().filter(file =>
  file.includes('/src/')
  && !file.includes('/packages/client/ui-theme/src/styles/')
  && !Object.keys(EXEMPT_FILES).some(suffix => file.endsWith(suffix)))

it('rejects literals that a --delta-* scale names', () => {
  expect(literalViolations(`
    .a { font-weight: 500; }
    .b { z-index: 1100; }
    .c { font-weight: var(--delta-font-weight-medium); z-index: var(--delta-layer-overlay); }
    .d { z-index: 2; font-weight: inherit; }
    .e { font-weight: 600 !important; }
    .f { padding: 4px 8px; gap: var(--delta-space-2); }
    .g { padding: 3px 5px; margin: 0 auto; }
    .h { font-size: 13px; line-height: 20px; }
    .i { font-size: 12.5px; line-height: 21px; }
    .j { transition: color 120ms ease, opacity 0.2s ease; }
    .k { transition: color var(--delta-transition-duration-fast) ease 200ms; animation-duration: 1.5s; }
  `)).toEqual([
    'font-weight: 500', 'z-index: 1100', 'font-weight: 600', 'padding: 4px 8px',
    'font-size: 13px', 'line-height: 20px',
    'transition: color 120ms ease, opacity 0.2s ease',
    'transition: color var(--delta-transition-duration-fast) ease 200ms',
    'animation-duration: 1.5s',
  ])
})

it('keeps feature stylesheets on --delta-* scale tokens', () => {
  expect(featureStylesheets.length).toBeGreaterThan(0)
  const failures = featureStylesheets.flatMap(file => literalViolations(readFileSync(file, 'utf8'))
    .map(violation => `${file}: ${violation}`))
  expect(failures).toEqual([])
})

it('names an existing file for every stylesheet exemption', () => {
  const all = packageStylesheets()
  for (const suffix of Object.keys(EXEMPT_FILES)) expect(all.some(file => file.endsWith(suffix)), suffix).toBe(true)
})

describe('legacy alias stylesheet', () => {
  const read = (path: string): string => readFileSync(fileURLToPath(new URL(path, import.meta.url)), 'utf8')
  const contract = JSON.parse(read('../src/legacy-tokens.json')) as { global: string[]; element: string[] }
  const aliases = read('../src/styles/legacy-plugin-aliases.css')
  const stylesEntry = read('../src/client/styles.ts')
  const rules = parseRules(aliases)
  const declared = (selector: string): Map<string, string> =>
    new Map(rules.filter(rule => rule.selectors.includes(selector)).flatMap(rule => rule.declarations))

  it('is loaded with theme styles and matches its generator', () => {
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
    expect(contract.global.length + contract.element.length).toBeLessThanOrEqual(482)
  })

  it('reports malformed contracts', () => {
    expect(contractProblems({ global: ['--b', '--a'], element: ['--dsw-x', '--dsw-x'] })).toEqual([
      'global names are not sorted', '--b has no legacy prefix', '--a has no legacy prefix', '--dsw-x is listed twice',
    ])
  })
})
