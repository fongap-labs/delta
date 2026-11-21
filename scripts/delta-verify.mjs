import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const read = path => readFileSync(resolve(root, path))
const text = path => read(path).toString('utf8')
const hash = path => createHash('sha256').update(read(path)).digest('hex')
const asset = 'branding/legacy/delta-logo-512x512.png'
for (const path of ['apps/desktop/resources/icon.png', 'apps/desktop/resources/icon-windows.png',
  'apps/desktop/resources/icon-macos.png', 'apps/desktop/renderer/assets/delta-logo-512x512.png']) {
  assert.equal(hash(path), hash(asset), 'brand drift: ' + path)
}
assert.equal(hash('packages/client/ui-brand-official/src/client/assets/delta-logo-256x256.png'),
  hash('branding/legacy/delta-logo-256x256.png'))
assert.match(text('apps/desktop/src/client/WelcomePage.tsx'), /delta-brand-logo/)
assert.match(text('apps/desktop/renderer/welcome.css'), /Delta legacy v0\.0\.1 visual treatment/)
const build = text('apps/desktop/scripts/electron-builder-config.mjs')
assert.match(build, /productName: 'Delta'/)
assert.match(build, /CFBundleLocalizations: \['en', 'zh_CN'\]/)
assert.match(build, /schemes: \['dsh'\]/, 'Preserve the Harness protocol')
const locale = text('packages/client/locale/src/locale-settings.ts')
assert.match(locale, /LOCALE_IDS = \['zh', 'en'\]/)
assert.match(text('apps/desktop/src/locale.ts'), /const dictionaries = \{ en, zh \}/)
assert.match(text('apps/desktop/src/client/WelcomePage.tsx'), /startSignIn\(\)/,
  'Must retain the Harness sign-in flow')
// A clean Delta profile enters the workspace directly; model setup is custom-only.
assert.match(text('apps/desktop/src/welcome-api.ts'),
  /export function needsWelcome\(_authentication: WelcomeAuthentication\): boolean \{\s*return false\s*\}/)
assert.match(text('packaging/portable/build-portable-package.ps1'), /Delta-Windows-Portable\.zip/)
const lock = JSON.parse(text('upstream.lock.json'))
assert.equal(lock.baseline_sha, '2e39e70496aa02986ca6a820c5d497e816b69d7e')
const manifest = JSON.parse(text('release.manifest.json'))
assert.equal(manifest.root, '.')
assert.equal(manifest.builds[0].assets[0], 'delta-windows-x64-portable.zip')
console.log('Delta branding, packaging contract, locale and baseline assertions passed')

assert.match(text('packaging/portable/launcher/src/main.rs'), /"DSH_HOME"/)
assert.match(text('packaging/portable/launcher/src/main.rs'), /"DSH_AGENTS_HOME"/)

const modelAdapter = text('packages/llm/llm-deepseek/src/adapter.ts')
assert.doesNotMatch(modelAdapter, /x-deepseek-harness-user-id/)
assert.doesNotMatch(modelAdapter, /x-deepseek-harness-session-id/)
const basePatch = text('packages/bundle/base/cordis.patch.yml')
// Delta exposes only user-declared model API routes by default.
assert.doesNotMatch(basePatch, /^    - id: (deepseek-llm-api-extensions|llm-deepseek|llm-deepseek-account|llm-deepseek-api-key)$/m)
assert.match(basePatch, /^    - id: llm-pi-ai$/m)
assert.equal((basePatch.match(/^    - id: llm-pi-ai$/gm) ?? []).length, 1,
  'Delta base must have exactly one generic custom-model adapter')
const minimalSdkPatch = text('packages/bundle/sdk-minimal/cordis.patch.yml')
assert.match(minimalSdkPatch, /^    - id: llm-pi-ai$/m)
assert.equal((minimalSdkPatch.match(/^    - id: llm-pi-ai$/gm) ?? []).length, 1,
  'Delta SDK must have exactly one generic custom-model adapter')
assert.doesNotMatch(minimalSdkPatch, /^    - id: (deepseek-llm-api-extensions|deepseek-auth|llm-deepseek|llm-deepseek-account|llm-deepseek-api-key)$/m)
assert.doesNotMatch(text('packages/bundle/sdk-minimal/package.json'), /"@deepseek-ai\/dsh-deepseek-(auth|llm-api-extensions)":/)
assert.doesNotMatch(text('packages/bundle/base/package.json'), /"@deepseek-ai\/dsh-deepseek-llm-api-extensions":/)
const sidebarMarkup = text('packages/client/ui-sidebar/src/client/SidebarRoot.tsx')
assert.match(sidebarMarkup, /css\.headerRow/)
assert.doesNotMatch(sidebarMarkup, /sidebar\.brand\.(mark|name)|css\.brand(Mark|Name|Identity)?|css\.logoRow/)
assert.doesNotMatch(text('packages/client/ui-sidebar/src/client/SidebarRoot.module.css'), /\.brand(Name|Mark|Identity)?\s*\{|\.railMark\s*\{/)
assert.match(text('packages/client/ui-settings-models/src/client/ModelsSection.tsx'), /<CustomProviderCard/)
const piCatalog = text('packages/llm/llm-pi-ai/src/catalog.ts')
assert.match(piCatalog, /export function catalogProviderIds\(\): readonly string\[\] \{ return \[\] \}/)
const customTransports = text('packages/llm/llm-pi-ai/src/provider.ts')
for (const protocol of ['openai-completions', 'openai-responses', 'anthropic-messages', 'google-generative-ai']) {
  assert.match(customTransports, new RegExp(`^  '${protocol}': `, 'm'),
    `missing custom model API transport: ${protocol}`)
}
const modelsCopy = text('packages/client/ui-settings-models/src/client/locales.ts')
assert.match(modelsCopy, /welcomeBody: 'Delta does not include a preconfigured model provider/)
assert.match(modelsCopy, /welcomeBody: 'Delta \u4e0d\u9884\u88c5\u6a21\u578b\u63d0\u4f9b\u5546/)
assert.doesNotMatch(basePatch, /^    - id: (session-log-deepseek|session-telemetry-otel|otel)$/m)
const webPatch = text('packages/bundle/web-app/cordis.patch.yml')
assert.doesNotMatch(webPatch, /^    - id: (desktop-product-telemetry|product-analytics|ui-settings-session-log)$/m)
const desktopMain = text('apps/desktop/src/main.ts')
assert.doesNotMatch(desktopMain, /writeCrashReport|persistCrashReport|productAnalytics\/report|productAnalytics\/watchPolicy|rendererConsole|OperationJournal/)

import { existsSync } from 'node:fs'
for (const path of [
  'apps/desktop/src/crash-report.ts',
  'apps/desktop/src/operation-history.ts',
  'apps/desktop/src/operation-journal.ts',
  'apps/desktop/src/operation-report.ts',
  'apps/desktop/src/operation-report-job.ts',
  'apps/desktop/src/operation-report-worker.ts',
]) assert.equal(existsSync(resolve(root, path)), false, `privacy module still present: ${path}`)

// The optional diagnostics profile must not expose raw Session-log inspection.
const inspectorPatch = text('packages/experimental/inspector-profile/cordis.patch.yml')
assert.doesNotMatch(inspectorPatch, /- id: session-inspector/)
assert.match(inspectorPatch, /captureFetch: false/)

// ZIP and Markdown are user-initiated local exports, not outbound telemetry.
assert.match(webPatch, /^    - id: session-log-download$/m)
const exportHost = text('packages/session-query/session-log-export/src/index.ts')
const exportClient = text('packages/session-query/session-log-export/src/client/index.ts')
assert.match(exportHost, /SESSION_LOG_EXPORT_PATH/)
assert.match(exportHost, /SESSION_MARKDOWN_EXPORT_PATH/)
assert.match(exportClient, /controller\.download\(sessionId\)/)
assert.match(exportClient, /copyController\.copy\(sessionId\)/)
assert.doesNotMatch(basePatch, /^    - id: session-log-deepseek$/m)

// Match the historical Windows portable root layout while keeping Electron inside App.
const portableBuild = text('packaging/portable/build-portable-package.ps1')
assert.match(portableBuild, /Join-Path \$delta "App\\Delta"/)
assert.match(portableBuild, /"Delta\/Data\/"/)
assert.match(portableBuild, /"Delta\/Other\/"/)
assert.match(portableBuild, /SelectSingleNode\('\/appinfo\/version'\)/)
assert.match(portableBuild, /sign-windows-binaries\.ps1/)
assert.equal((portableBuild.match(/Central asset checksum mismatch/g) ?? []).length, 1,
  'ZIP checksum validation must have exactly one final guard')
assert.equal((portableBuild.match(/Write-Host "Delta Portable asset:/g) ?? []).length, 1,
  'ZIP completion should be emitted only once')
assert.match(JSON.stringify(JSON.parse(text('.github/execution-manifest.json'))), /central-ci-verify\.sh/)
assert.equal(manifest.builds[0].script, 'packaging/portable/release-build-package.ps1')
assert.match(text('.github/scripts/central-ci-verify.sh'), /node scripts\\/delta-verify\\.mjs/)
assert.match(text('packaging/portable/release-build-package.ps1'), /build-portable-package\\.ps1/)


assert.match(text('packaging/portable/launcher/src/main.rs'), /root\.join\("App"\)\.join\("Delta"\)\.join\("Delta.exe"\)/)

const privacy = text('apps/desktop/src/privacy-redaction.ts')
assert.match(privacy, /redactDiagnostic/)
assert.match(text('apps/desktop/src/startup-error.ts'), /redactDiagnostic\(message\)/)
assert.doesNotMatch(text('apps/desktop/src/main.ts'), /console\.error\(error\)\n  if \(shuttingDown\)/)

// No dormant collector packages or telemetry opt-out plumbing remain.
import { existsSync as packageExists } from 'node:fs'
const removedCollectorPackages = [
  'packages/client/product-analytics',
  'packages/client/ui-settings-session-log',
  'packages/host/product-telemetry-otel',
  'packages/session/session-telemetry',
  'packages/session/session-telemetry-otel',
  'packages/telemetry/otel',
  'packages/identity/anonymous-user-id',
  'packages/session/session-log-deepseek',
]
for (const path of removedCollectorPackages) {
  assert.equal(packageExists(resolve(root, path)), false, `privacy collector still present: ${path}`)
}
assert.doesNotMatch(text('packages/boot/app-boot/src/profile-context.ts'), /telemetryDisabledEnv|resolveTelemetryPatch/)
assert.doesNotMatch(text('packages/api/remotes/src/client/index.ts'), /productAnalytics/)

assert.doesNotMatch(basePatch, /^    - id: plugin-package-inventory-deepseek$/m)
assert.doesNotMatch(text('packages/bundle/sdk-minimal/cordis.patch.yml'), /^    - id: plugin-package-inventory-deepseek$/m)
assert.equal(existsSync(resolve(root, 'packages/llm/plugin-package-inventory-deepseek')), false)
const extensionRegistry = text('packages/llm/deepseek-llm-api-extensions/src/index.ts')
assert.match(extensionRegistry, /fieldName === 'dsh_session_log'/)
assert.match(extensionRegistry, /fieldName === 'dsh_plugin_packages'/)
const exportRoutes = text('packages/session-query/session-log-export/src/index.ts')
assert.match(exportRoutes, /'cache-control': 'no-store'/)
assert.match(exportRoutes, /'x-content-type-options': 'nosniff'/)
assert.doesNotMatch(text('packages/account/deepseek-account/src/protocol.ts'), /\/api\/v0\/users\/notifications\/events/)
assert.doesNotMatch(text('packages/account/deepseek-account/src/index.ts'), /reportNotificationEvent/)

assert.equal(existsSync(resolve(root, 'apps/desktop/src/update-journal.ts')), false)
const mainPrivacy = text('apps/desktop/src/main.ts')
assert.match(mainPrivacy, /function logPrivateError/)
assert.doesNotMatch(mainPrivacy, /console\.error\(error\)/)
assert.doesNotMatch(mainPrivacy, /console\.error\(failure\)/)

const accountController = text('packages/client/ui-account-notifications/src/client/notification-controller.ts')
assert.doesNotMatch(accountController, /reportEvent|ctaClicked|requestNotificationEvent|sendNotificationEvent/)
assert.doesNotMatch(text('packages/llm/llm-deepseek/src/adapter.ts'), /x-deepseek-harness-(user-id|session-id)/)
assert.doesNotMatch(text('apps/desktop/src/main.ts'), /DSH_DESKTOP_UPDATE_JOURNAL_DIR|DesktopUpdateJournal/)
