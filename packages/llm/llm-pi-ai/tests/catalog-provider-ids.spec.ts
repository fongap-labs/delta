import { describe, expect, it } from 'vitest'
import { catalogModels, catalogProvider, catalogProviderIds } from '../src/catalog.ts'

describe('Delta custom-only model routes', () => {
  it('never exposes third-party providers or models from the installed pi-ai catalog', () => {
    expect(catalogProviderIds()).toEqual([])
    for (const provider of ['openai', 'anthropic', 'google', 'deepseek', 'custom']) {
      expect(catalogProvider(provider)).toBeUndefined()
      expect(catalogModels(provider).size).toBe(0)
    }
  })
})
