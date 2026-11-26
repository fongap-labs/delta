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
