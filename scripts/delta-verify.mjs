/** Static Delta source and product-policy gate; it never installs or executes project code. */
import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const args = process.argv.slice(2)
if (args.length > 1 || (args.length === 1 && args[0] !== '--ci')) {
  process.stderr.write('delta-verify: accepted arguments are --ci or none\n')
  process.exit(64)
}

function requireFile(path, label) {
  const fullPath = join(root, path)
  if (!existsSync(fullPath)) throw new Error(`delta-verify: missing ${label}`)
  return fullPath
}

function rowBlock(source, id) {
  const lines = source.split(/\r?\n/)
  const start = lines.findIndex(line => line.trim() === `- id: ${id}`)
  if (start < 0) return ''
  let end = start + 1
  while (end < lines.length && !/^\s{4}- id: /.test(lines[end] ?? '')) end += 1
  return lines.slice(start, end).join('\n')
}

const lockPath = requireFile('upstream.lock.json', 'upstream lock')
const lock = JSON.parse(readFileSync(lockPath, 'utf8'))
if (lock.schemaVersion !== 1
  || lock.upstream?.repository !== 'https://github.com/deepseek-ai/deepseek-harness'
  || lock.upstream?.commit !== '4b399c39a6d1479cd84ca591b4d7c88e24d38a96'
  || lock.deltaImport?.baseCommit !== '3effeb139e9af5198bfa5d380b1d3457caea4fcb') {
  throw new Error('delta-verify: source lock does not match the approved first-import baseline')
}

if (existsSync(join(root, 'packages', 'experimental'))) {
  throw new Error('delta-verify: Harness experimental package tree must not be shipped')
}

const workspaceManifests = [
  'package.json',
  ...readdirSync(join(root, 'apps'), { withFileTypes: true })
    .filter(entry => entry.isDirectory())
    .map(entry => join('apps', entry.name, 'package.json')),
  ...readdirSync(join(root, 'packages'), { withFileTypes: true })
    .filter(entry => entry.isDirectory())
    .flatMap(group => readdirSync(join(root, 'packages', group.name), { withFileTypes: true })
      .filter(entry => entry.isDirectory())
      .map(entry => join('packages', group.name, entry.name, 'package.json'))),
  ...readdirSync(join(root, 'vendor'), { withFileTypes: true })
    .filter(entry => entry.isDirectory())
    .map(entry => join('vendor', entry.name, 'package.json')),
].filter(path => existsSync(join(root, path)))
for (const path of workspaceManifests) {
  if (readFileSync(join(root, path), 'utf8').includes('@deepseek-ai/dsh-experimental-')) {
    throw new Error(`delta-verify: upstream experimental package referenced by workspace manifest: ${path}`)
  }
}

const corePatch = readFileSync(requireFile('packages/bundle/base/cordis.patch.yml', 'base profile patch'), 'utf8')
const webPatch = readFileSync(requireFile('packages/bundle/web-app/cordis.patch.yml', 'Web profile patch'), 'utf8')
const forbiddenPrivacyPackages = [
  'packages/session/session-log-deepseek',
  'packages/llm/plugin-package-inventory-deepseek',
  'packages/client/ui-settings-session-log',
]
for (const path of forbiddenPrivacyPackages) {
  if (existsSync(join(root, path))) throw new Error('delta-verify: forbidden collection package restored: ' + path)
}
for (const path of ['tsconfig.host.json', 'tsconfig.client.json', 'tsconfig.base.json']) {
  const content = readFileSync(requireFile(path, path), 'utf8')
  if (forbiddenPrivacyPackages.some(name => content.includes(name))) {
    throw new Error('delta-verify: forbidden collection project reference restored: ' + path)
  }
}
const forbiddenPrivacyDependencies = [
  '@deepseek-ai/dsh-session-log-deepseek',
  '@deepseek-ai/dsh-plugin-package-inventory-deepseek',
  '@deepseek-ai/dsh-client-ui-settings-session-log',
]
for (const path of [...workspaceManifests, 'pnpm-lock.yaml']) {
  const content = readFileSync(requireFile(path, path), 'utf8')
  if (forbiddenPrivacyDependencies.some(name => content.includes(name))) {
    throw new Error('delta-verify: forbidden collection dependency restored: ' + path)
  }
}
const desktopHostProcess = readFileSync(requireFile('apps/desktop/src/host-process.ts', 'desktop Host process'), 'utf8')
for (const marker of ['parsePluginOperationRecord', 'onPluginOperation', 'private stderr', 'MAX_HOST_DIAGNOSTIC_CHARS', 'message.diagnostic']) {
  if (desktopHostProcess.includes(marker)) throw new Error('delta-verify: automatic Host diagnostic collection restored')
}
const fatalRecovery = readFileSync(requireFile('apps/desktop/src/fatal-recovery.ts', 'desktop fatal recovery'), 'utf8')
for (const marker of ['writeReport', 'revealReport', 'reportState', 'CRASH_REPORT_WAIT_MS', 'getDiagnosticReport']) {
  if (fatalRecovery.includes(marker)) throw new Error('delta-verify: automatic recovery report interface restored')
}
const forbiddenDiagnosticFiles = [
  'apps/desktop-host/src/fatal-diagnostic.ts',
  'apps/desktop-host/src/operation-diagnostics.ts',
  'apps/desktop/src/crash-report.ts',
  'apps/desktop/src/operation-history.ts',
  'apps/desktop/src/operation-journal.ts',
  'apps/desktop/src/operation-report-job.ts',
  'apps/desktop/src/operation-report-worker.ts',
  'apps/desktop/src/operation-report.ts',
  'packages/client/ui-settings-account/src/contact-config.ts',
  'packages/client/ui-settings-account/src/client/contact-url.ts',
]
for (const path of forbiddenDiagnosticFiles) {
  if (existsSync(join(root, path))) throw new Error('delta-verify: forbidden privacy collection source: ' + path)
}
const desktopMain = readFileSync(join(root, 'apps/desktop/src/main.ts'), 'utf8')
if (desktopMain.includes('DSH_DESKTOP_DIAGNOSTIC_FILE')) throw new Error('delta-verify: automatic diagnostic export restored')
const accountClient = readFileSync(join(root, 'packages/client/ui-settings-account/src/client/index.ts'), 'utf8')
for (const marker of ['contactUrl(', 'deviceInfo', 'CONTACT_CONFIG_GLOBAL']) {
  if (accountClient.includes(marker)) throw new Error('delta-verify: contextual feedback collection restored: ' + marker)
}
const accountProtocol = readFileSync(join(root, 'packages/account/deepseek-account/src/protocol.ts'), 'utf8')
if (accountProtocol.includes('/api/v0/users/notifications/events')) throw new Error('delta-verify: interaction reporting restored')
if (rowBlock(webPatch, 'ui-account-notifications')) throw new Error('delta-verify: automatic account notification polling restored')
const extensionRegistry = readFileSync(join(root, 'packages/llm/deepseek-llm-api-extensions/src/index.ts'), 'utf8')
const extensionSerializer = readFileSync(join(root, 'packages/llm/llm-deepseek/src/request-extensions.ts'), 'utf8')
if (!extensionRegistry.includes('!isApprovedDeepSeekRequestExtension(fieldName)') || !extensionSerializer.includes('!APPROVED_MESSAGES_FIELDS.has(field)') || !extensionSerializer.includes('!isApprovedDeepSeekRequestExtension(field)')) {
  throw new Error('delta-verify: model request allowlist enforcement missing')
}
for (const path of ['adapter.ts', 'files-api.ts', 'listing.ts']) {
  const source = readFileSync(join(root, 'packages/llm/llm-deepseek/src', path), 'utf8')
  if (!source.includes('approvedDeepSeekAuthHeaders(')) throw new Error('delta-verify: credential header allowlist missing: ' + path)
}

const sdkPatch = readFileSync(requireFile('packages/bundle/sdk-minimal/cordis.patch.yml', 'SDK profile patch'), 'utf8')
for (const patch of [corePatch, webPatch, sdkPatch]) {
  for (const id of ['session-log-deepseek', 'plugin-package-inventory-deepseek', 'ui-settings-session-log']) {
    if (rowBlock(patch, id)) throw new Error('delta-verify: forbidden collection row restored: ' + id)
  }
}
for (const rowId of ['session-telemetry-otel', 'desktop-product-telemetry']) {
  if (rowBlock(corePatch, rowId) || rowBlock(webPatch, rowId)) {
    throw new Error(`delta-verify: forbidden telemetry row is mounted: ${rowId}`)
  }
}
if (rowBlock(webPatch, 'product-analytics')) {
  throw new Error('delta-verify: product analytics must not be assembled')
}

for (const path of [
  'packages/delta/computer-use/package.json',
  'packages/delta/browser-use/package.json',
  'packages/delta/forge/package.json',
  'packages/delta/worktree/package.json',
  'apps/desktop/portable-launcher.mjs',
  'apps/desktop/scripts/package-delta-portable.mjs',
  'docs/delta/portable/README.md',
  'docs/delta/portable/UPGRADE.md',
  'apps/desktop/resources/delta-brand/delta.ico',
  'apps/desktop/resources/delta-brand/delta-logo-256x256.png',
  'apps/desktop/resources/delta-brand/delta-logo-512x512.png',
  'LICENSE',
  'LICENSES/DEEPSEEK-HARNESS-MIT.txt',
  'THIRD_PARTY_NOTICES.md',
]) {
  requireFile(path, path)
}

const releaseManifest = JSON.parse(readFileSync(requireFile('.github/release.manifest.json', 'release manifest'), 'utf8'))
const [releaseBuild] = releaseManifest.builds ?? []
if (releaseManifest.schema_version !== 1
  || releaseManifest.target_repository !== 'fongap-labs/delta'
  || releaseManifest.builds.length !== 1
  || releaseBuild.runner_profile !== 'windows-build'
  || releaseBuild.target !== 'x86_64-pc-windows-msvc'
  || releaseBuild.assets?.join(',') !== 'Delta-win-x64.zip'
  || releaseBuild.attest_asset !== 'Delta-win-x64.zip') {
  throw new Error('delta-verify: invalid Windows x64 release contract')
}
for (const path of [releaseManifest.node_version_file, releaseBuild.script, releaseBuild.acceptance_script]) {
  if (typeof path !== 'string' || !path) throw new Error('delta-verify: incomplete release contract')
  requireFile(path, path)
}

const portablePackager = readFileSync(join(root, 'apps/desktop/scripts/package-delta-portable.mjs'), 'utf8')
const portableBuilder = readFileSync(join(root, 'apps/desktop/delta-electron-builder.config.mjs'), 'utf8')
const updateCoordinator = readFileSync(join(root, 'apps/desktop/src/update-coordinator.ts'), 'utf8')
if (!portableBuilder.includes('publish: null')
  || portableBuilder.includes('operation-report-worker')
  || !portableBuilder.includes("existsSync(join(resourcesDir, 'app-update.yml'))")
  || !portablePackager.includes("existsSync(join(electronBuilderOutput, 'resources', 'app-update.yml'))")
  || !desktopMain.includes("const policyInput: unknown = process.env.DELTA_PORTABLE === '1' ? undefined")
  || !updateCoordinator.includes("process.env.DELTA_PORTABLE !== '1' && existsSync")) {
  throw new Error('delta-verify: portable update isolation missing')
}
for (const marker of ["join(outputRoot, 'App')", "join(outputRoot, 'Data')", "join(outputRoot, 'Other')",
  "'Other/THIRD_PARTY_NOTICES.md'", "'Other/LICENSE'"]) {
  if (!portablePackager.includes(marker)) {
    throw new Error(`delta-verify: portable packager is missing ${marker}`)
  }
}

process.stdout.write('Delta static source and portable-product checks passed.\n')
