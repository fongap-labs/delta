/** Verify ownership and reference integrity for the shared Client design tokens. */
import { globSync, readFileSync, writeFileSync } from 'node:fs'
import { relative, resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const themeRoot = 'packages/client/ui-theme/src/styles/'
const catalogPath = 'docs/design-token-catalog.md'
const catalogPathZh = 'docs/design-token-catalog.zh.md'
const OWNED_TOKEN = /^--dsw-(?:static|alias|specific|radius|font|shadow|elevation)-[\w-]+$/
const GLOBAL_TOKEN = /^--dsw-(?:static|alias|specific|radius|font|shadow|elevation)-[\w-]+$/
const COMPONENT_REBIND = new Set(['--dsw-elevation-stroke-color'])

interface Declaration {
  file: string
  name: string
  value: string
}

/** Flatten CSS custom-property declarations without interpreting selector semantics. */
function declarations(css: string, file: string): Declaration[] {
  const clean = css.replace(/\/\*[\s\S]*?\*\//g, ' ')
  const found: Declaration[] = []
  for (const [, body = ''] of clean.matchAll(/\{([^{}]*)\}/g)) {
    for (const part of body.split(';')) {
      const colon = part.indexOf(':')
      if (colon < 0) continue
      const name = part.slice(0, colon).trim()
      if (OWNED_TOKEN.test(name)) found.push({ file, name, value: part.slice(colon + 1).trim() })
    }
  }
  return found
}

/** Resolve all CSS files shipped from first-party source trees. */
function sourceStylesheets(): string[] {
  return [...new Set([
    ...globSync('packages/*/*/src/**/*.css', { cwd: root }),
    ...globSync('apps/*/src/**/*.css', { cwd: root }),
    ...globSync('apps/*/renderer/**/*.css', { cwd: root }),
  ])]
    .map(file => file.replaceAll('\\', '/'))
    .filter(file => !/(?:^|\/)(?:lib|dist|node_modules|tests)\//.test(file))
    .sort()
}

/** Find missing references, duplicate owners, component-owned global tokens, and dependency cycles. */
export function designTokenProblems(files: string[], read = (file: string) => readFileSync(resolve(root, file), 'utf8')): string[] {
  const all = files.flatMap(file => declarations(read(file), file))
  const definitions = new Map<string, Declaration[]>()
  for (const item of all) definitions.set(item.name, [...(definitions.get(item.name) ?? []), item])

  const problems: string[] = []
  for (const [name, items] of definitions) {
    const paths = new Set(items.map(item => item.file))
    if (paths.size > 1 && !COMPONENT_REBIND.has(name)) problems.push(`${name} is defined in multiple files: ${[...paths].join(', ')}`)
    for (const item of items) {
      if (!item.file.startsWith(themeRoot) && !COMPONENT_REBIND.has(name)) {
        problems.push(`${item.file}: ${name} is outside the ui-theme authority`)
      }
    }
  }

  const graph = new Map<string, Set<string>>()
  for (const [name, items] of definitions) {
    const references = new Set(items.flatMap(item => [...item.value.matchAll(/var\(\s*(--[\w-]+)/g)]
      .map(([, ref]) => ref ?? '').filter(ref => GLOBAL_TOKEN.test(ref))))
    graph.set(name, references)
  }

  for (const file of files) {
    const css = read(file)
    for (const [, ref = ''] of css.matchAll(/var\(\s*(--dsw-(?:static|alias|specific|radius|font|shadow|elevation)-[\w-]+)/g)) {
      if (!definitions.has(ref)) problems.push(`${file}: references undefined token ${ref}`)
    }
  }

  const visiting = new Set<string>()
  const visited = new Set<string>()
  const cycles = new Set<string>()
  const visit = (name: string, chain: string[]): void => {
    if (visiting.has(name)) {
      const start = chain.indexOf(name)
      cycles.add([...chain.slice(start), name].join(' -> '))
      return
    }
    if (visited.has(name)) return
    visiting.add(name)
    for (const next of graph.get(name) ?? []) if (graph.has(next)) visit(next, [...chain, name])
    visiting.delete(name)
    visited.add(name)
  }
  for (const name of graph.keys()) visit(name, [])
  for (const cycle of cycles) problems.push(`token reference cycle: ${cycle}`)
  return [...new Set(problems)].sort((left, right) => left.localeCompare(right))
}

/** Render the complete shared-token name index from its owning CSS sources. */
export function renderDesignTokenCatalog(files: string[], language: 'en' | 'zh' = 'en', read = (file: string) => readFileSync(resolve(root, file), 'utf8')): string {
  const bySheet = new Map<string, Set<string>>()
  for (const file of files.filter(path => path.startsWith(themeRoot))) {
    for (const item of declarations(read(file), file)) {
      const tokens = bySheet.get(file) ?? new Set<string>()
      tokens.add(item.name)
      bySheet.set(file, tokens)
    }
  }
  const count = [...bySheet.values()].reduce((total, tokens) => total + tokens.size, 0)
  const sections = [...bySheet].sort(([left], [right]) => left.localeCompare(right)).map(([file, tokens]) => {
    const sheet = file.slice(themeRoot.length)
    const headers = language === 'en' ? ['Token', 'Role'] : ['令牌', '职责']
    return `### \`${sheet}\`\n\n| ${headers[0]} | ${headers[1]} |\n| --- | --- |\n${[...tokens].sort().map(token => `| \`${token}\` | ${tokenRole(token, language)} |`).join('\n')}`
  })
  return [
    language === 'en' ? '# Shared design token catalog' : '# 共享设计令牌目录',
    '',
    language === 'en' ? 'English | [中文](design-token-catalog.zh.md)' : '[English](design-token-catalog.md) | 中文',
    '',
    '<!-- Generated by `pnpm run generate-design-token-catalog`; do not edit the token list by hand. -->',
    '',
    language === 'en'
      ? `The Client theme source currently defines **${count} shared tokens**. This index records every globally owned \`--dsw-*\` token by its authoritative stylesheet. Exact values and light/dark mappings remain defined only in those CSS sheets; component CSS must consume the semantic role and must not copy its value.`
      : `Client 主题源码当前定义 **${count} 个共享令牌**。本目录按权威样式表列出所有全局拥有的 \`--dsw-*\` 令牌。具体数值与明暗主题映射只在对应 CSS 中定义；组件样式应使用语义角色，不得复制数值。`,
    '',
    language === 'en'
      ? 'Primitive palette and scale names are inputs to semantic tokens. Components should prefer `--dsw-alias-*` and use `--dsw-specific-*` only for a real component contract. `--dsw-static-*` is for the theme owner, not feature styles.'
      : '基础色板与尺度令牌用于构成语义令牌。组件优先使用 `--dsw-alias-*`；只有存在明确组件约定时才使用 `--dsw-specific-*`。`--dsw-static-*` 仅供主题维护方使用，功能样式不得直接引用。',
    '',
    ...sections.flatMap(section => [section, '']),
  ].join('\n').trimEnd() + '\n'
}

function tokenRole(name: string, language: 'en' | 'zh'): string {
  const english = name.startsWith('--dsw-static-') ? 'Primitive palette value'
    : name.startsWith('--dsw-alias-state-') ? 'Semantic status'
      : name.startsWith('--dsw-alias-') ? 'Semantic theme role'
        : name.startsWith('--dsw-specific-') ? 'Component or surface contract'
          : name.startsWith('--dsw-radius-') ? 'Shared corner scale'
            : name.startsWith('--dsw-font-') ? 'Typography family or scale'
              : name.startsWith('--dsw-shadow-') || name.startsWith('--dsw-elevation-') ? 'Elevation and shadow'
                : 'Theme token'
  if (language === 'en') return english
  return ({
    'Primitive palette value': '基础色板值',
    'Semantic status': '语义状态',
    'Semantic theme role': '语义主题角色',
    'Component or surface contract': '组件或表面约定',
    'Shared corner scale': '共享圆角尺度',
    'Typography family or scale': '字体族或排版尺度',
    'Elevation and shadow': '层级与阴影',
    'Theme token': '主题令牌',
  } as const)[english]
}

function main(): void {
  const files = sourceStylesheets()
  const catalog = renderDesignTokenCatalog(files, 'en')
  const catalogZh = renderDesignTokenCatalog(files, 'zh')
  if (process.argv.includes('--write')) {
    writeFileSync(resolve(root, catalogPath), catalog)
    writeFileSync(resolve(root, catalogPathZh), catalogZh)
  } else {
    for (const [path, expected] of [[catalogPath, catalog], [catalogPathZh, catalogZh]] as const) {
      const actual = readFileSync(resolve(root, path), 'utf8')
      if (actual !== expected) {
        console.error(`verify-design-token-contract: ${path} is stale; run pnpm run generate-design-token-catalog.`)
        process.exitCode = 1
        return
      }
    }
  }
  const problems = designTokenProblems(files)
  if (problems.length > 0) {
    console.error(`verify-design-token-contract: ${problems.length} problem(s):`)
    for (const problem of problems) console.error(`  ${problem}`)
    process.exitCode = 1
    return
  }
  const owned = files.filter(file => file.startsWith(themeRoot))
  console.log(`verify-design-token-contract: shared token references resolve across ${files.length} stylesheet(s); ${owned.length} ui-theme source sheet(s) own the global token vocabulary.`)
}

if (import.meta.filename === resolve(process.argv[1] ?? '')) main()

export const DESIGN_TOKEN_CONTRACT_ROOT = relative(root, resolve(root, themeRoot))
