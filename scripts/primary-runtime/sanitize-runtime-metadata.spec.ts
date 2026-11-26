import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { expect, it } from 'vitest'
import { sanitizeRuntimeMetadata, sanitizeRuntimeText } from './sanitize-runtime-metadata.ts'

it('removes release metadata without rewriting native bytes', async () => {
  const root = await mkdtemp(join(tmpdir(), 'primary-runtime-sanitize-'))
  const email = `owner${String.fromCharCode(64)}example.com`
  const unixHome = [String.fromCharCode(47), 'home', 'runner', 'work'].join(String.fromCharCode(47))
  const windowsHome = ['C:', 'Users', 'builder', 'wheel'].join(String.fromCharCode(92))
  const awsLike = `AA${String.fromCharCode(75)}IA1234567890123456`
  try {
    await mkdir(join(root, 'PIL', '__pycache__'), { recursive: true })
    await mkdir(join(root, 'demo-1.0.0.dist-info'))
    await writeFile(join(root, 'metadata.txt'), `${email} ${unixHome}\n`)
    await writeFile(join(root, 'PIL', 'ImageFont.py'), `font = b"""${awsLike}"""\n`)
    await writeFile(join(root, 'PIL', '__pycache__', 'ImageFont.pyc'), 'bytecode')
    await writeFile(join(root, 'demo-1.0.0.dist-info', 'DELVEWHEEL'), windowsHome)
    const native = Buffer.from([0, 1, 2, 0x41, 0x4b, 0x49, 0x41, 0x31, 0x32, 0x33])
    await writeFile(join(root, 'native.node'), native)

    sanitizeRuntimeMetadata(root)

    expect(sanitizeRuntimeText(`${email} ${unixHome}`)).not.toContain(String.fromCharCode(64))
    expect(await readFile(join(root, 'metadata.txt'), 'utf8')).not.toContain(String.fromCharCode(64))
    expect(await readFile(join(root, 'PIL', 'ImageFont.py'), 'utf8')).toContain('A\\x4bIA')
    await expect(readFile(join(root, 'PIL', '__pycache__', 'ImageFont.pyc'))).rejects.toMatchObject({ code: 'ENOENT' })
    await expect(readFile(join(root, 'demo-1.0.0.dist-info', 'DELVEWHEEL'))).rejects.toMatchObject({ code: 'ENOENT' })
    expect(await readFile(join(root, 'native.node'))).toEqual(native)
  } finally {
    await rm(root, { recursive: true, force: true })
  }
})
