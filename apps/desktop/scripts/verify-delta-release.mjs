/** Verify the actual portable archive on the Windows release runner before upload. */
import { createHash } from 'node:crypto'
import { mkdtempSync, readFileSync, readdirSync, rmSync, statSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, relative, resolve, sep } from 'node:path'
import { spawnSync } from 'node:child_process'
import { parseArgs } from 'node:util'

const { values } = parseArgs({ options: {
  target: { type: 'string' }, version: { type: 'string' }, 'output-dir': { type: 'string' },
} })
if (process.platform !== 'win32' || process.arch !== 'x64' || values.target !== 'x86_64-pc-windows-msvc'
  || !values.version || !values['output-dir']) throw new Error('Invalid Delta release acceptance request')
const archive = resolve(values['output-dir'], 'Delta-win-x64.zip')
const sevenZip = join(process.env.ProgramFiles, '7-Zip', '7z.exe')
const directory = mkdtempSync(join(tmpdir(), 'Delta portable acceptance '))
function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: 'inherit', timeout: 300000, windowsHide: true, ...options })
  if (result.error) throw result.error
  if (result.status !== 0) throw new Error(command + ' failed: ' + String(result.status))
}
function files(root, path = root) {
  return readdirSync(path, { withFileTypes: true }).flatMap(entry => {
    const full = join(path, entry.name)
    if (entry.isSymbolicLink()) throw new Error('Unexpected symbolic link in portable archive')
    return entry.isDirectory() ? files(root, full) : [relative(root, full).split(sep).join('/')]
  })
}
try {
  run(sevenZip, ['t', archive])
  run(sevenZip, ['x', archive, '-o' + directory, '-y'])
  if (readdirSync(directory).sort().join(',') !== 'App,Data,Other') throw new Error('Invalid portable directory layout')
  for (const path of ['App/Delta.exe', 'App/delta.cmd', 'App/resources/app.asar',
    'Other/VERSION.json', 'Other/LICENSE', 'Other/DEEPSEEK-HARNESS-MIT.txt', 'Other/THIRD_PARTY_NOTICES.md']) {
    if (!statSync(join(directory, path)).isFile() || statSync(join(directory, path)).size === 0) throw new Error('Missing release file: ' + path)
  }
  for (const path of ['UserData', 'Session', 'Logs', 'Temp', 'Home']) {
    if (!statSync(join(directory, 'Data', path)).isDirectory()) throw new Error('Missing portable data directory: ' + path)
  }
  if (files(join(directory, 'Data')).length !== 0) throw new Error('Portable release contains user data')
  const metadata = JSON.parse(readFileSync(join(directory, 'Other/VERSION.json'), 'utf8'))
  if (metadata.product !== 'Delta' || metadata.version !== values.version || metadata.platform !== 'win32-x64') {
    throw new Error('Portable metadata does not match the release request')
  }
  const expected = new Set(files(directory).filter(path => path.startsWith('App/') || (path.startsWith('Other/') && path !== 'Other/SHA256SUMS.txt')))
  for (const line of readFileSync(join(directory, 'Other/SHA256SUMS.txt'), 'utf8').trimEnd().split(/\r?\n/)) {
    const match = /^([a-f0-9]{64})  (.+)$/.exec(line)
    if (!match) throw new Error('Invalid SHA256SUMS entry')
    const path = match[2].replaceAll('\\', '/')
    if (!expected.delete(path)) throw new Error('Unexpected or duplicate checksum entry: ' + path)
    const actual = createHash('sha256').update(readFileSync(join(directory, path))).digest('hex')
    if (actual !== match[1]) throw new Error('Checksum mismatch: ' + path)
  }
  if (expected.size) throw new Error('Missing checksum entries')
  run('cmd.exe', ['/d', '/c', 'delta.cmd', '--help'], { cwd: join(directory, 'App'), timeout: 120000 })
  const desktopEnvironment = { ...process.env }
  delete desktopEnvironment.ELECTRON_RUN_AS_NODE
  delete desktopEnvironment.NODE_OPTIONS
  run(join(directory, 'App', 'Delta.exe'), ['--delta-portable-smoke'], {
    cwd: join(directory, 'App'), env: desktopEnvironment, timeout: 180000,
  })
  const startup = JSON.parse(readFileSync(join(directory, 'Data', 'Temp', 'delta-startup-verified.json'), 'utf8'))
  if (startup.version !== values.version || startup.workspaceReady !== true || startup.backendReady !== true) {
    throw new Error('Portable desktop startup did not verify the requested version and workspace')
  }
  console.log('Delta portable release archive, version, checksums, CLI and desktop workspace startup verified.')
} finally {
  rmSync(directory, { recursive: true, force: true })
}
