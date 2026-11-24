import { expect, it } from 'vitest'
import { selectDesktopPackageClosure, type PackedDesktopPackage } from '../scripts/prepare-package-set.ts'

const pack = (name: string, dependencies: Record<string, string> = {}): PackedDesktopPackage => ({
  tarball: `${name}.tgz`, manifest: { name, version: '1.0.0', dependencies },
})

it('requires the locally packed Delta worktree plugin in the desktop closure', () => {
  const available = new Map<string, PackedDesktopPackage>([
    ['@deepseek-ai/dsh', pack('@deepseek-ai/dsh', { '@deepseek-ai/dsh-base': '1.0.0' })],
    ['@deepseek-ai/dsh-desktop-host', pack('@deepseek-ai/dsh-desktop-host')],
    ['@deepseek-ai/dsh-base', pack('@deepseek-ai/dsh-base', { '@delta/worktree': '0.1.0', '@delta/forge': '0.1.0' })],
  ])
  expect(() => selectDesktopPackageClosure(available, { platform: 'win32', arch: 'x64' })).toThrow(/unpacked package @delta\/forge/u)
  available.set('@delta/forge', pack('@delta/forge'))
  expect(() => selectDesktopPackageClosure(available, { platform: 'win32', arch: 'x64' })).toThrow(/unpacked package @delta\/worktree/u)
  available.set('@delta/worktree', pack('@delta/worktree'))
  expect(selectDesktopPackageClosure(available, { platform: 'win32', arch: 'x64' }).map(item => item.manifest.name)).toContain('@delta/worktree')
  expect(selectDesktopPackageClosure(available, { platform: 'win32', arch: 'x64' }).map(item => item.manifest.name)).toContain('@delta/forge')
})
