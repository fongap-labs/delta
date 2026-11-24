import { createServer } from 'node:https'
import { randomUUID } from 'node:crypto'
import { WebSocketServer, WebSocket } from 'ws'
import { createRelayRegistry, DEFAULT_SHARE_TTL_MS } from './registry.mjs'

const MAX_MESSAGE_BYTES = 1024 * 1024
const MAX_VIEWER_CONNECTIONS = 1024
const MAX_VIEWERS_PER_SHARE = 32
const AUTH_TIMEOUT_MS = 5_000
const SWEEP_INTERVAL_MS = 10_000
const REQUEST_ID = /^[A-Za-z0-9_-]{1,64}$/u
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u

const SHARE_PAGE = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="referrer" content="no-referrer"><title>Delta shared session</title>
<link rel="stylesheet" href="/viewer.css"><script type="module" src="/viewer.js"></script></head>
<body><main><h1>Shared Delta session</h1><p id="status" role="status">Connecting…</p><ol id="messages" aria-live="polite"></ol></main></body></html>`

const VIEWER_CSS = `:root{color-scheme:light dark;font:16px/1.5 system-ui,sans-serif}body{margin:0;background:Canvas;color:CanvasText}main{max-width:56rem;margin:0 auto;padding:2rem 1rem}h1{font-size:1.4rem}#messages{list-style:none;padding:0}#messages li{white-space:pre-wrap;overflow-wrap:anywhere;border:1px solid color-mix(in srgb,CanvasText 18%,transparent);border-radius:.6rem;margin:1rem 0;padding:.8rem}#messages strong{display:block;margin-bottom:.4rem}#status{color:GrayText}`

const VIEWER_JS = `const statusNode=document.querySelector('#status');
const messages=document.querySelector('#messages');
const heading=document.querySelector('h1');
const chinese=navigator.language.toLowerCase().startsWith('zh');
const text=chinese?{title:'共享的 Delta 会话',connecting:'正在连接…',unavailable:'此分享已失效、过期或撤销。',readOnly:'只读共享会话',interrupted:'连接中断，正在重连…',user:'用户',assistant:'助手',session:'会话'}:{title:'Shared Delta session',connecting:'Connecting…',unavailable:'This share is unavailable, expired, or revoked.',readOnly:'Read-only shared session',interrupted:'Connection interrupted; reconnecting…',user:'User',assistant:'Assistant',session:'Session'};
document.documentElement.lang=chinese?'zh-CN':'en';
heading.textContent=text.title;
statusNode.textContent=text.connecting;
function fail(){statusNode.textContent=text.unavailable}
function add(value){const item=document.createElement('li');const label=document.createElement('strong');label.textContent=value?.role==='user'?text.user:value?.role==='assistant'?text.assistant:text.session;const body=document.createElement('span');body.textContent=typeof value?.text==='string'?value.text:JSON.stringify(value);item.append(label,body);messages.append(item)}
const token=location.hash.slice(1);history.replaceState(null,'',location.pathname);
if(!/^[A-Za-z0-9_-]{40,128}$/.test(token)){fail()}else{const endpoint=new URL('/v1/viewer',location.href);endpoint.protocol='wss:';let delay=1000;let closed=false;let socket;
function connect(){if(closed)return;socket=new WebSocket(endpoint);socket.addEventListener('open',()=>{delay=1000;socket.send(JSON.stringify({type:'authenticate',token}))});
socket.addEventListener('message',event=>{let value;try{value=JSON.parse(event.data)}catch{fail();socket.close();return}if(value.type==='ready'){statusNode.textContent=text.readOnly;socket.send(JSON.stringify({type:'request',id:'follow',op:'follow'}));return}if(value.type==='response'){if(value.ok&&Array.isArray(value.data?.messages)){messages.replaceChildren();for(const message of value.data.messages)add(message)}else if(!value.ok)fail();return}if(value.type==='event'){if(value.data?.type==='message')add(value.data.message);return}if(value.type==='unavailable')fail()});
socket.addEventListener('close',event=>{if(closed)return;if([4001,4003,1008].includes(event.code)){closed=true;fail();return}statusNode.textContent=text.interrupted;setTimeout(connect,delay);delay=Math.min(delay*2,15000)});socket.addEventListener('error',()=>{})}
connect();window.addEventListener('pagehide',()=>{closed=true;socket?.close()},{once:true})}
`

function bearer(request) {
  const value = request.headers.authorization
  if (typeof value !== 'string' || !value.startsWith('Bearer ')) return undefined
  return value.slice(7)
}

async function readJson(request, limit = 4096) {
  const chunks = []
  let size = 0
  for await (const chunk of request) {
    size += chunk.byteLength
    if (size > limit) throw new RangeError('request too large')
    chunks.push(chunk)
  }
  return JSON.parse(Buffer.concat(chunks).toString('utf8'))
}

function json(response, status, value) {
  const body = JSON.stringify(value)
  response.writeHead(status, {
    'cache-control': 'no-store',
    'content-length': Buffer.byteLength(body),
    'content-security-policy': "default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self' wss:",
    'content-type': 'application/json; charset=utf-8',
    'referrer-policy': 'no-referrer',
    'x-content-type-options': 'nosniff',
  })
  response.end(body)
}

function page(response, contentType, body) {
  response.writeHead(200, {
    'cache-control': 'no-store',
    'content-length': Buffer.byteLength(body),
    'content-security-policy': "default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self' wss:",
    'content-type': contentType,
    'referrer-policy': 'no-referrer',
    'x-content-type-options': 'nosniff',
  })
  response.end(body)
}

function parseViewerFrame(data, binary) {
  if (binary) return undefined
  const text = frameText(data)
  if (Buffer.byteLength(text) > 2048) return undefined
  let value
  try { value = JSON.parse(text) } catch { return undefined }
  if (typeof value !== 'object' || value === null || typeof value.id !== 'string' || !REQUEST_ID.test(value.id)) return undefined
  if (value.type === 'request' && value.op === 'follow') return { type: 'request', id: value.id, op: 'follow' }
  if (value.type === 'cancel') return { type: 'cancel', id: value.id }
  return undefined
}

function parseHostFrame(data, binary) {
  if (binary) return undefined
  const text = frameText(data)
  if (Buffer.byteLength(text) > MAX_MESSAGE_BYTES) return undefined
  let value
  try { value = JSON.parse(text) } catch { return undefined }
  if (typeof value !== 'object' || value === null || !UUID.test(value.viewerId)
      || typeof value.id !== 'string' || !REQUEST_ID.test(value.id)) return undefined
  if (value.type === 'response' && typeof value.ok === 'boolean') return value
  if (value.type === 'event') return value
  return undefined
}

function frameText(data) {
  if (Buffer.isBuffer(data)) return data.toString('utf8')
  if (Array.isArray(data)) return Buffer.concat(data).toString('utf8')
  return Buffer.from(data).toString('utf8')
}

/**
 * Start a TLS-only, self-hosted LiveShare relay. The relay stores only token
 * digests and share metadata; JSON frames are forwarded in memory.
 */
export async function startRelay({ ownerToken, statePath, key, cert, publicOrigin, host = '127.0.0.1', port = 0, now = Date.now } = {}) {
  const origin = new URL(publicOrigin)
  if (origin.protocol !== 'https:' || origin.username || origin.password || origin.pathname !== '/' || origin.search || origin.hash) {
    throw new TypeError('Relay publicOrigin must be an HTTPS origin without credentials, path, query, or fragment')
  }
  if (!key || !cert) throw new TypeError('Relay TLS key and certificate are required')
  const registry = await createRelayRegistry({ ownerToken, statePath, now })
  const server = createServer({ key, cert })
  const sockets = new WebSocketServer({ noServer: true, maxPayload: MAX_MESSAGE_BYTES, perMessageDeflate: false })
  const hosts = new Map()
  const viewers = new Map()
  let viewerConnections = 0
  let closed = false

  const open = (socket) => socket !== undefined && socket.readyState === WebSocket.OPEN
  const active = (id) => {
    const share = registry.getShare(id)
    return share !== undefined && !share.revoked && share.expiresAt > now()
  }

  const closeShare = (id, code, reason) => {
    const hostSocket = hosts.get(id)
    hosts.delete(id)
    if (hostSocket !== undefined && hostSocket.readyState < WebSocket.CLOSING) hostSocket.close(code, reason)
    const peers = viewers.get(id)
    viewers.delete(id)
    if (peers !== undefined) for (const socket of peers.values()) {
      if (socket.readyState < WebSocket.CLOSING) socket.close(code, reason)
    }
  }

  server.on('request', async (request, response) => {
    let url
    try { url = new URL(request.url ?? '/', origin) } catch {
      response.writeHead(400, { 'cache-control': 'no-store' })
      response.end()
      return
    }
    if (request.method === 'GET' && (url.pathname === '/share' || url.pathname === '/share/')) {
      page(response, 'text/html; charset=utf-8', SHARE_PAGE)
      return
    }
    if (request.method === 'GET' && url.pathname === '/viewer.css') {
      page(response, 'text/css; charset=utf-8', VIEWER_CSS)
      return
    }
    if (request.method === 'GET' && url.pathname === '/viewer.js') {
      page(response, 'text/javascript; charset=utf-8', VIEWER_JS)
      return
    }
    if (request.method === 'GET' && url.pathname === '/healthz') {
      json(response, 200, { status: 'ok' })
      return
    }
    if (!registry.authenticateOwner(bearer(request))) {
      json(response, 401, { error: 'unauthorized' })
      return
    }
    if (request.method === 'GET' && url.pathname === '/v1/shares') {
      const shares = registry.listMetadata()
        .filter(item => !item.revoked && item.expiresAt > now())
        .map(item => ({ id: item.id, expiresAt: item.expiresAt, connected: open(hosts.get(item.id)) }))
      json(response, 200, { shares })
      return
    }
    if (request.method === 'POST' && url.pathname === '/v1/shares') {
      try {
        const body = await readJson(request)
        if (typeof body !== 'object' || body === null || Object.keys(body).some(key => key !== 'expiresInMs')) {
          json(response, 400, { error: 'invalid request' })
          return
        }
        const share = await registry.createShare({ expiresInMs: body.expiresInMs ?? DEFAULT_SHARE_TTL_MS })
        json(response, 201, {
          id: share.id,
          expiresAt: share.expiresAt,
          viewerUrl: `${origin.origin}/share/#${share.viewerToken}`,
          hostToken: share.hostToken,
        })
      } catch (error) {
        json(response, error instanceof RangeError ? 400 : 400, { error: error instanceof RangeError ? 'invalid share duration' : 'invalid request' })
      }
      return
    }
    const revoke = /^\/v1\/shares\/([0-9a-f-]+)$/u.exec(url.pathname)
    if (request.method === 'DELETE' && revoke !== null) {
      const id = revoke[1]
      if (!(await registry.revoke(id))) {
        json(response, 404, { error: 'share not found' })
        return
      }
      closeShare(id, 4001, 'share revoked')
      response.writeHead(204, { 'cache-control': 'no-store' })
      response.end()
      return
    }
    json(response, 404, { error: 'not found' })
  })

  server.on('upgrade', (request, socket, head) => {
    let url
    try { url = new URL(request.url ?? '/', origin) } catch {
      socket.destroy()
      return
    }
    if (url.pathname.startsWith('/v1/host/')) {
      if (url.search || url.hash) { socket.destroy(); return }
      const match = /^\/v1\/host\/([0-9a-f-]+)$/u.exec(url.pathname)
      const id = match?.[1]
      const secret = bearer(request)
      if (id === undefined || !registry.authenticateHost(id, secret)) {
        socket.write('HTTP/1.1 401 Unauthorized\r\nConnection: close\r\n\r\n')
        socket.destroy()
        return
      }
      sockets.handleUpgrade(request, socket, head, (websocket) => {
        const current = hosts.get(id)
        if (current !== undefined && current.readyState < WebSocket.CLOSING) current.close(4000, 'host replaced')
        hosts.set(id, websocket)
        viewers.set(id, viewers.get(id) ?? new Map())
        websocket.once('close', () => {
          if (hosts.get(id) === websocket) {
            hosts.delete(id)
            const peers = viewers.get(id)
            viewers.delete(id)
            if (peers !== undefined) for (const peer of peers.values()) peer.close(1012, 'host disconnected')
          }
        })
        websocket.on('message', (data, binary) => {
          if (!active(id)) { closeShare(id, 4001, 'share expired'); return }
          const frame = parseHostFrame(data, binary)
          if (frame === undefined) { websocket.close(1008, 'invalid host frame'); return }
          const peer = viewers.get(id)?.get(frame.viewerId)
          if (peer === undefined || !open(peer)) return
          const output = frame.type === 'response'
            ? { type: 'response', id: frame.id, ok: frame.ok, ...(frame.ok ? { data: frame.data } : { error: 'request refused' }) }
            : { type: 'event', id: frame.id, data: frame.data }
          peer.send(JSON.stringify(output))
        })
        websocket.on('error', () => {})
      })
      return
    }
    if (url.pathname !== '/v1/viewer' || request.headers.origin !== origin.origin) {
      socket.write('HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n')
      socket.destroy()
      return
    }
    sockets.handleUpgrade(request, socket, head, (websocket) => {
      if (viewerConnections >= MAX_VIEWER_CONNECTIONS) {
        websocket.close(1013, 'relay viewer capacity reached')
        return
      }
      viewerConnections += 1
      websocket.once('close', () => { viewerConnections -= 1 })
      let authorized
      const timer = setTimeout(() => websocket.close(4003, 'authentication required'), AUTH_TIMEOUT_MS)
      timer.unref?.()
      websocket.once('message', (data, binary) => {
        clearTimeout(timer)
        if (binary || Buffer.byteLength(frameText(data)) > 1024) { websocket.close(4003, 'authentication failed'); return }
        let frame
        try { frame = JSON.parse(frameText(data)) } catch { websocket.close(4003, 'authentication failed'); return }
        if (frame?.type !== 'authenticate' || typeof frame.token !== 'string') { websocket.close(4003, 'authentication failed'); return }
        const share = registry.authenticateViewer(frame.token)
        if (share === undefined || !open(hosts.get(share.id))) { websocket.close(4003, 'share unavailable'); return }
        const currentPeers = viewers.get(share.id)
        if ((currentPeers?.size ?? 0) >= MAX_VIEWERS_PER_SHARE) { websocket.close(1013, 'share viewer capacity reached'); return }
        authorized = { ...share, viewerId: randomUUID() }
        const peers = viewers.get(share.id) ?? new Map()
        peers.set(authorized.viewerId, websocket)
        viewers.set(share.id, peers)
        websocket.send(JSON.stringify({ type: 'ready', expiresAt: share.expiresAt }))
        websocket.on('message', (message, isBinary) => {
          if (!active(authorized.id)) { closeShare(authorized.id, 4001, 'share expired'); return }
          const requestFrame = parseViewerFrame(message, isBinary)
          if (requestFrame === undefined) { websocket.close(1008, 'unsupported viewer operation'); return }
          const hostSocket = hosts.get(authorized.id)
          if (!open(hostSocket)) { websocket.close(1012, 'host disconnected'); return }
          hostSocket.send(JSON.stringify({ ...requestFrame, viewerId: authorized.viewerId }))
        })
        websocket.once('close', () => {
          const current = viewers.get(authorized.id)
          current?.delete(authorized.viewerId)
          if (current?.size === 0) viewers.delete(authorized.id)
          const hostSocket = hosts.get(authorized.id)
          if (open(hostSocket)) hostSocket.send(JSON.stringify({ type: 'disconnect', id: 'disconnect', viewerId: authorized.viewerId }))
        })
        websocket.on('error', () => {})
      })
    })
  })

  const sweep = setInterval(async () => {
    try {
      for (const id of await registry.expire()) closeShare(id, 4001, 'share expired')
    } catch { /* Fail closed on the next viewer frame; do not log data or credentials. */ }
  }, SWEEP_INTERVAL_MS)
  sweep.unref?.()
  server.on('error', () => {})
  sockets.on('error', () => {})

  await new Promise((resolve, reject) => {
    const fail = (error) => { server.off('listening', ready); reject(error) }
    const ready = () => { server.off('error', fail); resolve() }
    server.once('error', fail)
    server.once('listening', ready)
    server.listen(port, host)
  })

  return Object.freeze({
    address: server.address(),
    origin: origin.origin,
    async close() {
      if (closed) return
      closed = true
      clearInterval(sweep)
      for (const id of new Set([...hosts.keys(), ...viewers.keys()])) closeShare(id, 1001, 'relay shutdown')
      await new Promise((resolve) => server.close(() => resolve()))
      sockets.close()
    },
  })
}
