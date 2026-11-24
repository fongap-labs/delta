import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createComputerProvider, validateComputerAction } from '../src/index.mjs'

const jpeg = Buffer.from([0xff, 0xd8, 0xff, 0xd9])

function screenshot() {
  return {
    status: 'success',
    action: 'screenshot',
    image: jpeg.toString('base64'),
    screen: { width: 2561, height: 1440, imageWidth: 1920, imageHeight: 1080, scale: 1920 / 2561 },
  }
}

function harness({ outcome = 'allowed-once', runNative } = {}) {
  const approvals = []
  const nativeCalls = []
  const savedImages = []
  const provider = createComputerProvider({
    approval: { async request(request) { approvals.push(request); return outcome } },
    attachments: {
      async saveImage(input) {
        savedImages.push(input)
        return { attachmentId: 'a'.repeat(64), mediaType: 'image/jpeg', bytes: input.data.byteLength, width: 1920, height: 1080 }
      },
    },
    async runNative(input) {
      nativeCalls.push(input)
      return runNative ? runNative(input) : screenshot()
    },
  })
  const exec = (id = 'session-a', signal = new AbortController().signal) => ({
    agent: { session: { id } },
    callId: 'call-1',
    signal,
  })
  return { provider, approvals, nativeCalls, savedImages, exec }
}

test('validates desktop actions, bounds, key combinations, and text size', () => {
  assert.deepEqual(validateComputerAction({ action: 'screenshot' }), { action: 'screenshot' })
  assert.deepEqual(validateComputerAction({ action: 'press', screenId: '123e4567-e89b-12d3-a456-426614174000', key: 'Ctrl+A' }).key, 'CTRL+A')
  assert.throws(() => validateComputerAction({ action: 'click', screenId: 'bad', x: -1, y: 0 }), /screenId/)
  assert.throws(() => validateComputerAction({ action: 'press', screenId: '123e4567-e89b-12d3-a456-426614174000', key: 'Ctrl+Alt+Shift+A' }), /key/)
  assert.throws(() => validateComputerAction({ action: 'type', screenId: '123e4567-e89b-12d3-a456-426614174000', text: 'x'.repeat(1001) }), /text/)
})

test('requires approval before observing the desktop and stores the screenshot durably', async () => {
  const h = harness()
  const result = await h.provider.execute({ action: 'screenshot' }, h.exec())
  assert.equal(result.status, 'success')
  assert.equal(result.width, 1920)
  assert.equal(result.height, 1080)
  assert.match(result.screenId, /^[0-9a-f-]{36}$/u)
  assert.equal(h.approvals.length, 1)
  assert.equal(h.approvals[0].toolName, 'computer_control')
  assert.equal(h.nativeCalls.length, 1)
  assert.equal(h.savedImages[0].mediaType, 'image/jpeg')
  await h.provider.dispose()
})

test('maps fresh screenshot coordinates to the full virtual desktop and invalidates them after input', async () => {
  const h = harness()
  const first = await h.provider.execute({ action: 'screenshot' }, h.exec())
  const clicked = await h.provider.execute({
    action: 'click', screenId: first.screenId, x: 1919, y: 1079, button: 'left', count: 1,
  }, h.exec())
  assert.equal(clicked.status, 'success')
  assert.equal(h.nativeCalls[1].x, 2560)
  assert.equal(h.nativeCalls[1].y, 1439)
  assert.equal(h.nativeCalls[1].screenId, undefined)
  assert.notEqual(clicked.screenId, first.screenId)
  const stale = await h.provider.execute({ action: 'press', screenId: first.screenId, key: 'ENTER' }, h.exec())
  assert.equal(stale.status, 'error')
  assert.match(stale.text, /screenshot is stale/)
  assert.equal(h.nativeCalls.length, 2)
  await h.provider.dispose()
})

test('a rejected operation neither captures nor changes the desktop', async () => {
  const h = harness({ outcome: 'rejected' })
  const result = await h.provider.execute({ action: 'screenshot' }, h.exec())
  assert.equal(result.status, 'denied')
  assert.equal(h.nativeCalls.length, 0)
  assert.equal(h.savedImages.length, 0)
  await h.provider.dispose()
})

test('serializes desktop state changes across sessions to prevent overlapping input', async () => {
  let active = 0
  let maximumActive = 0
  const h = harness({ runNative: async () => {
    active += 1
    maximumActive = Math.max(maximumActive, active)
    await new Promise(resolve => setTimeout(resolve, 15))
    active -= 1
    return screenshot()
  } })
  const results = await Promise.all([
    h.provider.execute({ action: 'screenshot' }, h.exec('session-a')),
    h.provider.execute({ action: 'screenshot' }, h.exec('session-b')),
  ])
  assert.equal(results.every(result => result.status === 'success'), true)
  assert.equal(maximumActive, 1)
  await h.provider.dispose()
})

test('rejects malformed native screenshots before saving an attachment', async () => {
  const h = harness({ runNative: async () => ({ ...screenshot(), image: Buffer.from('not jpeg').toString('base64') }) })
  const result = await h.provider.execute({ action: 'screenshot' }, h.exec())
  assert.equal(result.status, 'error')
  assert.equal(h.savedImages.length, 0)
  await h.provider.dispose()
})
