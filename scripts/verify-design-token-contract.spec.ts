import { describe, expect, it } from 'vitest'
import { designTokenProblems } from './verify-design-token-contract.ts'

describe('shared design token contract', () => {
  it('allows centrally owned light and dark definitions and local elevation rebinding', () => {
    const files = [
      'packages/client/ui-theme/src/styles/base.css',
      'packages/client/ui-theme/src/styles/palette.css',
      'packages/client/ui-panel/src/Panel.module.css',
    ]
    const source: Record<string, string> = {
      [files[0]!]: ':root { --dsw-radius-sm: 8px; }',
      [files[1]!]: 'body { --dsw-static-brand: #123456; --dsw-alias-brand: var(--dsw-static-brand); } body[data-dark] { --dsw-static-brand: #abcdef; }',
      [files[2]!]: '.panel { --dsw-elevation-stroke-color: var(--dsw-alias-brand); color: var(--dsw-alias-brand); border-radius: var(--dsw-radius-sm); }',
    }
    expect(designTokenProblems(files, file => source[file]!)).toEqual([])
  })

  it('rejects undefined references, duplicate owners, component aliases, and cycles', () => {
    const files = [
      'packages/client/ui-theme/src/styles/a.css',
      'packages/client/ui-theme/src/styles/b.css',
      'packages/client/ui-panel/src/Panel.module.css',
    ]
    const source: Record<string, string> = {
      [files[0]!]: ':root { --dsw-alias-a: var(--dsw-alias-b); }',
      [files[1]!]: ':root { --dsw-alias-b: var(--dsw-alias-a); }',
      [files[2]!]: '.panel { --dsw-alias-a: red; color: var(--dsw-alias-missing); }',
    }
    expect(designTokenProblems(files, file => source[file]!)).toEqual([
      '--dsw-alias-a is defined in multiple files: packages/client/ui-theme/src/styles/a.css, packages/client/ui-panel/src/Panel.module.css',
      'packages/client/ui-panel/src/Panel.module.css: --dsw-alias-a is outside the ui-theme authority',
      'packages/client/ui-panel/src/Panel.module.css: references undefined token --dsw-alias-missing',
      'token reference cycle: --dsw-alias-a -> --dsw-alias-b -> --dsw-alias-a',
    ])
  })
})
