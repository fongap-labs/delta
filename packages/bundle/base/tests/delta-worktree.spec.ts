import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { resolve } from 'node:path'
import { parseEntryYaml } from '@deepseek-ai/cordis-plugin-include'
import { expect, it } from 'vitest'

it('ships the Delta worktree plugin disabled and uses the portable home', () => {
  const root = fileURLToPath(new URL('..', import.meta.url))
  const manifest = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8')) as { dependencies: Record<string, string> }
  const patch = parseEntryYaml(readFileSync(resolve(root, 'cordis.patch.yml'), 'utf8')) as { insert?: Record<string, unknown>[] }[]
  const rows = patch.flatMap(entry => entry.insert ?? []).filter(entry => entry.id === 'delta-worktree')
  expect(rows).toEqual([{ id: 'delta-worktree', name: '@delta/worktree', disabled: true, config: { root: { __jsExpr: "dshHomePath('worktrees')" } } }])
  expect(manifest.dependencies['@delta/worktree']).toBe('workspace:*')
  expect(patch.flatMap(entry => entry.insert ?? []).filter(entry => entry.id === 'delta-forge')).toEqual([
    { id: 'delta-forge', name: '@delta/forge', disabled: true },
  ])
  expect(manifest.dependencies['@delta/forge']).toBe('workspace:*')
  expect(patch.flatMap(entry => entry.insert ?? []).find(entry => entry.id === 'system-prompt')).toMatchObject({
    config: { personaPrefix: expect.stringContaining('language used in the conversation') },
  })
})
