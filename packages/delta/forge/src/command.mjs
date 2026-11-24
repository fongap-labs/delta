import { ForgeClient } from './index.mjs'

export const name = 'delta-forge'
export const inject = ['commands', 'credentials']
const REFERENCES = Object.freeze({ github: 'DELTA_GITHUB_TOKEN', gitlab: 'DELTA_GITLAB_TOKEN' })
const REF = /^[A-Za-z_][A-Za-z0-9_]*$/u

/** Resolve configured providers once per plugin lifetime; secrets remain per-operation. */
export function createForgeProviders(config = {}) {
  return Object.fromEntries(['github', 'gitlab'].map(provider => {
    const settings = config[provider] ?? {}
    const client = new ForgeClient(provider, { origin: settings.origin })
    // An explicitly configured origin never inherits the public-service token.
    const tokenRef = settings.tokenRef ?? (settings.origin === undefined ? REFERENCES[provider] : undefined)
    if (tokenRef !== undefined && (typeof tokenRef !== 'string' || !REF.test(tokenRef))) throw new TypeError('Invalid Forge credential reference')
    return [provider, { client, tokenRef }]
  }))
}

/** One user-initiated read; credentials are resolved afresh and never cached. */
export function apply(ctx, config = {}) {
  const providers = createForgeProviders(config)
  const active = new Set()
  ctx.effect(function* () {
    yield async () => { await Promise.allSettled(active) }
    yield ctx.commands.register({
      definitionId: '@delta/forge',
      name: 'forge',
      description: 'Read a GitHub pull request or GitLab merge request',
      input: { hint: 'github|gitlab <owner/project> <number>' },
      // A malformed input may contain a pasted secret. Never persist command input.
      recordInput: false,
      handler({ rawInput, signal }) {
        const fields = rawInput.trim().split(/\s+/u)
        const [provider, project, rawNumber] = fields
        if (fields.length !== 3 || !Object.hasOwn(providers, provider) || !/^[1-9]\d{0,15}$/u.test(rawNumber)) {
          return { kind: 'error', text: 'Usage: /forge github|gitlab <owner/project> <number>' }
        }
        const operation = (async () => {
          try {
            signal.throwIfAborted()
            const { client, tokenRef } = providers[provider]
            const resolved = tokenRef === undefined ? undefined : await ctx.credentials.resolve(tokenRef)
            signal.throwIfAborted()
            const result = await client.readChange(project, Number(rawNumber), { token: resolved?.value, signal })
            return { kind: 'success', text: JSON.stringify(result, null, 2) }
          } catch (error) {
            if (signal.aborted) return { kind: 'error', text: 'Forge request cancelled' }
            const message = error instanceof Error ? error.message : ''
            const http = /^Forge request failed with HTTP (\d{3})$/u.exec(message)
            return { kind: 'error', text: http ? `Forge request failed with HTTP ${http[1]}` : 'Forge request failed; check the project, credential, and connection' }
          }
        })()
        active.add(operation)
        void operation.then(() => active.delete(operation), () => active.delete(operation))
        return operation
      },
    })
  }, 'delta forge command')
}
