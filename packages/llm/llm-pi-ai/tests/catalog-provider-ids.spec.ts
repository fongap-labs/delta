import { describe, expect, it } from 'vitest'
import { catalogModels, catalogProvider, catalogProviderIds } from '../src/catalog.ts'
import { supportedProtocols } from '../src/provider.ts'

describe('Delta custom-only model routes', () => {
  it('preserves API-compatible transports without loading a vendor provider catalog', () => {
    expect(supportedProtocols()).toEqual([
      'openai-completions',
      'openai-responses',
      'anthropic-messages',
      'google-generative-ai',
    ])
  })

  it('never exposes third-party providers or models from the installed pi-ai catalog', () => {
    expect(catalogProviderIds()).toEqual([])
    for (const provider of ['openai', 'anthropic', 'google', 'deepseek', 'custom']) {
      expect(catalogProvider(provider)).toBeUndefined()
      expect(catalogModels(provider).size).toBe(0)
    }
  })
})
