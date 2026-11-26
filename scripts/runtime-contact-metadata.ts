/** Optional package contact metadata cleanup for staged runtime copies only. */
import { readFile, readdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

/** Remove contact addresses from optional author metadata, never legal notices or runtime fields. */
export function stripRuntimeContactEmails(manifest: Record<string, unknown>): boolean {
  const email = /[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/u
  const stripPerson = (person: unknown): unknown => {
    if (typeof person === 'string') {
      if (!email.test(person)) return person
      return person.replace(/<[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}>/gu, '')
        .replace(new RegExp(email.source, 'gu'), '').trim()
    }
    if (person === null || typeof person !== 'object' || Array.isArray(person)) return person
    const record = person as Record<string, unknown>
    if (typeof record.email !== 'string' || !email.test(record.email)) return person
    const { email: _email, ...retained } = record
    return retained
  }
  let hasChanges = false
  for (const field of ['author', 'contributors', 'maintainers']) {
    const original = manifest[field]
    const next = Array.isArray(original) ? original.map(stripPerson) : stripPerson(original)
    if (JSON.stringify(next) !== JSON.stringify(original)) {
      manifest[field] = next
      hasChanges = true
    }
  }
  return hasChanges
}

/** Clean npm manifests in a staged runtime without altering licenses or code. */
export async function stripStagedPackageContactEmails(root: string): Promise<void> {
  for (const entry of await readdir(root, { recursive: true, withFileTypes: true })) {
    if (!entry.isFile() || entry.name !== 'package.json') continue
    const path = join(entry.parentPath, entry.name)
    const source = await readFile(path, 'utf8')
    const manifest = JSON.parse(source) as Record<string, unknown>
    if (stripRuntimeContactEmails(manifest)) await writeFile(path, `${JSON.stringify(manifest, null, 2)}\n`)
  }
}
