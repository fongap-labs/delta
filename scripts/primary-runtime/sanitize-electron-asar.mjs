/** Scrub textual release metadata from the Electron application archive. */
import { createRequire } from 'node:module'
import { cpSync, createReadStream, existsSync, lstatSync, mkdtempSync, readdirSync, readlinkSync, renameSync, rmSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { sanitizeRuntimeMetadata } from './sanitize-runtime-metadata.ts'

const repositoryRoot = resolve(import.meta.dirname, '..', '..')
const asarStore = join(repositoryRoot, 'node_modules', '.pnpm')
const asarPackage = readdirSync(asarStore).find(entry => entry.startsWith('@electron+asar@'))
if (asarPackage === undefined) throw new Error('electron asar sanitizer: @electron/asar is not installed')
const { createPackageFromStreams, extractAll, getRawHeader } = createRequire(import.meta.url)(
  join(asarStore, asarPackage, 'node_modules', '@electron', 'asar'),
)
const archiveArgument = process.argv[2]
if (!archiveArgument) throw new Error('electron asar sanitizer: archive path is required')
const archive = resolve(archiveArgument)

const temporaryRoot = mkdtempSync(join(dirname(archive), '.delta-asar-sanitize-'))
const extracted = join(temporaryRoot, 'app')
const rebuilt = join(temporaryRoot, 'app.asar')
try {
  // Preserve the builder's native module and resource placement exactly.
  const unpacked = []
  function collect(directory, prefix = '', inherited = false, result = unpacked) {
    for (const [name, entry] of Object.entries(directory.files)) {
      const path = prefix ? `${prefix}/${name}` : name
      const outside = inherited || entry.unpacked === true
      if (entry.files) collect(entry, path, outside, result)
      else if (outside) result.push(path)
    }
  }
  collect(getRawHeader(archive).header)
  extractAll(archive, extracted)
  sanitizeRuntimeMetadata(extracted)
  const outsidePaths = new Set(unpacked)
  const streams = []
  function stage(directory, prefix = '') {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = prefix ? `${prefix}/${entry.name}` : entry.name
      const full = join(directory, entry.name)
      const common = { path, unpacked: outsidePaths.has(path), stat: lstatSync(full) }
      if (entry.isDirectory()) {
        streams.push({ ...common, type: 'directory' })
        stage(full, path)
      } else if (entry.isSymbolicLink()) {
        streams.push({ ...common, type: 'link', symlink: readlinkSync(full) })
      } else {
        streams.push({ ...common, type: 'file', streamGenerator: () => createReadStream(full) })
      }
    }
  }
  stage(extracted)
  await createPackageFromStreams(rebuilt, streams)
  const rebuiltUnpacked = []
  collect(getRawHeader(rebuilt).header, '', false, rebuiltUnpacked)
  if (JSON.stringify(unpacked.sort()) !== JSON.stringify(rebuiltUnpacked.sort())) {
    throw new Error('electron asar sanitizer: native resource placement changed')
  }
  if (existsSync(`${rebuilt}.unpacked`)) cpSync(`${rebuilt}.unpacked`, `${archive}.unpacked`, { recursive: true })
  rmSync(archive)
  renameSync(rebuilt, archive)
} finally {
  rmSync(temporaryRoot, { recursive: true, force: true })
}
