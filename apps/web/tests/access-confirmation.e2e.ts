// Full access remains behind an explicit acknowledgement in the built Web UI.
import type { Browser, Page } from 'playwright'
import { chromium } from 'playwright'
import { afterAll, beforeAll, describe, expect, it, onTestFailed } from 'vitest'
import {
  captureStableAria, compareOrRefreshGolden, launchWebScaffold, watchConsole, webSnapshotMode, type WebScaffold,
} from './scaffold.ts'
import { assertWarningDialogLayout, connectFreshWorkspaceZh, saveFailureShot, ZH_BROWSER_LOCALE } from './support.ts'

const MODE = webSnapshotMode()
const UI_EXPECTED = fileURLToPath(new URL('./expected/access-confirmation/ui.expected.md', import.meta.url))

describe('web e2e: Full access confirmation', () => {
  let scaffold: WebScaffold
  let browser: Browser
  let page: Page
  let tripwire: ReturnType<typeof watchConsole>

  beforeAll(async () => {
    scaffold = await launchWebScaffold()
    const executablePath = process.env.DSH_PLAYWRIGHT_EXECUTABLE_PATH
    browser = await chromium.launch(executablePath === undefined ? {} : { executablePath })
    page = await browser.newPage({ viewport: { width: 1680, height: 1000 }, locale: ZH_BROWSER_LOCALE, colorScheme: 'light' })
    tripwire = watchConsole(page)
    await page.goto(scaffold.authenticatedUrl, { waitUntil: 'load' })
    await page.waitForSelector('[class*="frame"]', { timeout: 30_000 })
    await connectFreshWorkspaceZh(page, scaffold.workspaceCwd)
  }, 120_000)

  afterAll(async () => {
    await browser?.close()
    await scaffold?.close()
  })

  it('requires acknowledgement before enabling Full access', async () => {
    onTestFailed(() => saveFailureShot(page, 'web-e2e-full-access-confirmation'))
    const access = page.locator('button[aria-label^="访问模式"]').first()
    await access.waitFor({ timeout: 10_000 })
    expect(await access.getAttribute('aria-label')).toBe('访问模式，当前：工作区内修改')

    await access.click()
    await page.getByRole('menuitem', { name: '完全权限' }).click()
    const dialog = page.getByRole('dialog', { name: '确认启用完全权限？' })
    await dialog.waitFor({ timeout: 10_000 })
    await assertWarningDialogLayout(dialog)
    const enable = dialog.getByRole('button', { name: '启用完全权限' })
    expect(await enable.isDisabled()).toBe(true)
    expect(await dialog.evaluate(node => node.parentElement?.parentElement === document.body)).toBe(true)
    await compareOrRefreshGolden(
      UI_EXPECTED,
      await captureStableAria(page, '[role="dialog"]', scaffold.workspaceCwd), MODE,
    )
    await dialog.getByRole('checkbox', { name: '我已了解风险，并愿意继续' }).check()
    expect(await enable.isEnabled()).toBe(true)
    await enable.click()
    await expect.poll(() => access.getAttribute('aria-label'), { timeout: 10_000 })
      .toBe('访问模式，当前：完全权限')
    expect(await dialog.count()).toBe(0)
    expect(tripwire.pageErrors).toEqual([])
  }, 60_000)
})
import { fileURLToPath } from 'node:url'
