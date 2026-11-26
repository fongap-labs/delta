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
const sessionLog = rowBlock(corePatch, 'session-log-deepseek')
if (!sessionLog || !/^\s+disabled:\s*true\s*$/m.test(sessionLog)) {
  throw new Error('delta-verify: session-log upload must remain disabled')
}
for (const rowId of ['session-telemetry-otel', 'desktop-product-telemetry']) {
  if (rowBlock(corePatch, rowId) || rowBlock(webPatch, rowId)) {
    throw new Error(`delta-verify: forbidden telemetry row is mounted: ${rowId}`)
  }
}
if (rowBlock(webPatch, 'product-analytics')) {
  throw new Error('delta-verify: product analytics must not be assembled')
}

// Delta must not reconnect the native collection paths through upstream sync.
const desktopMain = readFileSync(requireFile('apps/desktop/src/main.ts', 'desktop main'), 'utf8')
const prohibitedDesktopRuntime = [
  /\banalyticsEnabled\s*:/u,
  /\bnew\s+OperationJournal\b/u,
  /\bnew\s+RendererConsoleTail\b/u,
  /\b(?:writeCrashReport|runOperationReportJob|persistCrashReport)\s*\(/u,
  /\bwelcomeBackend\?*\.\s*(?:report|analyticsEnabled)\s*\(/u,
  /\banalytics\s*:\s*track\b/u,
]
if (prohibitedDesktopRuntime.some(pattern => pattern.test(desktopMain))) {
  throw new Error('delta-verify: native telemetry or automatic diagnostic collection must not be restored')
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
for (const marker of ["join(outputRoot, 'App')", "join(outputRoot, 'Data')", "join(outputRoot, 'Other')",
  "'Other/THIRD_PARTY_NOTICES.md'", "'Other/LICENSE'"]) {
  if (!portablePackager.includes(marker)) {
    throw new Error(`delta-verify: portable packager is missing ${marker}`)
  }
}

process.stdout.write('Delta static source and portable-product checks passed.\n')
