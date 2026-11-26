import { createHash } from 'node:crypto'
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { zipSync } from 'fflate'
import { expect, it } from 'vitest'
import lock from './lock.json' with { type: 'json' }
import { unpackPrimaryRuntimeWheel } from './prepare.ts'
import { prunePrimaryRuntimePythonTests, pruneSourceBackedPythonBytecode } from './prune-python-tests.ts'

const records = {
  numpy: `numpy-${lock.pythonPackages.numpy}.dist-info/RECORD`,
  pandas: `pandas-${lock.pythonPackages.pandas}.dist-info/RECORD`,
}

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

it('removes source-backed bytecode and matching wheel records but keeps sourceless modules and native binaries', async () => {
  const root = await mkdtemp(join(tmpdir(), 'primary-python-bytecode-'))
  try {
    const site = join(root, 'site-packages')
    const removed = ['standard.pyc', 'site-packages/pkg/__pycache__/module.cpython-312.pyc',
      'site-packages/pkg/__pycache__/module.cpython-312.opt-1.pyc']
    const retained = ['standard.py', 'site-packages/pkg/module.py',
      'site-packages/pkg/__pycache__/only.cpython-312.pyc', 'site-packages/pkg/native.pyd',
      'site-packages/pkg.dist-info/LICENSE']
    for (const path of [...removed, ...retained]) {
      const full = join(root, path)
      await mkdir(join(full, '..'), { recursive: true })
      await writeFile(full, 'fixture')
    }
    const record = join(site, 'pkg.dist-info', 'RECORD')
    const recorded = [...removed, ...retained].filter(path => path.startsWith('site-packages/'))
      .map(path => path.slice('site-packages/'.length) + ',,')
    await writeFile(record, recorded.join('\n') + '\n')
    pruneSourceBackedPythonBytecode(root, site)
    for (const path of removed) await expect(readFile(join(root, path))).rejects.toMatchObject({ code: 'ENOENT' })
    for (const path of retained) expect(await readFile(join(root, path), 'utf8')).toBe('fixture')
    const after = await readFile(record, 'utf8')
    expect(after).not.toContain('module.cpython-312')
    expect(after).toContain('only.cpython-312.pyc')
    expect(after).toContain('pkg.dist-info/LICENSE')
    pruneSourceBackedPythonBytecode(root, site)
    expect(await readFile(record, 'utf8')).toBe(after)
  } finally {
    await rm(root, { recursive: true, force: true })
  }
})
