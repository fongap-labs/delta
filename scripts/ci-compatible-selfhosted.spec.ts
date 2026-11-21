import { spawnSync } from 'node:child_process'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { expect, it } from 'vitest'

// The original upstream runner matrix is gone. Keep the behavior check for
// the standalone Node preload rather than asserting missing workflow jobs.
it('isolates the Node setup toolcache to RUNNER_TEMP without changing other Node processes', () => {
  const temp = mkdtempSync(join(tmpdir(), 'delta-ci-toolcache-'))
  const cwd = resolve(import.meta.dirname, '..')
  const env = {
    ...process.env,
    NODE_OPTIONS: '',
    RUNNER_TEMP: temp,
    RUNNER_TOOL_CACHE: join(temp, 'persistent'),
  }
  const probe = (overrides: Record<string, string>) => {
    const result = spawnSync(process.execPath, ['-p', 'process.env.RUNNER_TOOL_CACHE'], {
      cwd, env: { ...env, ...overrides }, encoding: 'utf8', timeout: 10_000,
    })
    expect(result.error).toBeUndefined()
    expect(result.signal).toBeNull()
    return result
  }

  try {
    const setup = probe({ NODE_OPTIONS: '--import=./scripts/ci-compatible-toolcache.mjs' })
    expect(setup.status, setup.stderr).toBe(0)
    expect(setup.stdout.trim()).toBe(join(temp, 'node-compat-toolcache'))

    const normal = probe({})
    expect(normal.status, normal.stderr).toBe(0)
    expect(normal.stdout.trim()).toBe(env.RUNNER_TOOL_CACHE)

    for (const invalid of ['', './relative']) {
      const rejected = probe({
        NODE_OPTIONS: '--import=./scripts/ci-compatible-toolcache.mjs',
        RUNNER_TEMP: invalid,
      })
      expect(rejected.status).not.toBe(0)
      expect(rejected.stderr).toContain('requires an absolute RUNNER_TEMP')
    }
  } finally {
    rmSync(temp, { recursive: true, force: true })
  }
})
