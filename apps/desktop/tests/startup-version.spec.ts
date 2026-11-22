import { describe, expect, it } from 'vitest'
import { isPackagedDesktopVersionQuery } from '../src/startup-version.ts'

describe('packaged Desktop version query', () => {
  it('accepts only an exact packaged --version command', () => {
    expect(isPackagedDesktopVersionQuery(['Delta.exe', '--version'], true)).toBe(true)
    expect(isPackagedDesktopVersionQuery(['Delta.exe', '--version'], false)).toBe(false)
  })

  it('does not intercept normal launches or commands with extra arguments', () => {
    expect(isPackagedDesktopVersionQuery(['Delta.exe'], true)).toBe(false)
    expect(isPackagedDesktopVersionQuery(['Delta.exe', '--updated'], true)).toBe(false)
    expect(isPackagedDesktopVersionQuery(['Delta.exe', '--version', '--updated'], true)).toBe(false)
    expect(isPackagedDesktopVersionQuery(['Delta.exe', '--', '--version'], true)).toBe(false)
  })
})
