import { createHash } from 'node:crypto'
import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { zipSync } from 'fflate'
import { expect, it } from 'vitest'
import lock from './lock.json' with { type: 'json' }
import { unpackPrimaryRuntimeWheel } from './prepare.ts'
import { pruneUnreferencedNpmTests } from './prune-npm-tests.ts'
import { prunePrimaryRuntimePythonTests } from './prune-python-tests.ts'
import { sanitizeRuntimeMetadata, sanitizeRuntimeText } from './sanitize-runtime-metadata.ts'

const records = {
  numpy: `numpy-${lock.pythonPackages.numpy}.dist-info/RECORD`,
  pandas: `pandas-${lock.pythonPackages.pandas}.dist-info/RECORD`,
}

it('preserves native ASAR placement beyond the glob pattern size limit', async () => {
  const root = await mkdtemp(join(tmpdir(), 'asar-large-native-tree-'))
  try {
    const store = resolve('node_modules/.pnpm')
    const installed = (await readdir(store)).find(name => name.startsWith('@electron+asar@'))!
    const asar = createRequire(import.meta.url)(join(store, installed, 'node_modules/@electron/asar'))
    const source = join(root, 'source')
    const relative = `node_modules/scoped-native-package/${'nested-resource-directory-'.repeat(3)}`
    const directory = join(source, relative)
    await mkdir(directory, { recursive: true })
    const native = Buffer.from([0, 1, 2, 255])
    for (let index = 0; index < 800; index++) await writeFile(join(directory, `${index}.node`), native)
    const email = `owner${String.fromCharCode(64)}example.invalid`
    await writeFile(join(source, 'LICENSE'), email)
    await writeFile(join(source, 'main.js'), 'module.exports = 42;')
    const demo = join(source, 'node_modules/demo')
    await mkdir(join(demo, 'test'), { recursive: true })
    await writeFile(join(demo, 'package.json'), JSON.stringify({ name: 'demo', main: 'index.js' }))
    await writeFile(join(demo, 'index.js'), 'module.exports = 7;')
    await writeFile(join(demo, 'test', 'probe.node'), native)
    await writeFile(join(demo, 'test', 'probe.js'), "require('../index.js')")
    const archive = join(root, 'app.asar')
    await asar.createPackageWithOptions(source, archive, { unpack: '**/*.node' })
    const result = spawnSync(process.execPath, ['--import', 'tsx',
      'scripts/primary-runtime/sanitize-electron-asar.mjs', archive], { encoding: 'utf8', timeout: 60000 })
    expect(result.status, result.stderr).toBe(0)
    asar.uncacheAll()
    for (let index = 0; index < 800; index++) {
      const file = `${relative}/${index}.node`
      expect(asar.statFile(archive, file).unpacked).toBe(true)
      expect(await readFile(join(`${archive}.unpacked`, file))).toEqual(native)
    }
    expect(asar.extractFile(archive, 'LICENSE').toString()).not.toContain(email)
    expect(asar.extractFile(archive, 'main.js').toString()).toBe('module.exports = 42;')
    expect(asar.extractFile(archive, 'node_modules/demo/index.js').toString()).toBe('module.exports = 7;')
    expect(() => asar.statFile(archive, 'node_modules/demo/test/probe.js')).toThrow()
    expect(() => asar.statFile(archive, 'node_modules/demo/test/probe.node')).toThrow()
    await expect(stat(join(`${archive}.unpacked`, 'node_modules/demo/test'))).rejects.toMatchObject({ code: 'ENOENT' })
  } finally {
    await rm(root, { recursive: true, force: true })
  }
}, 60000)

it('removes only NumPy/pandas test directories and keeps retained wheel files and RECORD hashes', async () => {
  const root = await mkdtemp(join(tmpdir(), 'primary-python-pruning-'))
  try {
    const removed = [
      'numpy/tests/test_numpy.py', 'numpy/_core/tests/test_core.py', 'numpy/_core/tests/data/values,small.csv',
      'numpy/testing/tests/test_utils.py', 'pandas/tests/io/test_excel.py', 'pandas/tests/io/data/workbook.xlsx',
    ]
    const retained = [
      'numpy/__init__.py', 'numpy/_core/_multiarray_tests.so', 'numpy/_core/_multiarray_tests.pyd',
      'numpy/testing/__init__.py', 'numpy/_core/tests.py', 'numpy/data/values,small.csv',
      'pandas/__init__.py', 'pandas/testing.py', 'pandas/_testing/__init__.py', 'pandas/_libs/testing.pyd',
      `numpy-${lock.pythonPackages.numpy}.dist-info/licenses/LICENSE.txt`,
      `pandas-${lock.pythonPackages.pandas}.dist-info/LICENSE`,
      'other/tests/test_other.py', 'numpy_extra/tests/test_extra.py',
    ]
    const files = Object.fromEntries([...removed, ...retained].map(path => [path, Buffer.from(`contents of ${path}\n`)]))
    // Quoted paths exercise RECORD's CSV encoding even when a filename contains a comma.
    const row = (path: string): string => `"${path}",sha256=${createHash('sha256').update(files[path]!).digest('base64url')},${files[path]!.length}\n`
    for (const [name, record] of Object.entries(records)) {
      const paths = [...removed, ...retained].filter(path => path.startsWith(`${name}/`) || path.startsWith(`${name}-`))
      files[record] = Buffer.from(`${paths.map(row).join('')}${record},,\n`)
    }
    const archive = join(root, 'libraries.whl')
    const sitePackages = join(root, 'site-packages')
    const wheel = Buffer.from(zipSync(files))
    await writeFile(archive, wheel)
    await unpackPrimaryRuntimeWheel(archive, sitePackages)
    await mkdir(join(sitePackages, 'numpy/empty/tests'), { recursive: true })

    prunePrimaryRuntimePythonTests(sitePackages)

    for (const path of [...removed, 'numpy/tests', 'numpy/empty/tests', 'pandas/tests']) {
      await expect(readFile(join(sitePackages, path))).rejects.toMatchObject({ code: 'ENOENT' })
    }
    for (const path of retained) expect(await readFile(join(sitePackages, path))).toEqual(files[path])
    for (const [name, record] of Object.entries(records)) {
      const lines = (await readFile(join(sitePackages, record), 'utf8')).trimEnd().split('\n')
      const expected = retained.filter(path => path.startsWith(`${name}/`) || path.startsWith(`${name}-`))
      expect(lines).toHaveLength(expected.length + 1)
      for (const path of expected) {
        const expectedRow = row(path).trimEnd()
        expect(lines).toContain(path.includes(',') ? expectedRow : expectedRow.replace(`"${path}"`, path))
      }
      expect(lines).toContain(`${record},,`)
    }
    expect(await readFile(archive)).toEqual(wheel)
    const before = await Promise.all(Object.values(records).map(record => readFile(join(sitePackages, record))))
    prunePrimaryRuntimePythonTests(sitePackages)
    expect(await Promise.all(Object.values(records).map(record => readFile(join(sitePackages, record))))).toEqual(before)
  } finally {
    await rm(root, { recursive: true, force: true })
  }
})

it.each(['"unterminated,,\n', 'numpy/tests/test_numpy.py,\n', '', ',,\n'])(
  'rejects malformed installed-file records before deleting that distribution\'s tests: %j', async (record) => {
    const root = await mkdtemp(join(tmpdir(), 'primary-python-record-'))
    try {
      await mkdir(join(root, `numpy-${lock.pythonPackages.numpy}.dist-info`))
      await writeFile(join(root, records.numpy), record)
      await mkdir(join(root, 'numpy/tests'), { recursive: true })
      const testFile = join(root, 'numpy/tests/test_numpy.py')
      await writeFile(testFile, 'test fixture\n')
      expect(() => { prunePrimaryRuntimePythonTests(root) }).toThrow('invalid wheel RECORD')
      expect(await readFile(testFile, 'utf8')).toBe('test fixture\n')
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  },
)

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
    await writeFile(join(root, 'metadata.map'), `${email} ${unixHome}\n`)
    const legacy = Buffer.concat([Buffer.from([0xe9]), Buffer.from(` ${email}\n`)])
    await writeFile(join(root, 'legacy.txt'), legacy)
    const font = Buffer.concat([Buffer.from([0]), Buffer.from(email), Buffer.from([0])])
    await writeFile(join(root, 'font.ttf'), font)
    await writeFile(join(root, 'LICENSE'), `\uFFFD ${email}\n`)
    const marker = ['-----BEGIN ', 'PRIVATE KEY-----'].join('')
    await writeFile(join(root, 'functional.js'), `const marker = ${JSON.stringify(marker)}; const contact = ${JSON.stringify(email)};`)
    await writeFile(join(root, 'PIL', 'ImageFont.py'), `font = b"""${awsLike}"""\n`)
    await writeFile(join(root, 'PIL', '__pycache__', 'ImageFont.pyc'), 'bytecode')
    await writeFile(join(root, 'demo-1.0.0.dist-info', 'DELVEWHEEL'), windowsHome)
    const native = Buffer.from([0, 1, 2, 0x41, 0x4b, 0x49, 0x41, 0x31, 0x32, 0x33])
    await writeFile(join(root, 'native.node'), native)

    const audit = sanitizeRuntimeMetadata(root)

    expect(sanitizeRuntimeText(`${email} ${unixHome}`)).not.toContain(String.fromCharCode(64))
    expect(await readFile(join(root, 'metadata.txt'), 'utf8')).not.toContain(String.fromCharCode(64))
    expect(await readFile(join(root, 'metadata.map'), 'utf8')).not.toContain(String.fromCharCode(64))
    const legacyResult = await readFile(join(root, 'legacy.txt'))
    expect(legacyResult[0]).toBe(0xe9)
    expect(legacyResult.toString('latin1')).not.toContain(email)
    expect(await readFile(join(root, 'font.ttf'))).toEqual(font)
    expect(audit.retained.font).toBe(1)
    expect(await readFile(join(root, 'LICENSE'), 'utf8')).not.toContain(String.fromCharCode(64))
    const functional = await readFile(join(root, 'functional.js'), 'utf8')
    expect(functional).not.toContain(marker)
    expect(functional).not.toContain(email)
    expect(new Function(`${functional}; return [marker, contact]`)()).toEqual([marker, email])
    expect(await readFile(join(root, 'PIL', 'ImageFont.py'), 'utf8')).toContain('A\\x4bIA')
    await expect(readFile(join(root, 'PIL', '__pycache__', 'ImageFont.pyc'))).rejects.toMatchObject({ code: 'ENOENT' })
    await expect(readFile(join(root, 'demo-1.0.0.dist-info', 'DELVEWHEEL'))).rejects.toMatchObject({ code: 'ENOENT' })
    expect(await readFile(join(root, 'native.node'))).toEqual(native)
  } finally {
    await rm(root, { recursive: true, force: true })
  }
})

it('sanitizes UTF-8 text in any script whatever its extension and leaves NUL-bearing data untouched', async () => {
  const root = await mkdtemp(join(tmpdir(), 'primary-runtime-utf8-'))
  const email = `author${String.fromCharCode(64)}example.com`
  const words = '\u0936\u0942\u0928\u094d\u092f \u090f\u0915 \u0926\u094b\u0928'
  try {
    const text = `${words} ${email}\n`
    await writeFile(join(root, 'numbers.sor'), text)
    const binary = Buffer.concat([Buffer.from([0]), Buffer.from(` ${email} `), Buffer.from([0])])
    await writeFile(join(root, 'table.sor'), binary)

    const audit = sanitizeRuntimeMetadata(root)

    const sanitized = await readFile(join(root, 'numbers.sor'))
    expect(sanitized.toString('utf8')).not.toContain(String.fromCharCode(64))
    expect(sanitized.toString('utf8')).toContain(words)
    expect(sanitized.length).toBe(Buffer.byteLength(text))
    expect(await readFile(join(root, 'table.sor'))).toEqual(binary)
    expect(audit.files).toBe(1)
    expect(audit.retained['other-binary']).toBe(1)
  } finally {
    await rm(root, { recursive: true, force: true })
  }
})

async function writeTree(root: string, files: Record<string, string>): Promise<void> {
  for (const [path, contents] of Object.entries(files)) {
    await mkdir(dirname(join(root, path)), { recursive: true })
    await writeFile(join(root, path), contents)
  }
}

async function exists(path: string): Promise<boolean> {
  try {
    await stat(path)
    return true
  } catch {
    return false
  }
}

it('removes only test directories that runtime sources and entry points never reference', async () => {
  const root = await mkdtemp(join(tmpdir(), 'primary-npm-pruning-'))
  try {
    await writeTree(root, {
      'node_modules/plain/package.json': JSON.stringify({ name: 'plain', main: 'index.js' }),
      'node_modules/plain/index.js': 'module.exports = 1\n',
      'node_modules/plain/test/index.test.js': "require('../index.js')\n",
      'node_modules/plain/benchmarks/run.js': "require('../test/index.test.js')\n",
      'node_modules/plain/node_modules/nested/package.json': JSON.stringify({ name: 'nested' }),
      'node_modules/plain/node_modules/nested/index.js': 'module.exports = 2\n',
      'node_modules/plain/node_modules/nested/tests/a.js': 'void 0\n',
      'node_modules/@scope/pkg/package.json': JSON.stringify({ name: '@scope/pkg' }),
      'node_modules/@scope/pkg/index.js': 'module.exports = 3\n',
      'node_modules/@scope/pkg/__tests__/a.js': 'void 0\n',
      'node_modules/emoji/package.json': JSON.stringify({ name: 'emoji', main: 'lib/index.js' }),
      'node_modules/emoji/lib/index.js': "module.exports = require('./emoji/test/parse.js')\n",
      'node_modules/emoji/lib/emoji/test/parse.js': "module.exports = require('../../../fixtures/data.json')\n",
      'node_modules/emoji/fixtures/data.json': '{}\n',
      'node_modules/entry/package.json': JSON.stringify({ name: 'entry', exports: { './fixtures': './fixtures/index.js' } }),
      'node_modules/entry/fixtures/index.js': 'module.exports = 4\n',
      'node_modules/broken/package.json': '{',
      'node_modules/broken/test/a.js': 'void 0\n',
    })
    const expected = [
      'node_modules/@scope/pkg/__tests__',
      'node_modules/plain/benchmarks',
      'node_modules/plain/node_modules/nested/tests',
      'node_modules/plain/test',
    ]
    const kept = [
      'node_modules/emoji/lib/emoji/test/parse.js',
      'node_modules/emoji/fixtures/data.json',
      'node_modules/entry/fixtures/index.js',
      'node_modules/broken/test/a.js',
      'node_modules/plain/index.js',
      'node_modules/plain/node_modules/nested/index.js',
    ]

    expect(pruneUnreferencedNpmTests(root, { isDryRun: true })).toEqual(expected)
    for (const path of expected) expect(await exists(join(root, path))).toBe(true)

    expect(pruneUnreferencedNpmTests(root)).toEqual(expected)
    for (const path of expected) expect(await exists(join(root, path))).toBe(false)
    for (const path of kept) expect(await exists(join(root, path))).toBe(true)
    expect(pruneUnreferencedNpmTests(root)).toEqual([])
  } finally {
    await rm(root, { recursive: true, force: true })
  }
})
