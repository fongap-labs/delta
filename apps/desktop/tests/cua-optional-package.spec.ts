/** Delta CUA and browser providers ship as first-party opt-in modules. */
import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { expect, it } from 'vitest'

const repository = resolve(import.meta.dirname, '../../..')

async function readJson(path: string): Promise<Record<string, unknown>> {
  return JSON.parse(await readFile(resolve(repository, path), 'utf8')) as Record<string, unknown>
}

it('registers Delta-owned desktop and browser providers as opt-in modules', async () => {
  const [computerUse, browserUse, patch] = await Promise.all([
    readJson('packages/delta/computer-use/package.json'),
    readJson('packages/delta/browser-use/package.json'),
    readFile(resolve(repository, 'packages/bundle/base/cordis.patch.yml'), 'utf8'),
  ])
  expect(computerUse.name).toBe('@delta/computer-use')
  expect((computerUse.dependencies as Record<string, string>)['@deepseek-ai/dsh-computer-use']).toBe('workspace:*')
  expect(JSON.stringify(computerUse)).not.toMatch(/experimental\/cua-(?:bundle|windows|macos)/u)
  expect(browserUse.name).toBe('@delta/browser-use')
  expect((browserUse.dependencies as Record<string, string>)['@deepseek-ai/dsh-browser-use']).toBe('workspace:*')
  expect(patch).toMatch(/id: delta-computer-use\s+name: '@delta\/computer-use'\s+disabled: true/u)
  expect(patch).toMatch(/id: delta-browser-use\s+name: '@delta\/browser-use'\s+disabled: true/u)
})
