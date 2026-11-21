import { existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs'
import * as childProcess from 'node:child_process'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { DesktopHostFatalError, DesktopHostProcess, DesktopHostUncleanExitError, QUIT_INSPECTION_DEADLINE_MS } from '../src/host-process.ts'

vi.mock('node:child_process', async (importOriginal) => {
  const original = await importOriginal<typeof import('node:child_process')>()
  return { ...original, spawn: vi.fn(original.spawn) }
})

const roots: string[] = []
const hosts: DesktopHostProcess[] = []

const HTTP_HOST = `
import { createServer } from 'node:http'
import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
let authenticationReceived = false
let receivedSecret = ''
let listening = false
let published = false
const publishReady = () => {
  if (!authenticationReceived || !listening || published) return
  published = true
  process.send({ type: 'ready', url: 'http://127.0.0.1:' + server.address().port + '/' })
}
const server = createServer((request, response) => {
  if (request.url === '/fatal') {
    process.send({ type: 'fatal', message: 'plugin unavailable' })
    response.end('reported')
    return
  }
  if (request.url === '/crash') {
    response.end('exiting', () => {
      process.stderr.write('plugin crashed', () => process.exit(7))
    })
    return
  }
  response.setHeader('content-type', 'application/json')
  response.end(JSON.stringify({runtime: process.argv[2], profile: process.argv[3], cwd: process.cwd(), nodePath: process.env.NODE_PATH, registry: process.env.NPM_CONFIG_REGISTRY, nodeOptions: process.env.NODE_OPTIONS, runAsNode: process.env.ELECTRON_RUN_AS_NODE, internals: process.execArgv.includes('--expose-internals'), privateAuth: authenticationReceived && /^[A-Za-z0-9_-]{43}$/.test(receivedSecret) && !JSON.stringify(process.argv).includes(receivedSecret) && !JSON.stringify(process.env).includes(receivedSecret)}))
})
server.listen(0, '127.0.0.1', () => { listening = true; publishReady() })
process.on('message', message => {
  if (message.type === 'desktop-auth') {
    if (authenticationReceived || !/^[A-Za-z0-9_-]{43}$/.test(message.secret)) throw new Error('invalid authentication handoff')
    authenticationReceived = true
    receivedSecret = message.secret
    publishReady()
    return
  }
  if (message.type === 'file-drag-cancel') {
    process.send({ type: 'file-drag-resource', requestId: message.requestId, resource: null })
    return
  }
  if (message.type === 'file-drag-resource') {
    if (message.address === 'pending') return
    if (message.address === 'invalid') {
      process.send({ type: 'file-drag-resource', requestId: message.requestId, resource: { path: '/invalid', sha256: 'bad', size: -1 } })
      return
    }
    process.send({ type: 'file-drag-resource', requestId: message.requestId,
      resource: message.address === 'remote' ? null : { path: '/authorized/report.pdf', sha256: 'a'.repeat(64), size: 42 },
      ...(message.address === 'denied' ? { error: 'File read denied' } : {}) })
    return
  }
  if (message.type === 'update-tasks') {
    process.send({ type: 'update-tasks', requestId: message.requestId, active: message.action === 'lock' })
    return
  }
  if (message.type === 'quit-inspection') {
    // Ids divisible by three never answer; the others report scheduled work for odd ids.
    if (message.requestId % 3 === 0) return
    process.send({ type: 'quit-inspection', requestId: message.requestId, activeTasks: false, scheduledTasks: message.requestId % 2 === 1 })
    return
  }
  if (message.type !== 'shutdown') return
  server.close(() => {
    writeFileSync(join(process.argv[3], 'stopped'), '')
    process.send({ type: 'shutdown-complete' }, () => process.disconnect())
  })
  server.closeAllConnections()
})
process.send({ type: 'starting' })
`

function projectWithHost(source = HTTP_HOST): string {
  const project = mkdtempSync(join(tmpdir(), 'dsh-desktop-host-test-'))
  roots.push(project)
  const packageRoot = join(project, 'node_modules', '@deepseek-ai', 'dsh-desktop-host')
  mkdirSync(join(packageRoot, 'lib'), { recursive: true })
  writeFileSync(join(packageRoot, 'package.json'), '{"name":"@deepseek-ai/dsh-desktop-host","type":"module"}\n')
  writeFileSync(join(packageRoot, 'lib', 'index.js'), source)
  return project
}

function hostProcess(
  runtime: string, profile = runtime, onFailure?: (error: Error) => void, environment = process.env,
): DesktopHostProcess {
  const host = new DesktopHostProcess(process.execPath, runtime, profile, undefined, environment, onFailure)
  hosts.push(host)
  return host
}

afterEach(async () => {
  await Promise.all(hosts.splice(0).map(host => host.stop()))
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true })
})

describe('desktop host process', () => {
  it('accepts only a startup failure summary without diagnostic metadata', async () => {
    const runtime = projectWithHost(`
process.once('message', message => {
  if (message.type !== 'desktop-auth') throw new Error('expected authentication')
  process.send({ type: 'fatal', message: 'profile startup failed' }, () => process.disconnect())
})
process.send({ type: 'starting' })
`)
    const host = hostProcess(runtime)
    const failure: unknown = await host.start().catch((error: unknown) => error)
    await host.stop()
    expect(failure).toBeInstanceOf(DesktopHostFatalError)
    expect(failure).toHaveProperty('message', 'profile startup failed')
    expect(failure).not.toHaveProperty('diagnostic')
    expect(failure).not.toHaveProperty('hostRunId')
  })

  it('does not retain child stderr output in the failure message', async () => {
    const runtime = projectWithHost(`
process.send({ type: 'starting' })
process.once('message', message => {
  if (message.type === 'desktop-auth') process.send({ type: 'ready', url: 'http://127.0.0.1:3080/' },
    () => process.stderr.write('private-token=fixture-secret', () => process.exit(7)))
})
`)
    const failure = vi.fn()
    const host = hostProcess(runtime, runtime, failure)
    await host.start()
    await expect.poll(() => failure.mock.calls.length).toBe(1)
    expect(failure.mock.calls[0]![0].message).toBe('dsh desktop host exited with 7')
    expect(failure.mock.calls[0]![0].message).not.toContain('fixture-secret')
    await host.stop()
  })

  it('correlates authorized file resources and rejects Host denial over private IPC', async () => {
    const host = hostProcess(projectWithHost())
    await host.start()
    expect(await Promise.all([host.resolveDragResource('local'), host.resolveDragResource('remote')]))
      .toEqual([{ path: '/authorized/report.pdf', sha256: 'a'.repeat(64), size: 42 }, null])
    await expect(host.resolveDragResource('denied')).rejects.toThrow('File read denied')
    await host.stop(true)
    await expect(host.resolveDragResource('local')).rejects.toThrow('file drag: Host is unavailable')
  })

  it('cancels a pending resource request without invalidating later Host requests', async () => {
    const host = hostProcess(projectWithHost())
    await host.start()
    const abort = new AbortController()
    const pending = host.resolveDragResource('pending', abort.signal)
    const rejected = expect(pending).rejects.toThrow('cancelled')
    abort.abort(new Error('cancelled'))
    await rejected
    await expect(host.resolveDragResource('local')).resolves.toMatchObject({ path: '/authorized/report.pdf' })
  })

  it('rejects malformed file authorization responses and terminates the Host', async () => {
    const host = hostProcess(projectWithHost())
    await host.start()
    await expect(host.resolveDragResource('invalid')).rejects.toThrow('invalid IPC event')
  })
  it('correlates task inspections and admission changes over private IPC', async () => {
    const host = hostProcess(projectWithHost())
    await expect(host.updateTasks('inspect')).rejects.toThrow('Host is unavailable')
    await host.start()
    expect(await Promise.all([host.updateTasks('inspect'), host.updateTasks('lock'), host.updateTasks('unlock')]))
      .toEqual([false, true, false])
    await host.stop(true)
    await expect(host.updateTasks('inspect')).rejects.toThrow('Host is unavailable')
  })

  it('correlates quit inspections with task requests and fails an unanswered one at its own deadline', async () => {
    const host = hostProcess(projectWithHost())
    await expect(host.inspectQuit()).rejects.toThrow('desktop quit: Host is unavailable')
    await host.start()
    // Request ids 1 and 2: the fixture answers by id parity, so both control kinds share one id space.
    expect(await Promise.all([host.inspectQuit(), host.updateTasks('inspect')]))
      .toEqual([{ activeTasks: false, scheduledTasks: true }, false])
    const started = Date.now()
    await expect(host.inspectQuit()).rejects.toThrow('desktop quit: inspection timed out')
    expect(Date.now() - started).toBeGreaterThanOrEqual(QUIT_INSPECTION_DEADLINE_MS - 50)
    expect(await host.inspectQuit()).toEqual({ activeTasks: false, scheduledTasks: false })
  }, 15_000)

  it.each([
    'process.exit(17)',
    'process.exit(0)',
  ])('refuses installation when exit lacks successful teardown acknowledgement: %s', async (exit) => {
    const host = hostProcess(projectWithHost(`
      process.send({ type: 'starting' })
      process.on('message', message => {
        if (message.type === 'desktop-auth') process.send({ type: 'ready', url: 'http://127.0.0.1:3080/' })
        if (message.type === 'shutdown') process.stderr.write('token=fixture-secret', () => { ${exit} })
      })
    `))
    await host.start()
    const error = await host.stop(true).then(() => undefined, (error: unknown) => error)
    expect(error).toBeInstanceOf(DesktopHostUncleanExitError)
    expect(String(error)).toContain('shutdown acknowledged false')
    expect(String(error)).toContain('graceful deadline exceeded false')
    expect(String(error)).not.toContain('fixture-secret')
    await expect(host.stop()).resolves.toBeUndefined()
  })

  it('rejects readiness before the private authentication handoff', async () => {
    const host = hostProcess(projectWithHost("process.send({ type: 'ready', url: 'http://127.0.0.1:3080/' })"))
    await expect(host.start()).rejects.toThrow('authentication handoff')
  })

  it.each([
    'https://example.test/',
    'http://127.0.0.1:3080/?token=untrusted',
    'http://user:password@127.0.0.1:3080/',
    'http://127.0.0.1:3080/other',
  ])('rejects an unexpected ready address before using a credential: %s', async (url) => {
    const host = hostProcess(projectWithHost(`
process.on('message', message => {
  if (message.type === 'desktop-auth') process.send({ type: 'ready', url: ${JSON.stringify(url)} })
})
process.send({ type: 'starting' })
`))
    await expect(host.start()).rejects.toThrow('invalid IPC event')
  })

  it('provides a private per-child authentication Cookie with the ordinary ready address', async () => {
    const source = `
process.on('message', message => {
  if (message.type === 'desktop-auth') {
    if (!/^[A-Za-z0-9_-]{43}$/.test(message.secret)) throw new Error('invalid credential')
    process.send({ type: 'ready', url: 'http://127.0.0.1:3080/', injections: [] })
  }
  if (message.type === 'shutdown') process.send({ type: 'shutdown-complete' }, () => process.disconnect())
})
process.send({ type: 'starting' })
`
    const runtime = projectWithHost(source)
    const first = await hostProcess(runtime).start()
    const second = await hostProcess(runtime).start()
    expect(first.url).toBe('http://127.0.0.1:3080/')
    expect(first.authenticationCookie).toMatch(/^dsh-desktop-session=[A-Za-z0-9_-]{43}$/)
    expect(first.authenticationCookie).not.toBe(second.authenticationCookie)
    expect(first.injections).toEqual([])
  })

  it('sends the credential once even when the child repeats starting', async () => {
    const source = HTTP_HOST.replace("process.send({ type: 'starting' })",
      "process.send({ type: 'starting' }); process.send({ type: 'starting' })")
    const host = hostProcess(projectWithHost(source))
    const ready = await host.start()
    expect((await fetch(ready.url)).status).toBe(200)
    await host.stop(true)
  })

  it('reports a credential-free failure when the private IPC send fails', async () => {
    const realSpawn = (await vi.importActual<typeof import('node:child_process')>('node:child_process')).spawn
    let secret = ''
    vi.mocked(childProcess.spawn).mockImplementationOnce((...args) => {
      const child = Reflect.apply(realSpawn, childProcess, args)
      const send = child.send.bind(child)
      vi.spyOn(child, 'send').mockImplementation((message, ...rest) => {
        if (typeof message === 'object' && message !== null && 'type' in message && message.type === 'desktop-auth') {
          secret = 'secret' in message ? String(message.secret) : ''
          const callback = rest.at(-1)
          if (typeof callback !== 'function') throw new Error('missing send callback')
          callback(new Error(`failed message ${secret}`))
          return false
        }
        return Reflect.apply(send, child, [message, ...rest])
      })
      return child
    })
    const failures: Error[] = []
    const host = hostProcess(projectWithHost(), undefined, error => failures.push(error))
    await expect(host.start()).rejects.toThrow('authentication handoff failed')
    await host.stop()
    expect(secret).toMatch(/^[A-Za-z0-9_-]{43}$/)
    expect(failures).toHaveLength(1)
    expect(String(failures[0])).not.toContain(secret)
  })

  it('returns the ordinary Host URL and waits for graceful shutdown', async () => {
    const runtime = projectWithHost()
    const failure = vi.fn()
    const host = hostProcess(runtime, runtime, failure)
    const ready = await host.start()
    expect(new URL(ready.url).search).toBe('')
    expect(ready.authenticationCookie).toMatch(/^dsh-desktop-session=[A-Za-z0-9_-]{43}$/)
    expect(await host.start()).toEqual(ready)
    expect((await fetch(ready.url)).status).toBe(200)
    await host.stop()
    expect(existsSync(join(runtime, 'stopped'))).toBe(true)
    await expect(fetch(ready.url)).rejects.toThrow()
    expect(failure).not.toHaveBeenCalled()
  })

  it('passes external dependencies and package-manager paths to the Host', async () => {
    const runtime = projectWithHost(HTTP_HOST.replace('runtime: process.argv[2]',
      'pnpm: process.argv[5], nodeBin: process.argv[6], primaryRuntime: process.argv[4], runtime: process.argv[2]'))
    const primaryRuntime = join(runtime, 'external-primary-runtime')
    const host = new DesktopHostProcess(process.execPath, runtime, runtime, undefined, process.env,
      undefined, primaryRuntime, { pnpm: join(runtime, 'pnpm.mjs'), nodeBin: join(runtime, 'bin') })
    hosts.push(host)
    const { url } = await host.start()
    expect(await (await fetch(url)).json()).toMatchObject({ primaryRuntime, pnpm: join(runtime, 'pnpm.mjs'), nodeBin: join(runtime, 'bin') })
  })

  it('reports a fatal event after readiness once', async () => {
    const runtime = projectWithHost()
    const failure = vi.fn()
    const host = hostProcess(runtime, runtime, failure)
    const { url } = await host.start()
    await fetch(new URL('/fatal', url))
    await expect.poll(() => failure.mock.calls.length).toBe(1)
    await host.stop()
    expect(failure).toHaveBeenCalledTimes(1)
    expect(failure).toHaveBeenCalledWith(expect.objectContaining({ message: 'plugin unavailable' }))
    expect(failure.mock.calls[0]![0]).not.toHaveProperty('hostRunId')
  })

  it('settles teardown when the executable cannot be spawned', async () => {
    const runtime = projectWithHost()
    const host = new DesktopHostProcess(join(runtime, 'missing-node'), runtime, runtime)
    hosts.push(host)
    await expect(host.start()).rejects.toThrow()
    await host.stop()
  })

  it('loads the resource entry with a separate profile and inherits runtime and package-manager configuration', async () => {
    const runtime = projectWithHost()
    const profile = mkdtempSync(join(tmpdir(), 'desktop-external-profile-'))
    roots.push(profile)
    const host = hostProcess(runtime, profile, undefined, {
      ...process.env, NODE_OPTIONS: '--no-warnings', NODE_PATH: '/custom', NPM_CONFIG_REGISTRY: 'https://registry.example.test/',
    })
    const { url } = await host.start()
    const response = await fetch(url)
    expect(await response.json()).toEqual({ runtime, profile, cwd: realpathSync(profile), nodePath: '/custom', registry: 'https://registry.example.test/', nodeOptions: '--no-warnings', runAsNode: '1', internals: true, privateAuth: true })
  })

  it.each([
    ["process.send({ type: 'fatal', message: 'startup failed' }); process.disconnect()", 'startup failed'],
    ["process.send({ type: 'ready', url: 4 })", 'invalid IPC event'],
    ["process.send({ type: 'fatal', message: 'startup failed', diagnostic: 'private details' })", 'invalid IPC event'],
    ['process.exit(0)', 'host stopped'],
  ])('rejects startup when the child fails before readiness: %s', async (source, message) => {
    const host = hostProcess(projectWithHost(source))
    await expect(host.start()).rejects.toThrow(message)
  })

})

it.each([null, 'stable-account'])('carries Platform identity %s over private IPC and clears credentials on shutdown', async (userId) => {
  const runtime = projectWithHost(HTTP_HOST.replace("process.send({ type: 'ready'", "process.send({ type: 'platform-session', session: { origin: 'https://platform.deepseek.com', userId: " + JSON.stringify(userId) + ", token: 'fixture-secret', embeddedPageDist: 'feat/test' } }); process.send({ type: 'ready'"))
  const changed = vi.fn()
  const host = new DesktopHostProcess(process.execPath, runtime, runtime, undefined, process.env, undefined, undefined, undefined, changed)
  hosts.push(host)
  await host.start()
  expect(changed).toHaveBeenCalledWith({ origin: 'https://platform.deepseek.com', userId, token: 'fixture-secret', embeddedPageDist: 'feat/test' })
  await host.stop()
  expect(changed).toHaveBeenLastCalledWith(null)
})

it.each([undefined, '', 7])('rejects malformed Platform account identity %s on private IPC', async (userId) => {
  const session = { origin: 'https://platform.deepseek.com', token: 'fixture-secret', userId }
  const runtime = projectWithHost(HTTP_HOST.replace("process.send({ type: 'ready'",
    `process.send({ type: 'platform-session', session: ${JSON.stringify(session)} }); process.send({ type: 'ready'`))
  const changed = vi.fn()
  const host = new DesktopHostProcess(process.execPath, runtime, runtime, undefined, process.env, undefined, undefined, undefined, changed)
  hosts.push(host)
  await expect(host.start()).rejects.toThrow('invalid IPC event')
  expect(changed.mock.calls).toEqual([[null]])
  await host.stop()
})
