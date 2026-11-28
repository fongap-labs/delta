/** FileTypeIcon's default per-category palette as CSS text. */
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import type { CodeFileType, FileType } from '@deepseek-ai/dsh-client-ui-primitives'

const css = readFileSync(fileURLToPath(new URL('../src/FileTypeIcon.module.css', import.meta.url)), 'utf8')

type TraditionalFileType = Exclude<FileType, CodeFileType>

const TYPE_COLORS: Readonly<Record<TraditionalFileType, string>> = {
  code: 'var(--delta-static-deepseek-500)',
  excel: 'var(--delta-static-green-500)',
  folder: 'var(--delta-static-amber-400)',
  html: 'var(--delta-static-deepseek-500)',
  image: 'var(--delta-file-type-violet)',
  markdown: 'var(--delta-static-deepseek-500)',
  other: 'var(--delta-static-neutral-bluish-300)',
  pdf: 'var(--delta-static-red-600)',
  ppt: 'var(--delta-static-amber-500)',
  video: 'var(--delta-file-type-violet)',
  word: 'var(--delta-static-deepseek-450)',
}

describe('FileTypeIcon.module.css', () => {
  it.each(Object.entries(TYPE_COLORS) as [TraditionalFileType, string][])(
    '%s has its own default color',
    (type, color) => {
      const rule = css.match(new RegExp(`\\.${type}\\s*\\{([^}]*)\\}`))?.[1]
      expect(rule).toContain(`--delta-file-type-default-color: ${color}`)
    },
  )

  it('keeps one caller override and the supplied violet in named variables', () => {
    expect(css).toContain('color: var(--delta-file-type-icon-color, var(--delta-file-type-default-color))')
    expect(css).toContain('--delta-file-type-violet: rgb(139, 118, 246)')
  })
})
