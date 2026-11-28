/** Source checks for the changed-files and explicit-delivery layout. */
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

const read = (name: string): string =>
  readFileSync(fileURLToPath(new URL(`../src/client/${name}`, import.meta.url)), 'utf8')

describe('deliverables layout', () => {
  it('keeps the first file-section offset and removes the second', () => {
    expect(read('ChangedFiles.module.css')).toMatch(/\.card\s*\{[^}]*margin-top:\s*var\(--delta-space-1\)/s)
    const deliveries = read('Deliverables.module.css')
    expect(deliveries).toMatch(/\.root\s*\{[^}]*margin-top:\s*var\(--delta-space-1\)/s)
    expect(deliveries).toMatch(/\.root\[data-after-changes='true'\]\s*\{\s*margin-top:\s*0;\s*\}/)
    expect(deliveries).toMatch(/\.file\s*\{[^}]*height:\s*60px;[^}]*padding:\s*var\(--delta-space-2\)\s*var\(--delta-space-2-5\)/s)
    expect(deliveries).toMatch(/\.fileIcon\s*\{[^}]*box-sizing:\s*border-box;[^}]*width:\s*40px;[^}]*height:\s*40px/s)
    expect(deliveries).toMatch(/\.fileIcon\s*\{[^}]*border:\s*0\.5px solid var\(--delta-alias-border-l1\)/s)
    expect(read('PresentedFileCard.tsx')).toMatch(/<FileTypeIcon path=\{file\.path\} size=\{20\} \/>/)
    expect(deliveries).toMatch(/\.fileName\s*\{[^}]*font-size:\s*var\(--delta-font-size-13\);/s)
    expect(deliveries).toMatch(/\.fileName\s*\{[^}]*line-height:\s*var\(--delta-line-height-20\);/s)
    expect(deliveries).toMatch(/\.description\s*\{[^}]*font-size:\s*var\(--delta-font-size-10\);/s)
    expect(deliveries).toMatch(/\.description\s*\{[^}]*line-height:\s*var\(--delta-line-height-16\);/s)
    expect(deliveries).toMatch(/\.presented\s*\{[^}]*gap:\s*var\(--delta-space-2-5\)/s)
  })

})
