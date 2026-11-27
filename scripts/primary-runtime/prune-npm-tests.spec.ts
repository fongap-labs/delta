import { mkdir, mkdtemp, rm, stat, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { expect, it } from 'vitest'
import { pruneUnreferencedNpmTests } from './prune-npm-tests.ts'

async function writeTree(root: string, files: Record<string, string>): Promise<void> {
  for (const [path, contents] of Object.entries(files)) {
    await mkdir(dirname(join(root, path)), { recursive: true })
    await writeFile(join(root, path), contents)
  }
}

async function exists(path: string): Promise<boolean> {
  try {
    await stat(path)
    return true
  } catch {
    return false
  }
}

it('removes only test directories that runtime sources and entry points never reference', async () => {
  const root = await mkdtemp(join(tmpdir(), 'primary-npm-pruning-'))
  try {
    await writeTree(root, {
      'node_modules/plain/package.json': JSON.stringify({ name: 'plain', main: 'index.js' }),
      'node_modules/plain/index.js': 'module.exports = 1\n',
      'node_modules/plain/test/index.test.js': "require('../index.js')\n",
      'node_modules/plain/benchmarks/run.js': "require('../test/index.test.js')\n",
      'node_modules/plain/node_modules/nested/package.json': JSON.stringify({ name: 'nested' }),
      'node_modules/plain/node_modules/nested/index.js': 'module.exports = 2\n',
      'node_modules/plain/node_modules/nested/tests/a.js': 'void 0\n',
      'node_modules/@scope/pkg/package.json': JSON.stringify({ name: '@scope/pkg' }),
      'node_modules/@scope/pkg/index.js': 'module.exports = 3\n',
      'node_modules/@scope/pkg/__tests__/a.js': 'void 0\n',
      'node_modules/emoji/package.json': JSON.stringify({ name: 'emoji', main: 'lib/index.js' }),
      'node_modules/emoji/lib/index.js': "module.exports = require('./emoji/test/parse.js')\n",
      'node_modules/emoji/lib/emoji/test/parse.js': "module.exports = require('../../../fixtures/data.json')\n",
      'node_modules/emoji/fixtures/data.json': '{}\n',
      'node_modules/entry/package.json': JSON.stringify({ name: 'entry', exports: { './fixtures': './fixtures/index.js' } }),
      'node_modules/entry/fixtures/index.js': 'module.exports = 4\n',
      'node_modules/broken/package.json': '{',
      'node_modules/broken/test/a.js': 'void 0\n',
    })
    const expected = [
      'node_modules/@scope/pkg/__tests__',
      'node_modules/plain/benchmarks',
      'node_modules/plain/node_modules/nested/tests',
      'node_modules/plain/test',
    ]
    const kept = [
      'node_modules/emoji/lib/emoji/test/parse.js',
      'node_modules/emoji/fixtures/data.json',
      'node_modules/entry/fixtures/index.js',
      'node_modules/broken/test/a.js',
      'node_modules/plain/index.js',
      'node_modules/plain/node_modules/nested/index.js',
    ]

    expect(pruneUnreferencedNpmTests(root, { isDryRun: true })).toEqual(expected)
    for (const path of expected) expect(await exists(join(root, path))).toBe(true)

    expect(pruneUnreferencedNpmTests(root)).toEqual(expected)
    for (const path of expected) expect(await exists(join(root, path))).toBe(false)
    for (const path of kept) expect(await exists(join(root, path))).toBe(true)
    expect(pruneUnreferencedNpmTests(root)).toEqual([])
  } finally {
    await rm(root, { recursive: true, force: true })
  }
})
