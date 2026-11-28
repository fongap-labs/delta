/** Feature client stylesheets take weights, layers, spacing, type sizes and durations from ui-theme tokens, never literals. */
import { readFileSync } from 'node:fs'
import { expect, it } from 'vitest'
import { packageStylesheets, parseRules } from './stylesheet-scan.ts'

/** Stylesheets that own the tokens or paint before ui-theme loads, keyed by repo-relative suffix. */
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
/** Numeric motion values remain literals except zero and the reduced-motion 0.01ms floor. */
const MOTION_TIME = /(?<![\w-])(\d*\.?\d+)(ms|s)\b/g

/** Whether a spacing token is a scale step written as a pixel literal. */
function onScaleSpace(token: string): boolean {
  const match = /^-?(\d+)px$/.exec(token)
  return match !== null && SPACE_STEPS.has(Number(match[1]))
}

/** A declaration that writes a value the `--delta-*` scales already name. */
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

/**
 * Literal violations in one stylesheet.
 * @param css - stylesheet text.
 * @returns `property: value` descriptions of each violating declaration.
 */
function literalViolations(css: string): string[] {
  return parseRules(css).flatMap(rule => rule.declarations
    .filter(([property, value]) => literal(property, value))
    .map(([property, value]) => `${property}: ${value.replace(/\s*!important$/, '').trim()}`))
}

const stylesheets = packageStylesheets().filter(file =>
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
  expect(stylesheets.length).toBeGreaterThan(0)
  const failures = stylesheets.flatMap(file => literalViolations(readFileSync(file, 'utf8'))
    .map(violation => `${file}: ${violation}`))
  expect(failures).toEqual([])
})

it('names an existing file for every file exemption', () => {
  const all = packageStylesheets()
  for (const suffix of Object.keys(EXEMPT_FILES)) expect(all.some(file => file.endsWith(suffix)), suffix).toBe(true)
})
