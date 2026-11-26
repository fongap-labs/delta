/** Delta must not assemble product-event collection in its first-party Web bundle. */
import { existsSync, readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { expect, it } from 'vitest'
import { parseEntryYaml } from '@deepseek-ai/cordis-plugin-include'

interface PatchRow { id?: string }
interface PatchEntry { insert?: PatchRow[] }

function insertedRows(url: URL): PatchRow[] {
  const parsed = parseEntryYaml(readFileSync(fileURLToPath(url), 'utf8'))
  if (!Array.isArray(parsed)) throw new TypeError('bundle patch must parse to a patch list')
  return (parsed as PatchEntry[]).flatMap(patch => patch.insert ?? [])
}

it('does not mount product analytics or telemetry collection', () => {
  const webRows = insertedRows(new URL('../cordis.patch.yml', import.meta.url))
  const baseRows = insertedRows(new URL('../../base/cordis.patch.yml', import.meta.url))
  const forbidden = new Set(['product-analytics', 'desktop-product-telemetry', 'session-telemetry-otel'])

  expect(webRows.filter(row => forbidden.has(row.id ?? ''))).toEqual([])
  expect(baseRows.filter(row => forbidden.has(row.id ?? ''))).toEqual([])
})

it('physically removes the product analytics workspace and its dependency graph', () => {
  expect(existsSync(new URL('../../../client/product-analytics/package.json', import.meta.url))).toBe(false)
  const lock = readFileSync(new URL('../../../../pnpm-lock.yaml', import.meta.url), 'utf8')
  expect(lock).not.toContain('product-analytics')
})
