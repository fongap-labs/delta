import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import { brandString } from '@deepseek-ai/dsh-brand'
import LlmRuntime, { createUserMessage, type ReasoningEffortId } from '@deepseek-ai/dsh-llm'
import type { StreamChunk } from '@deepseek-ai/dsh-llm'
import { liveConfig } from '../../../settings/settings/tests/live-config.ts'
const configurations = new WeakMap<Context, Awaited<ReturnType<typeof liveConfig>>>()
import * as LlmPiAi from '@deepseek-ai/dsh-llm-pi-ai'
import { PiAiAdapter } from '@deepseek-ai/dsh-llm-pi-ai'
import type { ContextFormed } from '@deepseek-ai/dsh-llm'
import { AssistantMessageEventStream } from '@earendil-works/pi-ai/utils/event-stream'
import { normalizeContext } from '@earendil-works/pi-ai/utils/transcript'
import type { Api, Model, OpenAICompletionsCompat, Provider } from '@earendil-works/pi-ai'
import { catalogModels, catalogProvider } from '../src/catalog.ts'
import { resolveProfiles } from '../src/config.ts'
import { createModels, createProvider, getSupportedThinkingLevels } from '../src/models.ts'
import { buildProvider, supportedProtocols } from '../src/provider.ts'
import { assemble } from './assemble.ts'
import { memoryAuth } from './auth-double.ts'
import { closeMockServers, mockServer, textEvents } from './mock-server.ts'

declare module '@deepseek-ai/dsh-llm' {
  interface MessageSourceMap {
    'test': { kind: 'test' } & ContextFormed
  }
}

// Routes name their credential by reference; the value lives in the
// environment, which is the layer the adapter falls back to without a
// mounted credentials seam.
const KEY_ENV = 'PI_TEST_KEY'

beforeEach(() => {
  vi.stubEnv(KEY_ENV, 'test-key')
})

afterEach(async () => {
  vi.unstubAllEnvs()
  await closeMockServers()
})

/** The dormant composition with Loader-managed live Config. */
async function bootWithSettings(config: LlmPiAi.Options): Promise<Context> {
  const ctx = new Context()
  await ctx.plugin(LlmRuntime)
  configurations.set(ctx, await liveConfig(ctx, LlmPiAi, config))
  return ctx
}

/** A complete hand-declared route: nothing about it exists in pi-ai's catalog. */
function gateway(baseURL: string, overrides: Record<string, unknown> = {}): LlmPiAi.Options {
  return {
    providers: {
      'acme-gateway': {
        apiKeyEnv: KEY_ENV,
        displayName: 'Acme Gateway',
        api: 'openai-completions',
        baseURL,
        models: [{ id: 'acme-large', name: 'Acme Large', contextWindow: 65_536, maxTokens: 4096 }],
        ...overrides,
      },
    },
  }
}

async function harness(config: LlmPiAi.Options): Promise<Context> {
  const ctx = new Context()
  await ctx.plugin(LlmRuntime)
  await ctx.plugin(LlmPiAi, config)
  return ctx
}

describe('hand-declared providers', () => {
  it('serves a route pi-ai has never heard of from its own declaration', async () => {
    const server = await mockServer([{ events: textEvents }])
    const ctx = await harness(gateway(`${server.url}/v1`))

    const result = await assemble(ctx, {
      provider: 'acme-gateway',
      model: 'acme-large',
      messages: [createUserMessage({
        content: [{ type: 'text', text: 'hi' }],
        source: { kind: 'test' },
      })],
    })

    expect(result.message.content).toEqual([{ type: 'text', text: 'hello' }])
    expect(result.finish).toEqual({ kind: 'stop' })
    expect(server.paths).toEqual(['/v1/chat/completions'])
    // The reference resolved through the environment and reached the wire.
    expect(server.headers[0]?.authorization).toBe('Bearer test-key')
  })

  it('lists and resolves the declared models rather than a catalog', async () => {
    const server = await mockServer([])
    const ctx = await harness(gateway(`${server.url}/v1`))

    expect(await ctx.llm.listModels('acme-gateway')).toEqual([
      { provider: 'acme-gateway', id: 'acme-large', name: 'Acme Large', inputModalities: ['text'] },
    ])
    const info = await ctx.llm.resolveModelInfo('acme-gateway', 'acme-large')
    expect(info).toMatchObject({
      provider: 'acme-gateway',
      id: 'acme-large',
      name: 'Acme Large',
      context: { contextWindow: 65_536 },
      defaultMaxTokens: 4096,
    })
  })

  it('joins the configurable-provider directory so a settings surface can reach it', async () => {
    const server = await mockServer([])
    const ctx = await harness(gateway(`${server.url}/v1`))
    const directory = ctx.llm.listConfigurableProviders()

    expect(directory).toContainEqual({
      provider: 'acme-gateway',
      displayName: 'Acme Gateway',
      settingsNs: 'llm-pi-ai',
      settingsPath: ['providers', 'acme-gateway'],
      // Nothing in the installed catalog answers for this route, which is what
      // configuration surfaces mark as a route this deployment declared.
      declared: true,
    })
    // Membership of the catalog, not of the settings document: a shipped
    // provider carries a stored profile the moment anyone corrects it.
    expect(directory.filter(entry => entry.declared).map(entry => entry.provider))
      .toEqual(['acme-gateway'])
    expect(directory.find(entry => entry.provider === 'deepseek')?.declared).toBe(false)
  })

  it('sizes a model the catalog cannot describe from the route\u2019s own fallbacks', () => {
    const resolved = resolveProfiles({
      'acme-gateway': {
        api: 'openai-completions',
        baseURL: 'https://acme.test',
        // A listing endpoint that discloses nothing but ids still yields a
        // serviceable route.
        models: [{ id: 'bare' }, { id: 'sized', contextWindow: 8192, maxTokens: 512 }],
      },
      'tuned-gateway': {
        api: 'openai-completions',
        baseURL: 'https://tuned.test',
        defaultContextWindow: 4096,
        defaultMaxTokens: 256,
        models: [{ id: 'bare' }],
      },
    })
    const modelsOf = (route: string): readonly { id: string; contextWindow: number; maxTokens: number }[] =>
      resolved.get(route)?.piProvider?.getModels() ?? []

    expect(modelsOf('acme-gateway')).toMatchObject([
      { id: 'bare', contextWindow: 1_000_000, maxTokens: 128_000 },
      { id: 'sized', contextWindow: 8192, maxTokens: 512 },
    ])
    // The fallback is a guess, so a deployment whose gateway serves smaller
    // models corrects it once for the whole route.
    expect(modelsOf('tuned-gateway')).toMatchObject([{ id: 'bare', contextWindow: 4096, maxTokens: 256 }])
    // Only an explicitly configured cap is a request default; a fallback is
    // the model's capability and stops there.
    expect(resolved.get('acme-gateway')?.configuredMaxTokens.get('bare')).toBeUndefined()
    expect(resolved.get('acme-gateway')?.configuredMaxTokens.get('sized')).toBe(512)
  })

  it('rejects a model the route cannot identify', () => {
    const declare = (model: LlmPiAi.PiAiModelProfile): (() => unknown) =>
      () => resolveProfiles({ 'acme-gateway': { api: 'openai-completions', baseURL: 'https://acme.test', models: [model] } })

    expect(declare({ id: '' })).toThrow(/empty id/)
    expect(() => resolveProfiles({
      'acme-gateway': {
        api: 'openai-completions',
        baseURL: 'https://acme.test',
        models: [{ id: 'dup', contextWindow: 1, maxTokens: 1 }, { id: 'dup', contextWindow: 2, maxTokens: 2 }],
      },
    })).toThrow(/more than once/)
  })

  it('retains duplicate-id diagnostics without offering the ambiguous model after loading', () => {
    const profile = resolveProfiles({
      'acme-gateway': {
        api: 'openai-completions',
        baseURL: 'https://acme.test',
        models: [{ id: 'dup' }, { id: 'valid' }, { id: 'dup' }],
      },
    }, 'deferred').get('acme-gateway')
    expect(profile?.modelErrors.get('dup')).toContain('lists model "dup" more than once')
    expect(profile?.piProvider?.getModels().map(model => model.id)).toEqual(['valid'])
  })

  it('rejects a declaration that names no wire protocol or endpoint', () => {
    expect(() => resolveProfiles({
      'acme-gateway': { baseURL: 'https://acme.test', models: [{ id: 'm', contextWindow: 1, maxTokens: 1 }] },
    })).toThrow(/needs an api/)
    expect(() => resolveProfiles({
      'acme-gateway': { api: 'openai-completions', models: [{ id: 'm', contextWindow: 1, maxTokens: 1 }] },
    })).toThrow(/needs a baseURL/)
  })

  it('retains the missing-api model diagnostic when a stored custom provider cannot be built', () => {
    const profile = resolveProfiles({
      'acme-gateway': { baseURL: 'https://acme.test', models: [{ id: '111' }] },
    }, 'deferred').get('acme-gateway')!
    const failure = 'llm-pi-ai: provider "acme-gateway" model "111" needs an api; '
      + 'the installed catalog does not describe it, so set the route\'s api to the wire protocol its endpoint speaks'

    expect(profile.catalogError).toBe(failure)
    expect(profile.modelErrors.get('111')).toBe(failure)
    expect(profile.piProvider).toBeUndefined()
  })

  it.each(['bedrock-converse-stream', 'google-vertex', 'azure-openai-responses', 'openai-codex-responses'])(
    'refuses %s, whose authentication a profile cannot express',
    (api) => {
      // These need SigV4 credentials and a region, a project plus ADC, provider
      // environment and an api-version, or OAuth — none of which a key, an
      // endpoint, and headers can carry, so a route naming one would be built
      // unable to authenticate.
      expect(supportedProtocols()).not.toContain(api)
      expect(() => buildProvider({ provider: 'acme-gateway', displayName: 'Acme', api, models: [], namesCredential: true }))
        .toThrow(/cannot serve; supported protocols are/)
    },
  )

  it('rejects a protocol this build cannot serve, and a route that names none', () => {
    const spec = { provider: 'acme-gateway', displayName: 'Acme Gateway', models: [], namesCredential: true }
    expect(() => buildProvider({ ...spec, api: 'quantum-telepathy' }))
      .toThrow(/cannot serve; supported protocols are/)
    expect(() => buildProvider(spec)).toThrow(/cannot serve; supported protocols are/)
  })

  it('leaves an unauthenticated route to its protocol rather than inventing a credential', async () => {
    const server = await mockServer([{ events: textEvents }])
    // Naming no credential is the deliberately unauthenticated posture — a
    // named reference that resolved to nothing would have failed with
    // MISSING_CREDENTIAL long before this point. The route resolves as
    // configured and the protocol decides: pi-ai's OpenAI-compatible
    // implementation wants a key or an Authorization header of its own, and
    // says so instead of the harness guessing a placeholder.
    const ctx = await harness({
      providers: {
        'local-llm': {
          api: 'openai-completions',
          baseURL: `${server.url}/v1`,
          models: [{ id: 'qwen3', contextWindow: 32_768, maxTokens: 2048 }],
        },
      },
    })

    const result = await assemble(ctx, { provider: 'local-llm', model: 'qwen3', messages: [] })
    expect(result.finish).toMatchObject({
      kind: 'error',
      failure: { message: 'No API key for provider: local-llm' },
    })
    expect(server.requests).toHaveLength(0)
  })

  it('authenticates an unauthenticated route through a configured header', async () => {
    const server = await mockServer([{ events: textEvents }])
    const ctx = await harness({
      providers: {
        'local-llm': {
          api: 'openai-completions',
          baseURL: `${server.url}/v1`,
          headers: { Authorization: 'Bearer local' },
          models: [{ id: 'qwen3', contextWindow: 32_768, maxTokens: 2048 }],
        },
      },
    })

    const result = await assemble(ctx, { provider: 'local-llm', model: 'qwen3', messages: [] })
    expect(result.finish).toEqual({ kind: 'stop' })
    expect(server.headers[0]?.authorization).toBe('Bearer local')
  })

  it('rejects a capacity that is not a positive integer', () => {
    const declare = (model: LlmPiAi.PiAiModelProfile): (() => unknown) =>
      () => resolveProfiles({ 'acme-gateway': { api: 'openai-completions', baseURL: 'https://acme.test', models: [model] } })

    expect(declare({ id: 'm', contextWindow: 0, maxTokens: 1 })).toThrow(/contextWindow must be a positive integer/)
    expect(declare({ id: 'm', contextWindow: 1.5, maxTokens: 1 })).toThrow(/contextWindow must be a positive integer/)
    expect(declare({ id: 'm', contextWindow: 1, maxTokens: 0 })).toThrow(/maxTokens must be a positive integer/)
    expect(declare({ id: 'm', contextWindow: 1, maxTokens: 1.5 })).toThrow(/maxTokens must be a positive integer/)
  })

  it('names the route key when no displayName is configured', () => {
    const resolved = resolveProfiles({
      'acme-gateway': {
        api: 'openai-completions',
        baseURL: 'https://acme.test',
        models: [{ id: 'm', contextWindow: 1, maxTokens: 1 }],
      },
    })
    expect(resolved.get('acme-gateway')?.displayName).toBe('acme-gateway')
    expect(() => resolveProfiles({ 'acme-gateway': { displayName: '' } })).toThrow(/empty displayName/)
  })
})

describe('per-model reasoning efforts', () => {
  /** One hand-declared route holding exactly the given models. */
  function declared(models: LlmPiAi.PiAiModelProfile[]): Record<string, LlmPiAi.PiAiProviderProfile> {
    return { 'acme-gateway': { api: 'openai-completions', baseURL: 'https://acme.test', models } }
  }

  /** The first materialized model of one route, or throw. */
  function modelOf(providers: Record<string, LlmPiAi.PiAiProviderProfile>, route = 'acme-gateway'): Model<Api> {
    const [model] = resolveProfiles(providers).get(route)?.piProvider?.getModels() ?? []
    if (model === undefined) throw new Error(`route "${route}" resolved no models`)
    return model
  }

  it('declares selectable levels with their wire spellings on a hand-declared model', () => {
    const model = modelOf(declared([{
      id: 'acme-think',
      reasoningEfforts: { off: null, low: 'low', high: 'high', max: 'ultra' },
    }]))

    expect(model.reasoning).toBe(true)
    // Undeclared levels are pinned null rather than left to pi-ai's own
    // defaulting, which is asymmetric: an absent key means "supported" for the
    // five base levels but "unsupported" for xhigh/max. A profile author
    // should not need to know that. Declared `off` with no value stays absent
    // from the map — supported, send nothing.
    expect(model.thinkingLevelMap).toEqual({
      minimal: null,
      medium: null,
      xhigh: null,
      low: 'low',
      high: 'high',
      max: 'ultra',
    })
    expect(getSupportedThinkingLevels(model)).toEqual(['off', 'low', 'high', 'max'])
  })

  it('keeps a declared off value in the map for dispatch to send', () => {
    const model = modelOf(declared([{ id: 'm', reasoningEfforts: { off: 'none', high: 'high' } }]))
    expect(model.thinkingLevelMap?.off).toBe('none')
    expect(getSupportedThinkingLevels(model)).toEqual(['off', 'high'])
  })

  it('offers exactly the declared keys: leaving off out makes thinking mandatory', () => {
    const model = modelOf(declared([{ id: 'm', reasoningEfforts: { high: 'high' } }]))
    expect(getSupportedThinkingLevels(model)).toEqual(['high'])
  })

  it('rejects a declaration that offers nothing or spells a level it cannot send', () => {
    const declare = (efforts: NonNullable<LlmPiAi.PiAiModelProfile['reasoningEfforts']>): (() => unknown) =>
      () => resolveProfiles(declared([{ id: 'm', reasoningEfforts: efforts }]))

    expect(declare({})).toThrow(/empty reasoningEfforts/)
    // A YAML `reasoningEfforts:` left valueless arrives as null through the
    // schema union; it declares nothing and is not a spelling of "inherit".
    expect(declare(null as never)).toThrow(/empty reasoningEfforts/)
    expect(declare({ off: null })).toThrow(/offers no level beyond "off"/)
    expect(declare({ off: 'none' })).toThrow(/offers no level beyond "off"/)
    expect(declare({ high: null })).toThrow(/only "off" may leave it empty/)
    expect(declare({ high: '' })).toThrow(/must not be an empty string/)
  })
})

describe('compat switches', () => {
  /** The materialized models of one route, keyed by id. */
  function modelsOf(providers: Record<string, LlmPiAi.PiAiProviderProfile>, route: string): Map<string, Model<Api>> {
    const models = resolveProfiles(providers).get(route)?.piProvider?.getModels() ?? []
    return new Map(models.map(model => [model.id, model]))
  }

  it('applies route switches to every openai-completions model, entries winning per field', () => {
    const models = modelsOf({
      'acme-gateway': {
        api: 'openai-completions',
        baseURL: 'https://acme.test',
        compat: { thinkingFormat: 'deepseek' },
        models: [
          { id: 'dialect-default', reasoningEfforts: { off: null, high: 'high' } },
          { id: 'dialect-odd', compat: { thinkingFormat: 'openai', supportsReasoningEffort: false } },
        ],
      },
    }, 'acme-gateway')

    expect(models.get('dialect-default')?.compat).toEqual({ thinkingFormat: 'deepseek' })
    expect(models.get('dialect-odd')?.compat).toEqual({ thinkingFormat: 'openai', supportsReasoningEffort: false })
  })

  it('rejects a model-level switch on a protocol that has no such field, naming what it offers', () => {
    expect(() => resolveProfiles({
      anthropic: {
        models: [{ id: 'claude-sonnet-4-5', compat: { thinkingFormat: 'openai' } }],
      },
    })).toThrow(/its api is "anthropic-messages", which does not take it.*exists on openai-completions/s)
  })

  it('rejects route switches no model on the route can take', () => {
    expect(() => resolveProfiles({
      anthropic: { compat: { thinkingFormat: 'openai' } },
    })).toThrow(/no model on the route speaks a protocol that takes it/)
  })

  it('carries the developer-role switch onto a hand-declared reasoning model', () => {
    // pi-ai reads this switch only for a reasoning model, and detects it from
    // the endpoint URL — which for a private gateway answers as though it were
    // OpenAI itself, so the route must be able to say otherwise.
    const models = modelsOf({
      'acme-gateway': {
        api: 'openai-completions',
        baseURL: 'https://acme.test',
        compat: { supportsDeveloperRole: false, maxTokensField: 'max_tokens' },
        models: [{ id: 'acme-think', reasoningEfforts: { off: null, high: 'high' } }],
      },
    }, 'acme-gateway')

    expect(models.get('acme-think')?.compat).toEqual({
      supportsDeveloperRole: false,
      maxTokensField: 'max_tokens',
    })
  })

  it('carries a switch both OpenAI protocols declare onto an openai-responses route', () => {
    const models = modelsOf({
      'acme-responses': {
        api: 'openai-responses',
        baseURL: 'https://acme.test',
        compat: { supportsDeveloperRole: false },
        models: [{ id: 'acme-r', reasoningEfforts: { off: null, high: 'high' } }],
      },
    }, 'acme-responses')

    expect(models.get('acme-r')?.compat).toEqual({ supportsDeveloperRole: false })
  })

  it('carries an anthropic-only switch onto an anthropic-messages route', () => {
    const models = modelsOf({
      'acme-claude': {
        api: 'anthropic-messages',
        baseURL: 'https://acme.test',
        compat: { supportsTemperature: false, supportsCacheControlOnTools: false },
        models: [{ id: 'acme-opus' }],
      },
    }, 'acme-claude')

    expect(models.get('acme-opus')?.compat).toEqual({
      supportsTemperature: false,
      supportsCacheControlOnTools: false,
    })
  })

  it('carries chat-template kwargs beside the thinking format that dispatches through them', () => {
    const models = modelsOf({
      'acme-qwen': {
        api: 'openai-completions',
        baseURL: 'https://acme.test',
        models: [{
          id: 'qwen-local',
          reasoningEfforts: { off: null, medium: 'medium' },
          compat: {
            thinkingFormat: 'qwen-chat-template',
            chatTemplateKwargs: { enable_thinking: { $var: 'thinking.enabled' } },
          },
        }],
      },
    }, 'acme-qwen')

    expect(models.get('qwen-local')?.compat).toEqual({
      thinkingFormat: 'qwen-chat-template',
      chatTemplateKwargs: { enable_thinking: { $var: 'thinking.enabled' } },
    })
  })

  it('carries private-endpoint stream and reasoning controls', () => {
    const models = modelsOf({
      'acme-baseten': {
        api: 'openai-completions',
        baseURL: 'https://acme.test',
        models: [{
          id: 'reasoning-local',
          compat: {
            supportsFinishReason: false,
            thinkingFormat: 'baseten',
            chatTemplateArgs: { enable_thinking: { $var: 'thinking.enabled' } },
            supportsThinkingTokenBudget: true,
          },
        }],
      },
    }, 'acme-baseten')

    expect(models.get('reasoning-local')?.compat).toEqual({
      supportsFinishReason: false,
      thinkingFormat: 'baseten',
      chatTemplateArgs: { enable_thinking: { $var: 'thinking.enabled' } },
      supportsThinkingTokenBudget: true,
    })
  })

  it('rejects a model switch on an unrecognized protocol as having no configurable compat', () => {
    expect(() => resolveProfiles({
      'acme-gateway': {
        api: 'acme-chat',
        baseURL: 'https://acme.test',
        models: [{ id: 'acme-a', compat: { supportsStore: false } }],
      },
    })).toThrow(/its api is "acme-chat", which does not take it.*"acme-chat" offers no configurable compat/s)
  })

  it('refuses a valueless compat key written through the composed settings path', async () => {
    // The write path an operator reaches: a section resolved by schemastery,
    // judged by this adapter's section validator before it is stored.
    // schemastery keeps the null, so nothing but that check stands between it
    // and `Model.compat`.
    const ctx = await bootWithSettings({})
    await expect(configurations.get(ctx)!.update({
      providers: {
        'acme-gateway': {
          api: 'openai-completions',
          baseURL: 'https://acme.test/v1',
          compat: { supportsDeveloperRole: null },
          models: [{ id: 'acme-a' }],
        },
      },
    })).rejects.toThrow(/compat "supportsDeveloperRole" with no value/)
  })

  it('carries a compat switch from a written settings section onto the wire', async () => {
    // End to end for the reported gap: the switch enters as configuration and
    // changes the request the provider receives, not merely the resolved model.
    vi.stubEnv(KEY_ENV, 'test-key')
    const server = await mockServer([{ events: textEvents }])
    const ctx = await bootWithSettings({})
    await configurations.get(ctx)!.update({
      providers: {
        'acme-gateway': {
          apiKeyEnv: KEY_ENV,
          api: 'openai-completions',
          baseURL: `${server.url}/v1`,
          compat: { supportsDeveloperRole: false },
          models: [{ id: 'acme-think', reasoningEfforts: { off: null, high: 'high' } }],
        },
      },
    })

    await assemble(ctx, {
      provider: 'acme-gateway',
      model: 'acme-think',
      reasoningEffort: brandString<ReasoningEffortId>('high'),
      system: 'you are a harness',
      messages: [],
    })

    const request = server.requests[0] as { messages: { role: string }[] }
    expect(request.messages.map(message => message.role)).toEqual(['system'])
  })

  it('refuses a valueless compat key rather than writing null over the catalog', () => {
    // schemastery passes a YAML bare key through as null. Carried forward it
    // would replace the installed entry's value, and pi-ai's `??` would then
    // reach for its baseURL detection — the "written but not applied" outcome.
    expect(() => resolveProfiles({
      'acme-gateway': {
        api: 'openai-completions',
        baseURL: 'https://acme.test',
        compat: { supportsDeveloperRole: null } as never,
        models: [{ id: 'acme-a' }],
      },
    })).toThrow(/compat "supportsDeveloperRole" with no value/)
  })

  it('refuses a compat key whose value is undefined, as a cordis.yml entry can write', () => {
    // `!!js undefined` reaches the same state as a YAML bare key, and
    // schemastery keeps the key either way, so both are refused together.
    expect(() => resolveProfiles({
      'acme-gateway': {
        api: 'openai-completions',
        baseURL: 'https://acme.test',
        compat: { supportsDeveloperRole: undefined } as never,
        models: [{ id: 'acme-a' }],
      },
    })).toThrow(/compat "supportsDeveloperRole" with no value/)
  })

  it('refuses a valueless compat key on a model entry too', () => {
    expect(() => resolveProfiles({
      deepseek: {
        modelOverrides: { 'deepseek-flash': { compat: { requiresReasoningContentOnAssistantMessages: null } } as never },
      },
    })).toThrow(/model "deepseek-flash" sets compat "requiresReasoningContentOnAssistantMessages" with no value/)
  })

  it('serves the Responses compat type on every protocol pi-ai gives it to', () => {
    // pi-ai types azure-openai-responses and openai-codex-responses with the
    // same OpenAIResponsesCompat, so a switch settable on one is settable on all.
    for (const route of ['azure', 'openai-codex']) {
      const models = modelsOf({ [route]: { compat: { supportsDeveloperRole: false } } }, route)
      const model = [...models.values()].find(model => model.api === (route === 'azure' ? 'azure-openai-responses' : 'openai-codex-responses'))
      expect(model).toBeDefined()
      expect(model?.compat).toMatchObject({ supportsDeveloperRole: false })
    }
  })

  it('serves the Bedrock compat type on its own protocol', () => {
    const models = modelsOf({ 'amazon-bedrock': { compat: { supportsStrictMode: false } } }, 'amazon-bedrock')
    const [first] = [...models.values()]
    expect((first?.compat as { supportsStrictMode?: boolean }).supportsStrictMode).toBe(false)
  })

  it('refuses a compat key no wire protocol declares instead of dropping it', () => {
    // Schemastery passes unknown keys through, so silently dropping one would
    // make an unreadable switch look applied; the resolver must refuse it.
    expect(() => resolveProfiles({
      'acme-gateway': {
        api: 'openai-completions',
        baseURL: 'https://acme.test',
        compat: { supportsDevelperRole: false } as never,
        models: [{ id: 'acme-a' }],
      },
    })).toThrow(/compat "supportsDevelperRole", which no wire protocol declares; the configurable switches are .*\bsupportsDeveloperRole\b/)
  })

  it('refuses compat keys pi-ai’s catalog owns, pointing at the catalog route', () => {
    for (const compat of [{ openRouterRouting: {} }, { supportsAdditionalTools: true }]) {
      expect(() => resolveProfiles({
        'acme-gateway': {
          api: 'openai-completions',
          baseURL: 'https://acme.test',
          models: [{ id: 'acme-a', compat: compat as never }],
        },
      })).toThrow(/which is not configurable here/)
    }
  })
})

describe('Delta custom-only configurable directory', () => {
  it('contains no built-in routes before a custom API is configured', async () => {
    const ctx = await bootWithSettings({})
    expect(ctx.llm.listConfigurableProviders()).toEqual([])
    await ctx.fiber.dispose()
  })

  it('publishes only explicitly declared API routes', async () => {
    const ctx = await bootWithSettings(gateway('https://api.example.invalid/v1'))
    expect(ctx.llm.listConfigurableProviders()).toContainEqual({
      provider: 'acme-gateway',
      displayName: 'Acme Gateway',
      settingsNs: 'llm-pi-ai',
      settingsPath: ['providers', 'acme-gateway'],
      declared: true,
    })
    expect(ctx.llm.listConfigurableProviders()).toHaveLength(1)
    await ctx.fiber.dispose()
  })
})
