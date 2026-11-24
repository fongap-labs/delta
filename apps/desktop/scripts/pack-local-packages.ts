/** Stage local-branded DSH packages for unsigned Desktop builds without a publication record. */

import { copyFileSync, existsSync, mkdirSync, rmSync } from 'node:fs'
import { resolve } from 'node:path'
import { parseArgs } from 'node:util'
import {
  readClientBuildRecord,
  repositoryClientBuildEnvironment,
  resolveClientBuildEnvironment,
} from '../../../scripts/client-build-environment.ts'
import { releaseFamily } from '../../../scripts/release/families.ts'
import { packFamilyMembers, parsePackConcurrency } from '../../../scripts/release/pack.ts'

async function main(): Promise<void> {
  const { values } = parseArgs({
    options: { out: { type: 'string' }, concurrency: { type: 'string' } },
    allowPositionals: false,
  })
  if (values.out === undefined) throw new Error('desktop local pack: --out must name the package staging directory')
  const concurrency = parsePackConcurrency(values.concurrency)
  const root = process.cwd()
  const environment = resolveClientBuildEnvironment(repositoryClientBuildEnvironment(root), 'local')
  readClientBuildRecord(root, environment)
  const destination = resolve(root, values.out)
  // The CLI manifest includes docs/, which is generated and ignored in Git.
  // Stage only Delta's actual portable documentation, not the upstream release website.
  const docs = resolve(root, 'apps/cli/docs')
  const source = resolve(root, 'docs/delta/portable')
  if (existsSync(docs)) throw new Error('desktop local pack: remove stale generated apps/cli/docs before packing')
  mkdirSync(docs)
  try {
    mkdirSync(resolve(docs, 'upgrade-guide'))
    copyFileSync(resolve(source, 'README.md'), resolve(docs, 'README.md'))
    copyFileSync(resolve(source, 'UPGRADE.md'), resolve(docs, 'upgrade-guide/README.md'))
    const packed = await packFamilyMembers(root, releaseFamily('dsh'), destination, concurrency)
    console.log(`desktop local pack: ${String(packed.length)} tarball(s) in ${destination}`)
  } finally {
    rmSync(docs, { recursive: true, force: true })
  }
}

await main()
