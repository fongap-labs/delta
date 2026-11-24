/** Unsupported system languages use the first-party English UI fallback. */
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { chromium, type Browser } from 'playwright'
import { expect, it, onTestFailed, onTestFinished } from 'vitest'
import { launchWebScaffold, watchConsole, type WebScaffold } from './scaffold.ts'
import { openSettings, saveFailureShot } from './support.ts'

it.each(['ja-JP', 'fr-FR', 'pt-BR'])('offers only shipped locales for system language %s', async (locale) => {
  const resources: { scaffold?: WebScaffold; browser?: Browser } = {}
  onTestFinished(async () => {
    try { await resources.browser?.close() } finally { await resources.scaffold?.close() }
  })
  const scaffold = await launchWebScaffold()
  resources.scaffold = scaffold
  const browser = await chromium.launch()
  resources.browser = browser
  const page = await browser.newPage({ locale, viewport: { width: 1440, height: 960 } })
  const tripwire = watchConsole(page)
  onTestFailed(() => saveFailureShot(page, `unsupported-system-locale-${locale}`))
  await page.goto(scaffold.authenticatedUrl)
  await expect.poll(() => page.locator('html').getAttribute('lang')).toBe('en')

  await openSettings(page, 'en')
  const dialog = page.getByRole('dialog', { name: 'Settings', exact: true })
  const selector = dialog.getByRole('button', { name: 'Follow system', exact: true })
  await selector.click()
  expect(await page.getByRole('menuitem').allTextContents()).toEqual(['Follow system', '中文', 'English'])
  await page.getByRole('menuitem', { name: '中文', exact: true }).click()
  await expect.poll(() => page.locator('html').getAttribute('lang')).toBe('zh-CN')
  await expect.poll(() => readFile(join(scaffold.harnessHome, 'profiles', 'scaffold', 'cordis.patch.yml'), 'utf8'))
    .toContain('preference: zh')

  await page.getByRole('button', { name: '中文', exact: true }).click()
  await page.getByRole('menuitem', { name: '跟随系统', exact: true }).click()
  await expect.poll(() => readFile(join(scaffold.harnessHome, 'profiles', 'scaffold', 'cordis.patch.yml'), 'utf8'))
    .not.toContain('preference: zh')
  await page.reload()
  await expect.poll(() => page.locator('html').getAttribute('lang')).toBe('en')
  expect(tripwire.warnings).toEqual([])
  expect(tripwire.pageErrors).toEqual([])
})
