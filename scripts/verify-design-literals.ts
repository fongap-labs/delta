/** Reject reusable design literals in client stylesheets outside the token authority. */
import { readFileSync, readdirSync } from 'node:fs'
import { join, relative, resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const clientRoot = join(root, 'packages/client')

/** Pre-theme styles that must paint before the shared theme stylesheet loads. */
const EXEMPT_FILES: Readonly<Record<string, string>> = Object.freeze({
  'packages/client/web/src/boot-page.module.css': 'boot page paints before ui-theme loads',
  'packages/client/web/src/base.css': 'shell fallbacks before ui-theme loads',
})

const SPACE_STEPS = new Set([2, 4, 6, 8, 10, 12, 14, 16, 20, 24, 28, 32, 40, 48])
const FONT_SIZES = new Set([10, 11, 12, 13, 14, 15, 16, 18, 20, 24, 28])
const LINE_HEIGHTS = new Set(['16px', '18px', '20px', '22px', '24px', '28px', '32px', '1', '1.4', '1.5', '1.6', '1.7'])
const SPACING = /^(?:padding|margin)(?:-(?:top|right|bottom|left|inline|block)(?:-(?:start|end))?)?$|^(?:row-|column-)?gap$/
const MOTION = /^(?:transition|animation)(?:-duration)?$/
const MOTION_TIME = /(?<![\w-])(\d*\.?\d+)(ms|s)\b/g

function stylesheetPaths(): string[] {
  const files: string[] = []
  const walk = (directory: string): void => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const absolute = join(directory, entry.name)
      if (entry.isDirectory()) {
        if (!entry.name.startsWith('.') && !['node_modules', 'lib', 'dist'].includes(entry.name)) walk(absolute)
      } else if (entry.isFile() && entry.name.endsWith('.css')) {
        const path = relative(root, absolute).replaceAll('\\', '/')
        if (
          path.includes('/src/')
          && !path.startsWith('packages/client/ui-theme/src/styles/')
        ) files.push(path)
      }
    }
  }
  walk(clientRoot)
  return files.sort()
}

function onScaleSpace(token: string): boolean {
  const match = /^-?(\d+)px$/.exec(token)
  return match !== null && SPACE_STEPS.has(Number(match[1]))
}

function isLiteral(property: string, value: string): boolean {
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
  if (!SPACING.test(property)) return false
  const tokens = bare.split(/\s+/)
  return tokens.every(token => /^-?\d*\.?\d+px$|^0$|^auto$/.test(token)) && tokens.some(onScaleSpace)
}

function violations(css: string): string[] {
  const source = css.replace(/\/\*[\s\S]*?\*\//g, ' ')
  const found: string[] = []
  for (const [, , body = ''] of source.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    for (const part of body.split(';')) {
      const colon = part.indexOf(':')
      if (colon < 0) continue
      const property = part.slice(0, colon).trim()
      const value = part.slice(colon + 1).trim()
      if (isLiteral(property, value)) found.push(`${property}: ${value.replace(/\s*!important$/, '').trim()}`)
    }
  }
  return found
}

function main(): void {
  const files = stylesheetPaths()
  const missingExemptions = Object.keys(EXEMPT_FILES).filter(path => !files.includes(path))
  const failures = files.filter(file => !Object.hasOwn(EXEMPT_FILES, file))
    .flatMap(file => violations(readFileSync(resolve(root, file), 'utf8'))
      .map(value => `${file}: ${value}`))
  if (missingExemptions.length > 0) {
    failures.push(...missingExemptions.map(path => `${path}: configured pre-theme exemption no longer exists`))
  }
  if (failures.length > 0) {
    console.error(`verify-design-literals: ${String(failures.length)} design literal violation(s); use --delta-* tokens:`)
    for (const failure of failures.slice(0, 200)) console.error(`  ${failure}`)
    process.exitCode = 1
    return
  }
  console.log(`verify-design-literals: ${String(files.length)} client stylesheets use shared design tokens.`)
}

main()
