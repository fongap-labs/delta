/** Start Delta with every app-managed path rooted beside the portable application. */
import { mkdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { app } from 'electron'

const appDirectory = dirname(process.execPath)
const portableRoot = app.isPackaged
  ? dirname(appDirectory)
  : resolve(process.env.DELTA_PORTABLE_ROOT ?? join(app.getAppPath(), '..', '..', '.delta-dev'))
const dataRoot = join(portableRoot, 'Data')
const paths = {
  userData: join(dataRoot, 'UserData'),
  sessionData: join(dataRoot, 'Session'),
  logs: join(dataRoot, 'Logs'),
  temp: join(dataRoot, 'Temp'),
  home: join(dataRoot, 'Home'),
}

for (const path of Object.values(paths)) mkdirSync(path, { recursive: true })
app.setName('Delta')
app.setPath('userData', paths.userData)
app.setPath('sessionData', paths.sessionData)
app.setPath('logs', paths.logs)
app.setPath('temp', paths.temp)
process.env.DELTA_PORTABLE = app.isPackaged ? '1' : '0'
process.env.DELTA_PORTABLE_ROOT = portableRoot
process.env.DSH_HOME = paths.home
process.env.DSH_CLIENT_TITLE = 'Delta'
if (process.platform === 'win32') app.setAppUserModelId('org.fongap.delta')

if (app.isPackaged && process.argv.slice(1).join(' ') === '--version') {
  console.log(app.getVersion())
  app.exit(0)
} else {
  await import('./lib/main.js')
}
