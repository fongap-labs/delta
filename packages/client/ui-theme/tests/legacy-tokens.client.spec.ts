// @vitest-environment jsdom
/** Legacy `--dsw-`/`--dsh-`/`--dsl-`/`--ds-` token keys keep reaching host styles through the runtime. */
import { Context } from '@deepseek-ai/cordis'
import { stubConfigForm } from '@deepseek-ai/dsh-client-test-runtime'
import type { ThemeSettings, ThemeTokenOverrides } from '@deepseek-ai/dsh-client-ui-theme/client'
import { ThemeRuntime } from '@deepseek-ai/dsh-client-ui-theme/client'
import { describe, expect, it } from 'vitest'
import { normalizeLegacyTokenName, normalizeLegacyTokens } from '../src/legacy-tokens.ts'

const make = (): ThemeRuntime => new ThemeRuntime(new Context(), stubConfigForm<ThemeSettings>().scope)

describe('legacy token names', () => {
  it('map by prefix, with the user font preferences renamed explicitly', () => {
    expect(normalizeLegacyTokenName('--dsw-alias-bg-layer-1')).toBe('--delta-alias-bg-layer-1')
    expect(normalizeLegacyTokenName('--dsh-frame-leading-clearance')).toBe('--delta-frame-leading-clearance')
    expect(normalizeLegacyTokenName('--dsl-terminal-font')).toBe('--delta-terminal-font')
    expect(normalizeLegacyTokenName('--ds-font-family-code')).toBe('--delta-font-family-code')
    expect(normalizeLegacyTokenName('--dsh-font-family-code')).toBe('--delta-font-family-user-code')
    expect(normalizeLegacyTokenName('--dsh-font-family-text')).toBe('--delta-font-family-user-text')
    expect(normalizeLegacyTokenName('--dsh-font-family-terminal')).toBe('--delta-font-family-user-terminal')
    expect(normalizeLegacyTokenName('--delta-alias-bg-base')).toBe('--delta-alias-bg-base')
    expect(normalizeLegacyTokenName('--plugin-own')).toBe('--plugin-own')
    expect(normalizeLegacyTokenName('--dsx-other')).toBe('--dsx-other')
  })

  it('let a current name win over its legacy spelling in either key order', () => {
    expect(normalizeLegacyTokens({ '--dsw-alias-bg-base': 'old', '--delta-alias-bg-base': 'new' }))
      .toEqual({ '--delta-alias-bg-base': 'new' })
    expect(normalizeLegacyTokens({ '--delta-alias-bg-base': 'new', '--dsw-alias-bg-base': 'old' }))
      .toEqual({ '--delta-alias-bg-base': 'new' })
  })
})

describe('ThemeRuntime legacy keys', () => {
  it('folds legacy override keys into the current tokens for each palette', () => {
    const theme = make()
    theme.setTheme('light')
    theme.overrideTokens('legacy-plugin', {
      '--dsw-alias-bg-layer-1': { light: '#fefefe', dark: '#101010' },
      '--dsh-font-family-code': { light: '"Iosevka"', dark: '"Iosevka"' },
    } satisfies ThemeTokenOverrides)
    expect(theme.getTheme().active.tokens).toEqual({
      '--delta-alias-bg-layer-1': '#fefefe',
      '--delta-font-family-user-code': '"Iosevka"',
    })
    theme.setTheme('dark')
    expect(theme.getTheme().active.tokens['--delta-alias-bg-layer-1']).toBe('#101010')
    expect(Object.keys(theme.getTheme().active.tokens).some(name => name.startsWith('--dsw-'))).toBe(false)
  })

  it('registers themes written with legacy token keys', () => {
    const theme = make()
    theme.register({ id: 'sepia', colorScheme: 'light', tokens: { '--dsw-alias-bg-base': 'wheat', '--delta-alias-label-primary': 'sienna' } })
    theme.setTheme('sepia')
    expect(theme.getTheme().active.tokens).toEqual({ '--delta-alias-bg-base': 'wheat', '--delta-alias-label-primary': 'sienna' })
  })

  it('still rejects malformed legacy override values', () => {
    const theme = make()
    expect(() => theme.overrideTokens('legacy-plugin', { '--dsw-alias-bg-base': 'red' } as unknown as ThemeTokenOverrides))
      .toThrow('bare string')
  })
})
