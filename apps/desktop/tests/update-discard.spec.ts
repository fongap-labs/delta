import { mkdtemp, mkdir, readFile, writeFile } from 'node:fs/promises'
import * as fs from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, expect, it, vi } from 'vitest'
import type { AppUpdater } from 'electron-updater'
import { discardDesktopUpdate } from '../src/updates/electron-updater/discard.ts'

vi.mock('node:fs/promises', async (importOriginal) => {
  const actual = await importOriginal<typeof import('node:fs/promises')>()
  return { ...actual, rm: vi.fn(actual.rm) }
})

const environment = vi.hoisted(() => ({ logs: '' }))
vi.mock('electron', () => ({ app: { getPath: () => environment.logs } }))
const roots: string[] = []
afterEach(async () => {
  vi.restoreAllMocks()
  for (const root of roots.splice(0)) await fs.rm(root, { recursive: true, force: true })
})

it('retires the target despite a removal failure and emits only a non-identifying warning and does not persist logs', async () => {
  const root = await mkdtemp(join(tmpdir(), 'update-discard-'))
  roots.push(root)
  const pending = join(root, 'pending')
  environment.logs = join(root, 'logs')
  await mkdir(pending)
  const file = join(pending, 'old.zip')
  await writeFile(file, 'obsolete')
  const helper = { _file: file, _packageFile: file, versionInfo: {}, fileInfo: {},
    _downloadedFileInfo: {}, cacheDirForPendingUpdate: pending }
  const internal = { updateInfoAndProvider: {}, downloadedUpdateHelper: helper }
  const failure = Object.assign(new Error('file is locked'), { code: 'EPERM' })
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
  vi.mocked(fs.rm).mockRejectedValueOnce(failure)
  await discardDesktopUpdate(internal as object as AppUpdater)
  expect(internal.updateInfoAndProvider).toBeNull()
  expect(helper).toMatchObject({ _file: null, _packageFile: null, versionInfo: null, fileInfo: null, _downloadedFileInfo: null })
  expect(await readFile(file, 'utf8')).toBe('obsolete')
  expect(warn).toHaveBeenCalledExactlyOnceWith('desktop update: cache removal failed')
  await expect(readFile(join(environment.logs, 'update-cleanup.jsonl'), 'utf8')).rejects.toMatchObject({ code: 'ENOENT' })
})

it('does not retire a target that the native installer already owns', async () => {
  const internal = { squirrelDownloadedUpdate: true, updateInfoAndProvider: {} }
  await expect(discardDesktopUpdate(internal as object as AppUpdater)).rejects.toThrow('native installation already staged')
  expect(internal.updateInfoAndProvider).not.toBeNull()
})

