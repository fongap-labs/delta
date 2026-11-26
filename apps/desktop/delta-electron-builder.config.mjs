/** Minimal Windows x64 directory build used by Delta's portable package. */
import { join } from 'node:path'
import { existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { desktopTargetBuildPaths, resolveDesktopBuildTarget } from './scripts/desktop-build-paths.mjs'
import { officePackageDirectories } from '../../scripts/libreoffice-packages.mjs'
import { verifyDesktopRuntime } from './lib/types/runtime-tree.js'
import { prepareWindowsAsarUnpack, verifyWindowsAsarUnpack } from './scripts/windows-asar-unpack.mjs'

if (process.platform !== 'win32' || process.arch !== 'x64' || resolveDesktopBuildTarget() !== 'win-x64') {
  throw new Error('Delta portable packaging requires a Windows x64 build host')
}

const appRoot = import.meta.dirname
const repositoryRoot = join(appRoot, '..', '..')
const paths = desktopTargetBuildPaths('win-x64')
const product = JSON.parse(await (await import('node:fs/promises')).readFile(join(appRoot, 'package.json'), 'utf8'))
const dshProduct = JSON.parse(await (await import('node:fs/promises')).readFile(join(repositoryRoot, 'package.json'), 'utf8'))
const releaseVersion = process.env.DELTA_RELEASE_VERSION
if (releaseVersion !== undefined && !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-beta\.[1-9]\d*)?$/.test(releaseVersion)) {
  throw new Error('Invalid Delta release version')
}
let windowsCode = []

export default {
  appId: 'org.fongap.delta',
  productName: 'Delta',
  copyright: `Copyright © ${new Date().getFullYear()} Delta`,
  extraMetadata: {
    main: 'portable-launcher.mjs',
    dshDesktopAppId: 'org.fongap.delta',
    version: releaseVersion ?? dshProduct.version,
  },
  artifactName: 'Delta-${version}-win-x64.${ext}',
  directories: { output: join(paths.root, 'delta-electron-builder') },
  asar: true,
  electronDist: paths.electron,
  electronFuses: { runAsNode: true },
  files: [
    'portable-launcher.mjs',
    'lib/main.js', 'lib/welcome/**/*',
    'lib/preload-app.cjs', 'lib/preload-mandatory.cjs', 'lib/preload-platform-account.cjs',
    'lib/preload-update-dialog.cjs', 'lib/preload-welcome.cjs',
    'renderer/**/*', 'package.json',
    '!node_modules/**/*.{js,cjs,mjs,ts,cts,mts,css}.map',
    '!node_modules/**/*.tsbuildinfo',
    { from: paths.dsh, to: 'dsh', filter: ['**/*'] },
    { from: join(paths.dsh, 'node_modules'), to: 'dsh/node_modules', filter: ['**/*'] },
  ],
  asarUnpack: ['**/*.{node,dll,exe}', '**/*.so.*', '**/spawn-helper', '**/@vscode/ripgrep-*/bin/rg',
    '**/node_modules/@deepseek-ai/libreoffice-kit-win32-x64/**/*'],
  extraResources: [
    { from: paths.runtime, to: 'runtime' },
    { from: fileURLToPath(new URL('./resources/delta-brand/delta-logo-512x512.png', import.meta.url)), to: 'icon.png' },
    { from: fileURLToPath(new URL('./resources/delta-brand/delta.ico', import.meta.url)), to: 'tray.ico' },
    { from: fileURLToPath(new URL('./resources/delta-brand/delta-logo-512x512.png', import.meta.url)), to: 'delta-brand/delta-logo-512x512.png' },
  ],
  win: {
    icon: fileURLToPath(new URL('./resources/delta-brand/delta.ico', import.meta.url)),
    target: ['dir'],
  },
  publish: null,
  beforePack: async context => {
    const office = await officePackageDirectories(paths.dsh, { platform: 'win32', arch: 'x64' })
    const patterns = office.map(directory => `**/${directory.slice(paths.dsh.length + 1).replaceAll('\\', '/')}/**/*`)
    context.packager.config.asarUnpack = [...context.packager.config.asarUnpack, ...patterns]
    windowsCode = await prepareWindowsAsarUnpack(context, paths.dsh)
  },
  afterPack: async context => {
    const resourcesDir = context.packager.getResourcesDir(context.appOutDir)
    if (existsSync(join(resourcesDir, 'app-update.yml'))) throw new Error('Delta portable must not embed an automatic update feed')
    await verifyDesktopRuntime(paths.dsh, dshProduct.version, { platform: 'win32', arch: 'x64' })
    await verifyWindowsAsarUnpack(paths.dsh, context.packager.getResourcesDir(context.appOutDir), windowsCode)
  },
}
