/** Remove upstream Python test suites from the staged primary runtime. */

import { lstatSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { basename, dirname, join, relative, sep } from 'node:path'
import Papa from 'papaparse'
import lock from './lock.json' with { type: 'json' }

function removeTestDirectories(root: string, directory: string): string[] {
  const removed: string[] = []
  for (const entry of readdirSync(join(root, directory), { withFileTypes: true })) {
    if (!entry.isDirectory()) continue
    const path = `${directory}/${entry.name}`
    if (entry.name === 'tests') {
      rmSync(join(root, path), { recursive: true })
      removed.push(path)
    } else {
      removed.push(...removeTestDirectories(root, path))
    }
  }
  return removed
}

/**
 * Prune NumPy/pandas tests and their RECORD rows, preserving testing helpers and native extensions.
 * @param sitePackages - Staged site-packages directory containing the locked wheels.
 * @throws If a locked distribution's RECORD is missing or malformed.
 */
export function prunePrimaryRuntimePythonTests(sitePackages: string): void {
  for (const name of ['numpy', 'pandas'] as const) {
    const record = join(sitePackages, `${name}-${lock.pythonPackages[name]}.dist-info`, 'RECORD')
    const { data, errors } = Papa.parse<string[]>(readFileSync(record, 'utf8'), { delimiter: ',', skipEmptyLines: true })
    if (errors.length > 0 || data.length === 0
      || !data.every((row): row is [string, string, string] => row.length === 3 && Boolean(row[0]))) {
      throw new Error(`primary runtime: invalid wheel RECORD: ${record}`)
    }
    const removed = removeTestDirectories(sitePackages, name)
    if (removed.length === 0) continue
    const retained = data.filter(([path]) => !removed.some(directory => path.startsWith(`${directory}/`)))
    writeFileSync(record, `${Papa.unparse(retained, { newline: '\n' })}\n`)
  }
}

/**
 * Remove redundant Python bytecode from a staged Windows runtime, never from user data.
 * Only delete regular files with a corresponding regular .py source; preserve sourceless
 * modules, native extensions, licenses, and all other wheel content.
 * @param library - Staged Python Lib directory.
 * @param sitePackages - Staged wheel installation directory within Lib.
 */
export function pruneSourceBackedPythonBytecode(library: string, sitePackages: string): void {
  const removed: string[] = []
  const isSource = (path: string): boolean => {
    try { return lstatSync(path).isFile() } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') return false
      throw error
    }
  }
  const visit = (directory: string): void => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name)
      if (entry.isDirectory()) { visit(path); continue }
      if (!entry.isFile() || !entry.name.endsWith('.pyc')) continue
      const match = basename(directory) === '__pycache__'
        ? /^(.*)\.cpython-\d+(?:\.opt-\d+)?\.pyc$/u.exec(entry.name)
        : undefined
      if (basename(directory) === '__pycache__' && match === undefined) continue
      const source = match === undefined
        ? path.slice(0, -1)
        : join(dirname(directory), match[1] + '.py')
      if (isSource(source)) removed.push(path)
    }
  }
  visit(library)
  if (removed.length === 0) return
  const removedRows = new Set(removed.map(path => relative(sitePackages, path).split(sep).join('/')))
  const records: { path: string; content: string }[] = []
  for (const entry of readdirSync(sitePackages, { withFileTypes: true })) {
    if (!entry.isDirectory() || !entry.name.endsWith('.dist-info')) continue
    const record = join(sitePackages, entry.name, 'RECORD')
    if (!isSource(record)) continue
    const { data, errors } = Papa.parse<string[]>(readFileSync(record, 'utf8'), { delimiter: ',', skipEmptyLines: true })
    if (errors.length > 0 || data.length === 0 || !data.every(row => row.length === 3 && Boolean(row[0]))) {
      throw new Error('primary runtime: malformed wheel RECORD during bytecode cleanup')
    }
    const retained = data.filter(([path]) => !removedRows.has(path!))
    if (retained.length !== data.length) records.push({ path: record, content: Papa.unparse(retained, { newline: '\n' }) + '\n' })
  }
  for (const path of removed) rmSync(path)
  for (const record of records) writeFileSync(record.path, record.content)
  process.stdout.write('primary runtime: removed ' + String(removed.length) + ' redundant source-backed Python bytecode files\n')
}
