/** Scrub textual release metadata from the Electron application archive. */
import { createRequire } from 'node:module'
import { cpSync, existsSync, mkdtempSync, readdirSync, renameSync, rmSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { sanitizeRuntimeMetadata } from './sanitize-runtime-metadata.ts'

const repositoryRoot = resolve(import.meta.dirname, '..', '..')
const asarStore = join(repositoryRoot, 'node_modules', '.pnpm')
const asarPackage = readdirSync(asarStore).find(entry => entry.startsWith('@electron+asar@'))
if (asarPackage === undefined) throw new Error('electron asar sanitizer: @electron/asar is not installed')
const { createPackageWithOptions, extractAll, getRawHeader } = createRequire(import.meta.url)(
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
  const literalGlob = value => value.replace(/([\\*?\[\]{}(),!+@])/g, '\\$1')
  const patterns = unpacked.map(path => literalGlob(`${extracted.replaceAll('\\', '/')}/${path}`))
  await createPackageWithOptions(extracted, rebuilt, {
    unpack: patterns.length === 0 ? undefined : patterns.length === 1 ? patterns[0] : `{${patterns.join(',')}}`,
  })
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
