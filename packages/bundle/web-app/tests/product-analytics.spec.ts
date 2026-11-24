/** Delta must not assemble product-event collection in its first-party Web bundle. */
import { readFileSync } from 'node:fs'
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
