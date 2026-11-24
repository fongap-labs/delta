/** Build and stage the Windows x64 App/Data/Other portable directory. */
import { createHash } from 'node:crypto'
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { isAbsolute, join, relative, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import { desktopTargetBuildPaths } from './desktop-build-paths.mjs'

const appRoot = resolve(import.meta.dirname, '..')
const repositoryRoot = resolve(appRoot, '..', '..')
const paths = desktopTargetBuildPaths('win-x64')
const outputsRoot = resolve(repositoryRoot, '..', '..', 'outputs')
const outputRoot = resolve(process.env.DELTA_OUTPUT_DIR ?? join(outputsRoot, 'Delta-v1-win-x64'))
const electronBuilderOutput = join(paths.root, 'delta-electron-builder', 'win-unpacked')
const appOutput = join(outputRoot, 'App')
const dataOutput = join(outputRoot, 'Data')
const otherOutput = join(outputRoot, 'Other')
const packageJson = JSON.parse(readFileSync(join(appRoot, 'package.json'), 'utf8'))
const releaseVersion = process.env.DELTA_RELEASE_VERSION
if (releaseVersion !== undefined && !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-beta\.[1-9]\d*)?$/.test(releaseVersion)) {
  throw new Error('Invalid Delta release version')
}
const node = process.execPath
const tsx = resolve(repositoryRoot, 'node_modules', 'tsx', 'dist', 'cli.mjs')
const pnpm = resolve(repositoryRoot, 'node_modules', 'pnpm', 'bin', 'pnpm.cjs')
process.env.npm_execpath = pnpm

function run(label, args, cwd) {
  process.stdout.write(`delta portable: ${label}\n`)
  const result = spawnSync(node, [tsx, ...args], { cwd, stdio: 'inherit', env: process.env, windowsHide: true })
  if (result.error) throw result.error
  if (result.status !== 0) throw new Error(`delta portable: ${label} failed (${String(result.status)})`)
}

function runPnpm(label, args, cwd) {
  process.stdout.write(`delta portable: ${label}\n`)
  const result = spawnSync(node, [pnpm, ...args], { cwd, stdio: 'inherit', env: process.env, windowsHide: true })
  if (result.error) throw result.error
  if (result.status !== 0) throw new Error(`delta portable: ${label} failed (${String(result.status)})`)
}

function filesUnder(directory, root = directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = join(directory, entry.name)
    return entry.isDirectory() ? filesUnder(path, root) : [relative(root, path)]
  })
}

if (process.platform !== 'win32' || process.arch !== 'x64') throw new Error('delta portable packaging requires Windows x64')
const outputRelative = relative(outputsRoot, outputRoot)
if (!outputRelative || outputRelative.startsWith('..') || isAbsolute(outputRelative)) {
  throw new Error(`delta portable: output must stay under outputs/: ${outputRoot}`)
}

if (existsSync(outputRoot) && readdirSync(outputRoot).length > 0) {
  if (process.env.DELTA_REPLACE_OUTPUT !== '1') {
    throw new Error(`delta portable: output directory is not empty: ${outputRoot}; set DELTA_REPLACE_OUTPUT=1 to replace it`)
  }
  rmSync(outputRoot, { recursive: true, force: true })
}
mkdirSync(outputRoot, { recursive: true })
delete process.env.DSH_CLIENT_TITLE
delete process.env.DSH_CLIENT_BUILD_PROFILE
delete process.env.DSH_BUILD_CLIENT_PROFILE
if (process.env.DELTA_SKIP_BUILD !== '1') run('build Delta artifacts', ['scripts/build.ts', '--profile', 'local', '--scope', 'artifacts', '--serial'], repositoryRoot)
if (process.env.DELTA_REUSE_ELECTRON_OUTPUT !== '1') {
  if (process.env.DELTA_REUSE_PREPARED_RUNTIME !== '1') {
    run('prepare primary runtime', ['apps/desktop/scripts/prepare-primary-runtime.ts'], repositoryRoot)
    run('prepare runtime', ['apps/desktop/scripts/prepare-runtime.ts'], repositoryRoot)
  } else if (!existsSync(join(paths.runtime, 'versions.json'))
    || !existsSync(join(paths.runtime, 'primary-runtime', 'runtime.json'))
    || !existsSync(join(paths.electron, 'electron.exe'))) {
    throw new Error('delta portable: requested runtime reuse, but prepared runtime inputs are incomplete')
  }
  run('pack Delta runtime packages', ['apps/desktop/scripts/pack-local-packages.ts', '--out', paths.packedDsh], repositoryRoot)
  run('pack bundled vendor packages', ['scripts/release/pack.ts', '--family', 'vendor', '--out', paths.packedVendor], repositoryRoot)
  rmSync(paths.packedLandlock, { recursive: true, force: true })
  mkdirSync(paths.packedLandlock, { recursive: true })
  runPnpm('build native package metadata', ['--dir', 'native/system', 'run', 'build:ts'], repositoryRoot)
  runPnpm('pack native entry package', ['--dir', 'native/system/packages/entry', 'pack', '--pack-destination', paths.packedLandlock], repositoryRoot)
  run('assemble production package set', ['apps/desktop/scripts/prepare-package-set.ts', '--pack-missing'], repositoryRoot)
  run('prepare application runtime', ['apps/desktop/scripts/prepare-dsh.ts'], repositoryRoot)
  const builder = spawnSync(node, [resolve(appRoot, 'node_modules', 'electron-builder', 'cli.js'), '--config',
    'delta-electron-builder.config.mjs', '--win', '--x64', '--dir'], {
    cwd: appRoot, stdio: 'inherit', env: process.env, windowsHide: true,
  })
  if (builder.error) throw builder.error
  if (builder.status !== 0) throw new Error(`delta portable: Electron packaging failed (${String(builder.status)})`)
} else if (!existsSync(join(electronBuilderOutput, 'Delta.exe'))
  || !existsSync(join(electronBuilderOutput, 'resources', 'app.asar'))) {
  throw new Error(`delta portable: reusable Electron output is incomplete: ${electronBuilderOutput}`)
}

cpSync(electronBuilderOutput, appOutput, { recursive: true })
for (const path of [join(dataOutput, 'UserData'), join(dataOutput, 'Session'), join(dataOutput, 'Logs'),
  join(dataOutput, 'Temp'), join(dataOutput, 'Home')]) mkdirSync(path, { recursive: true })
mkdirSync(otherOutput, { recursive: true })
cpSync(join(repositoryRoot, 'LICENSE'), join(otherOutput, 'LICENSE'))
cpSync(join(repositoryRoot, 'LICENSES', 'DEEPSEEK-HARNESS-MIT.txt'), join(otherOutput, 'DEEPSEEK-HARNESS-MIT.txt'))
cpSync(join(repositoryRoot, 'THIRD_PARTY_NOTICES.md'), join(otherOutput, 'THIRD_PARTY_NOTICES.md'))
writeFileSync(join(otherOutput, 'VERSION.json'), `${JSON.stringify({
  product: 'Delta',
  version: releaseVersion ?? packageJson.version,
  harnessSource: process.env.DELTA_HARNESS_SHA ?? '4b399c39a6d1479cd84ca591b4d7c88e24d38a96',
  platform: 'win32-x64',
  electron: JSON.parse(readFileSync(join(appRoot, 'node_modules', 'electron', 'package.json'), 'utf8')).version,
  runtime: JSON.parse(readFileSync(join(paths.runtime, 'versions.json'), 'utf8')),
}, null, 2)}\n`)
writeFileSync(join(otherOutput, 'README.md'), `# Delta Windows x64 便携版\n\n- 启动桌面应用：运行上级目录中的 \`App\\Delta.exe\`。\n- 启动命令行：在 \`App\` 目录运行 \`delta.cmd --help\`；把 \`App\` 加入 PATH 后也可直接输入 \`delta\`。\n- 用户数据、会话、日志、缓存与 Harness Home 位于同级 \`Data\` 目录。移动整个 Delta 目录时数据随目录移动。\n- \`Other\\VERSION.json\` 记录 Delta 版本、Harness 来源提交与目标平台。\n\n这是便携目录构建，当前没有配置 Delta 自动更新源或代码签名证书。\n`)
writeFileSync(join(appOutput, 'delta.cmd'), [
  '@echo off',
  'setlocal',
  'set "DELTA_CLI_NAME=delta"',
  'set "DELTA_PORTABLE_ROOT=%~dp0.."',
  'set "DSH_HOME=%~dp0..\\Data\\Home"',
  'set "DSH_DESKTOP_USER_DATA_DIR=%~dp0..\\Data\\UserData"',
  'set "ELECTRON_RUN_AS_NODE=1"',
  '"%~dp0Delta.exe" --expose-internals "%~dp0resources\\app.asar\\dsh\\node_modules\\@deepseek-ai\\dsh-desktop-host\\lib\\cli.js" %*',
  'endlocal',
  '',
].join('\r\n'))
const hashes = [...filesUnder(appOutput).map(file => `App/${file}`),
  'Other/README.md', 'Other/VERSION.json', 'Other/LICENSE', 'Other/DEEPSEEK-HARNESS-MIT.txt',
  'Other/THIRD_PARTY_NOTICES.md']
  .sort()
  .map(file => `${createHash('sha256').update(readFileSync(join(outputRoot, file))).digest('hex')}  ${file}`)
writeFileSync(join(otherOutput, 'SHA256SUMS.txt'), `${hashes.join('\n')}\n`)
process.stdout.write(`delta portable: staged ${outputRoot}\n`)
