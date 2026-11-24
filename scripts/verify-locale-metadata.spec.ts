/** Metadata coverage follows Delta's shipped Chinese and English locales. */
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, expect, it } from 'vitest'
import { localeMetadataProblems } from './verify-locale-metadata.ts'

const roots: string[] = []
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }) })

function fixture(zh: unknown = { meta: { title: '名称 {name}', description: '中文描述。' } }): string {
  const root = mkdtempSync(join(tmpdir(), 'delta-locale-meta-'))
  roots.push(root)
  const directory = join(root, 'packages/demo/plugin/locale')
  mkdirSync(directory, { recursive: true })
  writeFileSync(join(directory, 'en.json'), JSON.stringify({ meta: { title: 'Name {name}', description: 'English description.' } }))
  writeFileSync(join(directory, 'zh.json'), JSON.stringify(zh))
  return root
}

it('accepts complete Chinese and English metadata', () => {
  expect(localeMetadataProblems(fixture())).toEqual([])
})

it.each([
  [{ meta: { title: '名称 {name}' } }, 'missing meta.description'],
  [{ meta: { title: '名称 {other}', description: '中文描述。' } }, 'changes placeholders'],
  [{ meta: { title: '', description: '中文描述。' } }, 'expected non-empty text'],
])('rejects incomplete or inconsistent Chinese metadata', (zh, diagnostic) => {
  expect(localeMetadataProblems(fixture(zh)).join('\n')).toContain(diagnostic)
})

it('checks referenced detail documents in both shipped locales', () => {
  const root = fixture({ meta: { title: '名称 {name}', detail: 'docs/DETAIL.md' } })
  const directory = join(root, 'packages/demo/plugin/locale')
  const en = { meta: { title: 'Name {name}', detail: 'docs/DETAIL.md' } }
  writeFileSync(join(directory, 'en.json'), JSON.stringify(en))
  expect(localeMetadataProblems(root).join('\n')).toContain('missing detail resource')
  mkdirSync(join(directory, '../docs'))
  writeFileSync(join(directory, '../docs/DETAIL.md'), '# Detail\n')
  expect(localeMetadataProblems(root)).toEqual([])
})

it('does not mistake package.json in the locale service package for a translation', () => {
  const root = fixture()
  const directory = join(root, 'packages/client/locale')
  mkdirSync(directory, { recursive: true })
  writeFileSync(join(directory, 'package.json'), '{"name":"locale"}')
  mkdirSync(join(directory, 'src'))
  writeFileSync(join(directory, 'src/index.ts'), 'export {}')
  expect(localeMetadataProblems(root)).toEqual([])
})

it('rejects an empty discovery', () => {
  const root = mkdtempSync(join(tmpdir(), 'delta-locale-empty-'))
  roots.push(root)
  expect(localeMetadataProblems(root)).toEqual(['no package locale metadata found'])
})

it('rejects loadable plugins without an English locale directory', () => {
  const root = fixture()
  const directory = join(root, 'packages/demo/new-plugin')
  mkdirSync(join(directory, 'src'), { recursive: true })
  writeFileSync(join(directory, 'package.json'), JSON.stringify({ name: '@delta/new-plugin' }))
  writeFileSync(join(directory, 'src/index.ts'), 'export function apply() {}')
  expect(localeMetadataProblems(root).join('\n')).toContain('new-plugin/locale')
})

it('checks nested locale resources and package-relative detail files', () => {
  const root = fixture()
  const directory = join(root, 'packages/demo/plugin/locale/tools')
  mkdirSync(directory)
  for (const language of ['en', 'zh']) {
    writeFileSync(join(directory, `${language}.json`), JSON.stringify({ meta: { title: 'Tools', detail: 'docs/TOOLS.md' } }))
  }
  mkdirSync(join(root, 'packages/demo/plugin/docs'))
  writeFileSync(join(root, 'packages/demo/plugin/docs/TOOLS.md'), '# Tools\n')
  expect(localeMetadataProblems(root)).toEqual([])
})

it.each(['../../other/README.md', 'docs\\DETAIL.md', 'docs/DETAIL.txt', '/README.md'])('rejects invalid detail path %s', (detail) => {
  const root = fixture({ meta: { title: '名称 {name}', description: '中文描述。', detail } })
  expect(localeMetadataProblems(root).join('\n')).toMatch(/must (?:be a relative path|name a Markdown file)/u)
})

it('requires sentence punctuation for English descriptions', () => {
  const root = fixture()
  const file = join(root, 'packages/demo/plugin/locale/en.json')
  writeFileSync(file, JSON.stringify({ meta: { title: 'Name {name}', description: 'Description' } }))
  expect(localeMetadataProblems(root).join('\n')).toContain('sentence punctuation')
  writeFileSync(file, JSON.stringify({ meta: { title: 'Name {name}', description: 'English description.' } }))
  expect(localeMetadataProblems(root)).toEqual([])
})

it('permits optional Chinese metadata for loadable plugins', () => {
  const root = fixture()
  const directory = join(root, 'packages/demo/plugin')
  mkdirSync(join(directory, 'src'))
  writeFileSync(join(directory, 'src/index.ts'), 'export function apply() {}')
  writeFileSync(join(directory, 'package.json'), JSON.stringify({ name: '@delta/demo' }))
  rmSync(join(directory, 'locale/zh.json'))
  expect(localeMetadataProblems(root)).toEqual([])
})

it('excludes packages formally classified as test helpers', () => {
  const root = fixture()
  const directory = join(root, 'packages/test-support/llm-replay')
  mkdirSync(join(directory, 'src'), { recursive: true })
  writeFileSync(join(directory, 'src/index.ts'), 'export function apply() {}')
  writeFileSync(join(directory, 'package.json'), JSON.stringify({ name: '@deepseek-ai/dsh-llm-replay' }))
  expect(localeMetadataProblems(root)).toEqual([])
})

it.each(['\u200b', '\ufeff', String.raw`\u200b`, String.raw`\ufeff`])('rejects invisible characters in shipped locale resources', (character) => {
  const root = fixture()
  const file = join(root, 'packages/demo/plugin/locale/zh.json')
  writeFileSync(file, readFileSync(file, 'utf8').replace('中文描述。', `中文${character}描述。`))
  expect(localeMetadataProblems(root).join('\n')).toContain('invisible locale character')
})
