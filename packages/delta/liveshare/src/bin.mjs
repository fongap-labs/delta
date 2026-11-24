import { randomBytes } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import { startRelay } from './relay.mjs'

if (process.argv.includes('--generate-owner-token')) {
  process.stdout.write(`${randomBytes(32).toString('base64url')}\n`)
  process.exit(0)
}

const required = (name) => {
  const value = process.env[name]
  if (typeof value !== 'string' || value.length === 0) throw new Error(`Missing ${name}`)
  return value
}

async function main() {
  const portValue = process.env.DELTA_LIVESHARE_PORT ?? '8443'
  if (!/^\d{1,5}$/u.test(portValue) || Number(portValue) < 1 || Number(portValue) > 65535) {
    throw new Error('DELTA_LIVESHARE_PORT must be an integer from 1 through 65535')
  }
  const relay = await startRelay({
    ownerToken: required('DELTA_LIVESHARE_OWNER'),
    statePath: required('DELTA_LIVESHARE_STATE'),
    key: await readFile(required('DELTA_LIVESHARE_KEY')),
    cert: await readFile(required('DELTA_LIVESHARE_CERT')),
    publicOrigin: required('DELTA_LIVESHARE_ORIGIN'),
    host: process.env.DELTA_LIVESHARE_HOST ?? '127.0.0.1',
    port: Number(portValue),
  })
  process.stdout.write(`Delta LiveShare Relay listening on ${JSON.stringify(relay.address)}\n`)
  const shutdown = async () => {
    await relay.close()
    process.exit(0)
  }
  process.once('SIGINT', () => { void shutdown() })
  process.once('SIGTERM', () => { void shutdown() })
}

main().catch(() => {
  process.stderr.write('Delta LiveShare Relay could not start; check its TLS files and required environment settings.\n')
  process.exitCode = 1
})
