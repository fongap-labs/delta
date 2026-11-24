/** Select and copy the local npm tarball closures that supply Desktop dsh and its private Host. */

import { createHash } from 'node:crypto'
import { spawnSync } from 'node:child_process'
import {
  constants,
  copyFileSync,
  globSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { basename, dirname, join, resolve } from 'node:path'
import { parseArgs } from 'node:util'
import * as yaml from 'js-yaml'
import {
  DESKTOP_HOST_PACKAGE,
  DESKTOP_HOST_RUNTIME_FILES,
  DESKTOP_PACKAGES_DIR,
  DESKTOP_PACKAGE_SET_FILE,
  parseDesktopCorePackageSet,
  type DesktopCorePackageRecord,
} from '../src/core-package-set.ts'
import { capture } from '../../../scripts/release/process.ts'
import { pnpmInvocation } from '../../../scripts/pnpm-invocation.ts'
import { tarballFiles } from '../../../scripts/release/tarball.ts'
import { desktopTargetPlatform, resolveDesktopBuildTarget, resolveDesktopTargetBuildPaths } from './desktop-build-paths.mjs'

const DSH_PACKAGE = '@deepseek-ai/dsh'
const ROOT_PACKAGES = [DSH_PACKAGE, DESKTOP_HOST_PACKAGE] as const
const APP_ROOT = resolve(import.meta.dirname, '..')
const REPOSITORY_ROOT = resolve(APP_ROOT, '..', '..')

const REQUIRED_DEPENDENCY_SECTIONS = ['dependencies', 'peerDependencies'] as const
const OPTIONAL_DEPENDENCY_SECTION = 'optionalDependencies'

/** Packed package information needed to form the local Desktop closure. */
export interface PackedDesktopPackage {
  readonly tarball: string
  readonly manifest: Readonly<Record<string, unknown>>
}

function supportsTarget(manifest: Readonly<Record<string, unknown>>, field: 'os' | 'cpu', target: string): boolean {
  const values = manifest[field]
  if (values === undefined) return true
  if (!Array.isArray(values) || !values.every((value): value is string => typeof value === 'string')) {
    throw new Error(`desktop package set: ${String(manifest.name)} has invalid ${field}`)
  }
  if (values.includes(`!${target}`)) return false
  return values.length === 0 || (values.length === 1 && values[0] === 'any') || values.includes(target)
    || values.every(value => value.startsWith('!'))
}

function dependencyNames(manifest: Readonly<Record<string, unknown>>, section: string): string[] {
  const value = manifest[section]
  if (value === undefined) return []
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`desktop package set: ${String(manifest.name)} has invalid ${section}`)
  }
  return Object.keys(value).sort()
}

interface WorkspaceDesktopPackage {
  readonly directory: string
  readonly manifest: Readonly<Record<string, unknown>>
}

function readWorkspacePackages(): Map<string, WorkspaceDesktopPackage> {
  const workspace = yaml.load(readFileSync(join(REPOSITORY_ROOT, 'pnpm-workspace.yaml'), 'utf8')) as { packages: string[] }
  const result = new Map<string, WorkspaceDesktopPackage>()
  for (const path of globSync(workspace.packages.map(pattern => `${pattern}/package.json`), { cwd: REPOSITORY_ROOT })) {
    const manifest = JSON.parse(readFileSync(join(REPOSITORY_ROOT, path), 'utf8')) as Record<string, unknown>
    const name = manifest.name
    if (typeof name !== 'string' || name === '' || result.has(name)) {
      throw new Error(`desktop package set: invalid or duplicate workspace package at ${path}`)
    }
    result.set(name, { directory: dirname(resolve(REPOSITORY_ROOT, path)), manifest })
  }
  return result
}

type ManifestPackage = { readonly manifest: Readonly<Record<string, unknown>> }

function targetCompatible(entry: ManifestPackage, target: ReturnType<typeof desktopTargetPlatform>): boolean {
  return supportsTarget(entry.manifest, 'os', target.platform)
    && supportsTarget(entry.manifest, 'cpu', target.arch)
}

/** Select the target-specific local runtime closure, including private workspace packages. */
export function selectDesktopPackageClosure<T extends ManifestPackage>(
  available: ReadonlyMap<string, T>,
  target: ReturnType<typeof desktopTargetPlatform>,
  workspace: ReadonlyMap<string, ManifestPackage> = readWorkspacePackages(),
): T[] {
  const selected = new Map<string, T>()
  const missing = new Set<string>()
  const visit = (name: string): void => {
    if (selected.has(name)) return
    const packed = available.get(name)
    if (packed === undefined) throw new Error(`desktop package set: packed inputs omit ${name}`)
    selected.set(name, packed)
    for (const section of REQUIRED_DEPENDENCY_SECTIONS) {
      for (const dependency of dependencyNames(packed.manifest, section)) {
        if (available.has(dependency)) visit(dependency)
        else if (workspace.has(dependency)) missing.add(`${name} requires unpacked package ${dependency}`)
      }
    }
    for (const dependency of dependencyNames(packed.manifest, OPTIONAL_DEPENDENCY_SECTION)) {
      const optional = available.get(dependency)
      if (optional !== undefined && targetCompatible(optional, target)) visit(dependency)
      else if (optional === undefined) {
        const source = workspace.get(dependency)
        if (source !== undefined && targetCompatible(source, target)) {
          missing.add(`${name} requires unpacked package ${dependency} for ${target.platform}/${target.arch}`)
        }
      }
    }
  }
  for (const name of ROOT_PACKAGES) visit(name)
  if (missing.size > 0) throw new Error(`desktop package set: ${[...missing].sort().join('; ')}`)
  return [...selected.entries()].sort(([left], [right]) => left.localeCompare(right)).map(([, packed]) => packed)
}

/** Plan missing local tarballs from the complete workspace graph, not a plugin allowlist. */
export function planMissingDesktopPacks(
  available: ReadonlyMap<string, PackedDesktopPackage>,
  target: ReturnType<typeof desktopTargetPlatform>,
  workspace: ReadonlyMap<string, WorkspaceDesktopPackage> = readWorkspacePackages(),
): WorkspaceDesktopPackage[] {
  const candidates = new Map<string, ManifestPackage>(workspace)
  for (const [name, packed] of available) candidates.set(name, packed)
  return selectDesktopPackageClosure(candidates, target, workspace).flatMap(entry => {
    const name = String(entry.manifest.name)
    const source = workspace.get(name)
    return !available.has(name) && source !== undefined ? [source] : []
  })
}

/** Pack every missing local runtime package from this checkout, without publishing it. */
function packMissingDesktopPackages(
  inputs: readonly string[], destination: string, target: ReturnType<typeof desktopTargetPlatform>,
): void {
  const workspace = readWorkspacePackages()
  const plan = planMissingDesktopPacks(packedPackages(inputs), target, workspace)
  console.log(`desktop package set: local pack plan (${plan.length}): ${plan.map(entry => entry.manifest.name).join(', ')}`)
  for (const entry of plan) {
    const invocation = pnpmInvocation(['--dir', entry.directory, 'pack', '--pack-destination', destination])
    const result = spawnSync(invocation.command, invocation.args, { cwd: REPOSITORY_ROOT, stdio: 'inherit', windowsHide: true })
    if (result.error) throw result.error
    if (result.status !== 0) throw new Error(`desktop package set: pack failed for ${String(entry.manifest.name)} (${String(result.status)})`)
  }
  const actual = packedPackages(inputs)
  const selected = selectDesktopPackageClosure(actual, target, workspace)
  for (const source of plan) {
    const name = String(source.manifest.name)
    const entry = actual.get(name)
    if (entry === undefined || entry.manifest.version !== source.manifest.version) {
      throw new Error(`desktop package set: missing or mismatched packed identity for ${name}`)
    }
  }
  for (const entry of selected) {
    const name = String(entry.manifest.name)
    if (workspace.get(name)?.manifest.private !== true) continue
    const files = new Set(tarballFiles(entry.tarball))
    const main = entry.manifest.main
    if (typeof main === 'string' && !files.has(`package/${main.replace(/^\.\//u, '')}`)) {
      throw new Error(`desktop package set: ${name} omits runtime entry ${main}`)
    }
  }
}

function packedManifest(tarball: string): Record<string, unknown> {
  const value: unknown = JSON.parse(capture('tar', ['-xOzf', tarball, 'package/package.json']))
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`desktop package set: ${tarball} has no package manifest`)
  }
  return value as Record<string, unknown>
}

function packedPackages(inputs: readonly string[]): Map<string, PackedDesktopPackage> {
  const available = new Map<string, PackedDesktopPackage>()
  for (const input of inputs) {
    const tarballs = readdirSync(input).filter(file => file.endsWith('.tgz')).sort()
    if (tarballs.length === 0) throw new Error(`desktop package set: ${input} contains no tarballs`)
    for (const file of tarballs) {
      const tarball = join(input, file)
      const manifest = packedManifest(tarball)
      const name = manifest.name
      if (typeof name !== 'string' || name === '') throw new Error(`desktop package set: ${tarball} has no package name`)
      if (available.has(name)) throw new Error(`desktop package set: duplicate packed package ${name}`)
      available.set(name, { tarball, manifest })
    }
  }
  return available
}

/**
 * Require every private Host file used before the Desktop profile can pass its health check.
 * @param files - Tarball paths rooted at `package/`.
 * @returns Nothing.
 */
export function assertDesktopHostPackageFiles(files: readonly string[]): void {
  const available = new Set(files)
  const missing = DESKTOP_HOST_RUNTIME_FILES
    .map(file => `package/${file}`)
    .filter(file => !available.has(file))
  if (missing.length > 0) {
    throw new Error(`desktop package set: ${DESKTOP_HOST_PACKAGE} tarball omits required file(s): ${missing.join(', ')}`)
  }
}

/**
 * Prepare a package set from release tarball directories.
 * @param inputs - Release tarball directories.
 * @param output - Destination package-set directory.
 * @param target - Platform and architecture of the installer payload.
 */
export function prepareDesktopPackageSet(
  inputs: readonly string[], output: string, target: ReturnType<typeof desktopTargetPlatform>,
): void {
  const selected = selectDesktopPackageClosure(packedPackages(inputs), target)
  const host = selected.find(packed => packed.manifest.name === DESKTOP_HOST_PACKAGE)
  if (host === undefined) throw new Error(`desktop package set: selected closure omits ${DESKTOP_HOST_PACKAGE}`)
  assertDesktopHostPackageFiles(tarballFiles(host.tarball))
  rmSync(output, { recursive: true, force: true })
  const packageDir = join(output, DESKTOP_PACKAGES_DIR)
  mkdirSync(packageDir, { recursive: true })
  const records: DesktopCorePackageRecord[] = selected.map((packed) => {
    const name = packed.manifest.name
    const version = packed.manifest.version
    if (typeof name !== 'string' || typeof version !== 'string') {
      throw new Error(`desktop package set: ${packed.tarball} has no package identity`)
    }
    const file = basename(packed.tarball)
    const destination = join(packageDir, file)
    copyFileSync(packed.tarball, destination, constants.COPYFILE_EXCL)
    const body = readFileSync(destination)
    return {
      name,
      version,
      file,
      bytes: statSync(destination).size,
      integrity: `sha512-${createHash('sha512').update(body).digest('base64')}`,
    }
  })
  const packageSet = parseDesktopCorePackageSet({ schemaVersion: 1, packages: records })
  writeFileSync(join(output, DESKTOP_PACKAGE_SET_FILE), `${JSON.stringify(packageSet, undefined, 2)}\n`, { mode: 0o600 })
}

function main(): void {
  const buildPaths = resolveDesktopTargetBuildPaths()
  const defaultInputs = [
    buildPaths.packedDsh,
    buildPaths.packedVendor,
    buildPaths.packedLandlock,
  ]
  const { values } = parseArgs({
    options: { from: { type: 'string', multiple: true }, out: { type: 'string' }, 'pack-missing': { type: 'boolean' } },
    allowPositionals: false,
  })
  const inputs = (values.from ?? defaultInputs).map(path => resolve(REPOSITORY_ROOT, path))
  const output = values.out === undefined ? buildPaths.packageSet : resolve(REPOSITORY_ROOT, values.out)
  const target = desktopTargetPlatform(resolveDesktopBuildTarget())
  if (values['pack-missing']) {
    if (values.from !== undefined) throw new Error('desktop package set: --pack-missing requires default input directories')
    packMissingDesktopPackages(inputs, buildPaths.packedDsh, target)
  }
  prepareDesktopPackageSet(inputs, output, target)
  console.log(`desktop package set: prepared ${output}`)
}

if (import.meta.main) main()
