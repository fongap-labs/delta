/** Scrub textual release metadata from the Electron application archive. */
import { createRequire } from 'node:module'
import { mkdtempSync, readdirSync, renameSync, rmSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { sanitizeRuntimeMetadata } from './sanitize-runtime-metadata.ts'

const repositoryRoot = resolve(import.meta.dirname, '..', '..')
const asarStore = join(repositoryRoot, 'node_modules', '.pnpm')
const asarPackage = readdirSync(asarStore).find(entry => entry.startsWith('@electron+asar@'))
if (asarPackage === undefined) throw new Error('electron asar sanitizer: @electron/asar is not installed')
const { createPackage, extractAll } = createRequire(import.meta.url)(
  join(asarStore, asarPackage, 'node_modules', '@electron', 'asar'),
)
const archiveArgument = process.argv[2]
if (!archiveArgument) throw new Error('electron asar sanitizer: archive path is required')
const archive = resolve(archiveArgument)

const temporaryRoot = mkdtempSync(join(dirname(archive), '.delta-asar-sanitize-'))
const extracted = join(temporaryRoot, 'app')
const rebuilt = join(temporaryRoot, 'app.asar')
try {
  extractAll(archive, extracted)
  sanitizeRuntimeMetadata(extracted)
  await createPackage(extracted, rebuilt)
  rmSync(archive)
  renameSync(rebuilt, archive)
} finally {
  rmSync(temporaryRoot, { recursive: true, force: true })
}
