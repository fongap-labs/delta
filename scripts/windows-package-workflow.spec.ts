import { spawnSync } from 'node:child_process'
import { resolve } from 'node:path'
import { expect, it } from 'vitest'

// The retired windows-package.yml is no longer part of Delta's CI. Validate
// the remaining PowerShell helper directly on Windows without changing priorities.
it.skipIf(process.platform !== 'win32')('inspects Windows packaging priority without modifying it', () => {
  const script = resolve(import.meta.dirname, '../apps/desktop/scripts/windows-packaging-priority.ps1')
  const result = spawnSync('pwsh', ['-NoProfile', '-File', script, '-InspectOnly'], {
    encoding: 'utf8', timeout: 30_000,
  })
  expect(result.error).toBeUndefined()
  expect(result.signal).toBeNull()
  expect(result.status, result.stderr).toBe(0)
  const report = JSON.parse(result.stdout.trim()) as {
    event: string
    before: Record<string, string | number>
    after: Record<string, string | number>
  }
  expect(report.event).toBe('packaging-process-priority')
  expect(report.before).toEqual(report.after)
})
