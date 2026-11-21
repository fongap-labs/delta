/**
 * Read-only Delta Code profile migration preflight.
 * Does not load a plugin, evaluate YAML !!js values, log credentials or write files.
 */
import { readFile, stat } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const IDENTITIES = {
  legacy: { bundle: '@fongap-labs/dsh-delta-code', entry: 'dsh-delta-code' },
  current: { bundle: '@fongap-labs/delta-code', entry: 'delta-code' },
}
const MAX_INPUT_BYTES = 2 * 1024 * 1024

function record(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function indicator() {
  return { dependency: false, selected: false, inserted: 0, overrides: 0 }
}

/**
 * Audit only the profile's dependency manifest, selected bundles and own patch.
 * Other overlays, installed package metadata, lockfiles and live runtime are NOT inspected.
 * The returned object contains only fixed booleans and counts, never user configuration.
 */
export function auditDeltaCodeProfile(manifest, patches = []) {
  if (!record(manifest) || !Array.isArray(patches)) throw new TypeError('invalid profile')
  const dependencies = ['dependencies', 'devDependencies', 'optionalDependencies']
  const selected = manifest.dsh?.profile?.bundles ?? []
  if (!Array.isArray(selected) || !selected.every(name => typeof name === 'string')) {
    throw new TypeError('invalid bundle selection')
  }
  const result = {
    version: 1,
    scope: ['package.json', 'cordis.patch.yml'],
    status: 'absent',
    legacy: indicator(),
    current: indicator(),
    requiresExternalOverlayReview: true,
  }
  for (const field of dependencies) {
    const value = manifest[field]
    if (value !== undefined && !record(value)) throw new TypeError('invalid dependencies')
    for (const [key, identity] of Object.entries(IDENTITIES)) {
      if (Object.hasOwn(value ?? {}, identity.bundle)) result[key].dependency = true
    }
  }
  for (const [key, identity] of Object.entries(IDENTITIES)) {
    result[key].selected = selected.includes(identity.bundle)
  }
  const mark = (row, field) => {
    if (!record(row)) throw new TypeError('invalid patch entry')
    for (const [key, identity] of Object.entries(IDENTITIES)) {
      if (row.id === identity.entry || row.name === identity.bundle) result[key][field]++
    }
  }
  const seen = new WeakSet()
  const walkInserted = (rows, depth) => {
    if (!Array.isArray(rows) || depth > 32 || seen.has(rows)) throw new TypeError('invalid inserted patch')
    seen.add(rows)
    for (const row of rows) {
      mark(row, 'inserted')
      if (row.group === true && Array.isArray(row.config)) walkInserted(row.config, depth + 1)
    }
  }
  for (const patch of patches) {
    if (!record(patch)) throw new TypeError('invalid patch')
    if (Object.hasOwn(patch, 'insert')) walkInserted(patch.insert, 0)
    else if (Object.hasOwn(patch, 'id')) mark(patch, 'overrides')
  }
  const present = key => Object.values(result[key]).some(value => value === true || typeof value === 'number' && value > 0)
  const legacy = present('legacy')
  const current = present('current')
  result.status = legacy && current ? 'mixed' : legacy ? 'legacy-only' : current ? 'current-only' : 'absent'
  return result
}

async function readBounded(path, optional = false) {
  try {
    const facts = await stat(path)
    if (!facts.isFile() || facts.size > MAX_INPUT_BYTES) throw new Error('invalid input size')
    return await readFile(path, 'utf8')
  } catch (error) {
    if (optional && error?.code === 'ENOENT') return null
    throw error
  }
}

async function main(args) {
  if (args.length !== 1 || args[0].startsWith('-')) {
    process.stdout.write(JSON.stringify({ status: 'invalid', reason: 'expected-one-profile-directory' }) + '\n')
    process.exitCode = 2
    return
  }
  const directory = resolve(args[0])
  let manifest
  try {
    manifest = JSON.parse(await readBounded(join(directory, 'package.json')))
  } catch {
    process.stdout.write(JSON.stringify({ status: 'invalid', reason: 'unreadable-profile-manifest' }) + '\n')
    process.exitCode = 2
    return
  }
  let patches = []
  try {
    const patchText = await readBounded(join(directory, 'cordis.patch.yml'), true)
    if (patchText !== null) {
      // The custom tag is parsed as an inert string; it is NEVER executed.
      const { default: yaml } = await import('js-yaml')
      const javascriptTag = new yaml.Type('tag:yaml.org,2002:js', {
        kind: 'scalar', construct: source => source ?? '',
      })
      patches = yaml.load(patchText, { schema: yaml.DEFAULT_SCHEMA.extend([javascriptTag]) })
      if (!Array.isArray(patches)) throw new TypeError('invalid patch document')
    }
  } catch {
    process.stdout.write(JSON.stringify({ status: 'invalid', reason: 'unreadable-profile-patch' }) + '\n')
    process.exitCode = 2
    return
  }
  try {
    const result = auditDeltaCodeProfile(manifest, patches)
    process.stdout.write(JSON.stringify(result) + '\n')
    process.exitCode = result.status === 'mixed' ? 2 : result.status === 'legacy-only' ? 1 : 0
  } catch {
    process.stdout.write(JSON.stringify({ status: 'invalid', reason: 'invalid-profile-structure' }) + '\n')
    process.exitCode = 2
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main(process.argv.slice(2))
}
