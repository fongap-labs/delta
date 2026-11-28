import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

/** Resolve the installed native compiler's declared executable without using package subpath exports. */
export function nativeCompilerPath(): string {
  const directory = resolve(import.meta.dirname, '../node_modules/@typescript/native')
  const manifest: unknown = JSON.parse(readFileSync(resolve(directory, 'package.json'), 'utf8'))
  if (typeof manifest !== 'object' || manifest === null || !('bin' in manifest)) {
    throw new Error('Native TypeScript package does not declare a compiler executable')
  }
  const bin = manifest.bin
  const entry = typeof bin === 'string' ? bin
    : typeof bin === 'object' && bin !== null && 'tsc' in bin ? bin.tsc : undefined
  if (typeof entry !== 'string' || entry.length === 0) {
    throw new Error('Native TypeScript package does not declare the tsc executable')
  }
  return resolve(directory, entry)
}

if (import.meta.main) {
  const result = spawnSync(process.execPath, [...process.execArgv, nativeCompilerPath(), ...process.argv.slice(2)], {
    stdio: 'inherit',
  })
  if (result.error !== undefined) throw result.error
  process.exitCode = result.status ?? 1
}
