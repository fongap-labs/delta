import { readFileSync } from 'node:fs'
import { expect, it } from 'vitest'

const desktop = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8')
const hostProcess = readFileSync(new URL('../src/host-process.ts', import.meta.url), 'utf8')
const host = readFileSync(new URL('../../desktop-host/src/index.ts', import.meta.url), 'utf8')
const bundle = readFileSync(new URL('../tsdown.config.ts', import.meta.url), 'utf8')

it('does not wire automatic crash reports, renderer console capture or plugin operation journals into the desktop runtime', () => {
  for (const marker of ['writeCrashReport', 'RendererConsoleTail', 'OperationJournal', 'runOperationReportJob', 'persistCrashReport', 'pruneCrashReports']) expect(desktop).not.toContain(marker)
  expect(host).not.toContain('installOperationDiagnostics')
  expect(host).not.toContain('formatFatalDiagnostic')
  expect(bundle).not.toContain('operation-report-worker')
  expect(desktop).toContain('recovery.report(error)')
  expect(desktop).toContain('backend.close()')
})

it('does not submit desktop product analytics or subscribe to analytics policy', () => {
  expect(desktop).not.toContain('product-analytics/types')
  expect(desktop).not.toContain('welcomeBackend.analyticsEnabled')
  expect(desktop).not.toContain('welcomeBackend?.report')
  expect(desktop).not.toContain('track(')
  expect(desktop).toContain('analyticsEnabled: () => Promise.resolve(false)')
})

it('never retains raw Host stderr or operation diagnostics', () => {
  for (const marker of ['parsePluginOperationRecord', 'onPluginOperation', 'private stderr', 'MAX_HOST_DIAGNOSTIC_CHARS', 'message.diagnostic']) expect(hostProcess).not.toContain(marker)
  expect(hostProcess).toContain('child.stderr?.resume()')
  expect(hostProcess).toContain("!('diagnostic' in candidate)")
})

it('keeps native recovery without any diagnostic report persistence interface', () => {
  const recovery = readFileSync(new URL('../src/fatal-recovery.ts', import.meta.url), 'utf8')
  for (const marker of ['writeReport', 'revealReport', 'reportState', 'CRASH_REPORT_WAIT_MS', 'getDiagnosticReport']) {
    expect(recovery).not.toContain(marker)
  }
  expect(recovery).toContain('this.operations.stop()')
  expect(recovery).toContain('this.operations.disablePlugins()')
  expect(recovery).toContain('this.operations.restart()')
})

it('does not restore automatic diagnostic file output', () => {
  expect(desktop).not.toContain('DSH_DESKTOP_DIAGNOSTIC_FILE')
})

it('keeps portable releases disconnected from inherited update and mandatory-policy feeds', () => {
  const coordinator = readFileSync(new URL('../src/update-coordinator.ts', import.meta.url), 'utf8')
  const builder = readFileSync(new URL('../delta-electron-builder.config.mjs', import.meta.url), 'utf8')
  const packager = readFileSync(new URL('../scripts/package-delta-portable.mjs', import.meta.url), 'utf8')
  expect(desktop).toContain("const policyInput: unknown = process.env.DELTA_PORTABLE === '1' ? undefined")
  expect(coordinator).toContain("process.env.DELTA_PORTABLE !== '1' && existsSync")
  expect(builder).toContain('publish: null')
  expect(builder).toContain("existsSync(join(resourcesDir, 'app-update.yml'))")
  expect(builder).not.toContain('operation-report-worker')
  expect(packager).toContain("existsSync(join(electronBuilderOutput, 'resources', 'app-update.yml'))")
})
