/** Delta's self-contained startup splash; it never opens an account or network page. */
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { app, BrowserWindow } from 'electron'

const TEAL = '#28727a'

function splashDocument(locale: string, image: Buffer): string {
  const label = locale === 'zh-CN' ? '正在启动…' : 'Starting…'
  const logo = image.toString('base64')
  return `<!doctype html><html lang="${locale}"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Delta</title><style>
    *{box-sizing:border-box}html,body{width:100%;height:100%;margin:0}body{display:grid;place-items:center;background:${TEAL};color:#fff;font:14px/1.5 "Segoe UI",sans-serif;-webkit-font-smoothing:antialiased}main{display:flex;flex-direction:column;align-items:center;gap:10px}img{width:112px;height:112px;border-radius:25px}strong{font-size:20px;font-weight:600;letter-spacing:.04em}.status{display:flex;align-items:center;gap:8px;margin-top:8px;color:#d8eef0;font-size:12px}.dot{width:7px;height:7px;border-radius:50%;background:#fff;animation:pulse 1.1s ease-in-out infinite}@keyframes pulse{50%{opacity:.35}}@media(prefers-reduced-motion:reduce){.dot{animation:none}}
    </style></head><body><main aria-label="Delta"><img src="data:image/png;base64,${logo}" alt="Delta"><strong>Delta</strong><div class="status" role="status"><span class="dot" aria-hidden="true"></span><span>${label}</span></div></main></body></html>`
}

/** Create a small, passive splash while the local workspace starts. */
export function createDeltaSplashWindow(localeId = 'en'): BrowserWindow {
  const imagePath = app.isPackaged
    ? join(process.resourcesPath, 'delta-brand', 'delta-logo-512x512.png')
    : join(app.getAppPath(), 'resources', 'delta-brand', 'delta-logo-512x512.png')
  const image = readFileSync(imagePath)
  const locale = localeId === 'zh-CN' ? 'zh-CN' : 'en'
  const window = new BrowserWindow({
    width: 340,
    height: 300,
    useContentSize: true,
    center: true,
    frame: false,
    resizable: false,
    movable: false,
    minimizable: false,
    maximizable: false,
    fullscreenable: false,
    show: false,
    skipTaskbar: true,
    title: 'Delta',
    backgroundColor: TEAL,
    webPreferences: { nodeIntegration: false, contextIsolation: true, sandbox: true, devTools: false },
  })
  window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))
  window.once('ready-to-show', () => { if (!window.isDestroyed()) window.show() })
  void window.loadURL(`data:text/html;charset=utf-8,${encodeURIComponent(splashDocument(locale, image))}`)
    .catch((error: unknown) => { console.error('delta splash: could not load local screen', error) })
  return window
}
