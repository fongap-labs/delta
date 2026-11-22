/** Local, bounded desktop failure journal. Never uploads user data or raw exceptions. */
import { appendFile, mkdir, rename, rm, stat } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { join } from 'node:path'

const MAX_BYTES = 128 * 1024
const RETENTION_MS = 14 * 24 * 60 * 60 * 1000

export type DesktopFailureSource = 'host' | 'web-boot' | 'renderer' | 'main'
type StoredFailure = {
  readonly time: string
  readonly kind: 'fatal'
  readonly source: DesktopFailureSource
  readonly errorClass: string
  readonly errorCode?: string
  readonly stackFingerprint?: string
}

/** One private directory under Electron's already-isolated userData root. */
export class LocalDiagnosticJournal {
  private readonly directory: string
  private readonly path: string
  private pending: Promise<void> = Promise.resolve()

  constructor(userData: string) {
    this.directory = join(userData, 'diagnostics')
    this.path = join(this.directory, 'errors.jsonl')
  }

  /** Never store a raw message, stack, URL, credential or Session identifier. */
  recordFatal(source: DesktopFailureSource, failure: unknown): Promise<void> {
    const reason = failure instanceof Error ? failure : undefined
    const rawClass = reason?.name ?? 'UnknownError'
    const errorClass = /^[A-Za-z][A-Za-z0-9]{0,63}$/u.test(rawClass) ? rawClass : 'Error'
    const code = (reason as (Error & { code?: unknown }) | undefined)?.code
    const errorCode = typeof code === 'string' && /^[A-Z][A-Z0-9_]{1,39}$/u.test(code) ? code : undefined
    const fingerprint = reason?.stack === undefined ? undefined
      : createHash('sha256').update(reason.stack).digest('hex').slice(0, 16)
    const item: StoredFailure = {
      time: new Date().toISOString(), kind: 'fatal', source, errorClass,
      ...errorCode === undefined ? {} : { errorCode },
      ...fingerprint === undefined ? {} : { stackFingerprint: fingerprint },
    }
    // Suppress disk failure: reporting must not break crash recovery or leak paths.
    const write = this.pending.then(() => this.write(item))
    this.pending = write.catch(() => {})
    return this.pending
  }

  private async write(item: StoredFailure): Promise<void> {
    await mkdir(this.directory, { recursive: true, mode: 0o700 })
    const line = JSON.stringify(item) + '\n'
    const now = Date.now()
    // Rotated journals must expire independently of the active file.
    for (const suffix of ['.1', '.2']) {
      const archivedPath = this.path + suffix
      const archived = await stat(archivedPath).catch(() => undefined)
      if (archived !== undefined && now - archived.mtimeMs > RETENTION_MS) {
        await rm(archivedPath, { force: true })
      }
    }
    const existing = await stat(this.path).catch(() => undefined)
    if (existing !== undefined && now - existing.mtimeMs > RETENTION_MS) {
      await rm(this.path, { force: true })
    } else if (existing !== undefined && existing.size + Buffer.byteLength(line) > MAX_BYTES) {
      await rm(this.path + '.2', { force: true })
      await rename(this.path + '.1', this.path + '.2').catch(() => {})
      await rename(this.path, this.path + '.1')
    }
    await appendFile(this.path, line, { encoding: 'utf8', mode: 0o600 })
  }
}
