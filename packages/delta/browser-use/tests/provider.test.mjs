import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createBrowserProvider, validateBrowserAction } from '../src/index.mjs'

class FakePage {
  currentUrl = 'about:blank'
  closed = false
  password = false
  filled = []
  keys = []
  clicks = []

  isClosed() { return this.closed }
  url() { return this.currentUrl }
  async title() { return 'Example page' }
  async goto(url) { this.currentUrl = url }
  async screenshot() { return Buffer.from('not-a-real-image') }
  async keyboardPress(key) { this.keys.push(key) }
  get keyboard() { return { press: async key => this.keyboardPress(key) } }
  locator(selector) {
    return {
      click: async () => { this.clicks.push(selector) },
      waitFor: async () => {},
      evaluate: async () => this.password,
      fill: async value => { this.filled.push([selector, value]) },
      innerText: async () => 'Hello from an untrusted page',
    }
  }
}

class FakeContext {
  constructor() {
    this.page = new FakePage()
    this.closed = false
    this.routes = []
    this.handlers = new Map()
  }
  pages() { return this.closed ? [] : [this.page] }
  async newPage() { this.page.closed = false; return this.page }
  async route(pattern, handler) { this.routes.push(pattern); this.routeHandler = handler }
  async routeWebSocket(pattern, handler) { this.webSocketPattern = pattern; this.webSocketHandler = handler }
  on(event, handler) { this.handlers.set(event, handler) }
  async close() {
    this.closed = true
    this.page.closed = true
    this.handlers.get('close')?.()
  }
}

function harness({ outcome = 'allowed-once', allowLocalNetwork = false } = {}) {
  const contexts = []
  const launches = []
  const approvals = []
  const savedImages = []
  const provider = createBrowserProvider({
    root: 'C:\\Delta\\Data\\browser',
    allowLocalNetwork,
    chromium: {
      async launchPersistentContext(path, options) {
        launches.push({ path, options })
        const context = new FakeContext()
        contexts.push(context)
        return context
      },
    },
    lookup: async host => host === 'public.test'
      ? [{ address: '93.184.216.34', family: 4 }]
      : [{ address: '127.0.0.1', family: 4 }],
    approval: {
      async request(request) { approvals.push(request); return outcome },
    },
    attachments: {
      async saveImage(input) {
        savedImages.push(input)
        return { attachmentId: 'a'.repeat(64), mediaType: 'image/jpeg', bytes: input.data.byteLength, width: 32, height: 24, name: input.name }
      },
    },
  })
  const exec = (id = 'session-a', signal = new AbortController().signal) => ({
    agent: { session: { id } },
    callId: 'call-1',
    signal,
  })
  return { provider, contexts, launches, approvals, savedImages, exec }
}

test('accepts safe HTTP(S) inputs and rejects credentials or non-web schemes', () => {
  assert.equal(validateBrowserAction({ action: 'open', url: 'https://public.test/a?q=x' }).url, 'https://public.test/a?q=x')
  assert.throws(() => validateBrowserAction({ action: 'open', url: 'file:///C:/Windows/win.ini' }), /HTTP or HTTPS/)
  assert.throws(() => validateBrowserAction({ action: 'open', url: 'https://user:secret@public.test/' }), /HTTP or HTTPS/)
})

test('requires approval before launching the visible browser and returns page text as untrusted data', async () => {
  const h = harness()
  const result = await h.provider.execute({ action: 'open', url: 'https://public.test/start?token=secret' }, h.exec())
  assert.equal(result.status, 'success')
  assert.equal(result.url, 'https://public.test/start')
  assert.equal(result.title, 'Example page')
  assert.equal(result.text, 'Hello from an untrusted page')
  assert.equal(h.approvals.length, 1)
  assert.match(h.approvals[0].displayReason.en, /public\.test/)
  assert.equal(h.launches[0].options.headless, false)
  assert.equal(h.launches[0].options.channel, 'msedge')
  await h.provider.dispose()
})

test('a declined operation does not launch a browser or resolve a page', async () => {
  const h = harness({ outcome: 'denied' })
  const result = await h.provider.execute({ action: 'open', url: 'https://public.test/' }, h.exec())
  assert.equal(result.status, 'denied')
  assert.equal(h.launches.length, 0)
  await h.provider.dispose()
})

test('blocks private network destinations after approval but before launching a browser', async () => {
  const h = harness()
  const result = await h.provider.execute({ action: 'open', url: 'http://127.0.0.1:3000/' }, h.exec())
  assert.equal(result.status, 'error')
  assert.match(result.text, /Local network access is disabled/)
  assert.equal(h.approvals.length, 1)
  assert.equal(h.launches.length, 0)
  await h.provider.dispose()
})

test('checks every browser request and WebSocket destination before allowing egress', async () => {
  const h = harness()
  await h.provider.execute({ action: 'open', url: 'https://public.test/' }, h.exec())
  const context = h.contexts[0]
  const allowed = { continued: false, aborted: false, request: () => ({ url: () => 'https://public.test/image.png' }), async continue() { this.continued = true }, async abort() { this.aborted = true } }
  const blocked = { continued: false, aborted: false, request: () => ({ url: () => 'http://127.0.0.1/admin' }), async continue() { this.continued = true }, async abort() { this.aborted = true } }
  await context.routeHandler(allowed)
  await context.routeHandler(blocked)
  assert.equal(allowed.continued, true)
  assert.equal(blocked.aborted, true)
  let closed
  const socket = {
    url: () => 'wss://localhost/private',
    connectToServer() { throw new Error('private WebSocket unexpectedly connected') },
    async close(options) { closed = options },
  }
  await context.webSocketHandler(socket)
  assert.equal(closed.code, 1008)
  await h.provider.dispose()
})

test('stores screenshots as durable images and separates profiles by session', async () => {
  const h = harness()
  const first = await h.provider.execute({ action: 'screenshot' }, h.exec('session-a'))
  const second = await h.provider.execute({ action: 'screenshot' }, h.exec('session-b'))
  assert.equal(first.status, 'success')
  assert.equal(first.image.mediaType, 'image/jpeg')
  assert.equal(h.savedImages.length, 2)
  assert.notEqual(h.launches[0].path, h.launches[1].path)
  assert.equal(h.launches[0].options.serviceWorkers, 'block')
  await h.provider.dispose()
  assert.equal(h.contexts.every(context => context.closed), true)
})

test('does not enter password fields even after the user approves the browser operation', async () => {
  const h = harness()
  const opened = await h.provider.execute({ action: 'open', url: 'https://public.test/' }, h.exec())
  assert.equal(opened.status, 'success')
  h.contexts[0].page.password = true
  const result = await h.provider.execute({ action: 'fill', selector: 'input[type=password]', text: 'secret' }, h.exec())
  assert.equal(result.status, 'error')
  assert.match(result.text, /will not enter text into password fields/)
  assert.deepEqual(h.contexts[0].page.filled, [])
  await h.provider.dispose()
})

test('close shuts the session window but retains its isolated profile path', async () => {
  const h = harness()
  await h.provider.execute({ action: 'open', url: 'https://public.test/' }, h.exec())
  const profilePath = h.launches[0].path
  const result = await h.provider.execute({ action: 'close' }, h.exec())
  assert.equal(result.status, 'success')
  assert.equal(h.contexts[0].closed, true)
  assert.equal(profilePath, h.launches[0].path)
  await h.provider.dispose()
})
