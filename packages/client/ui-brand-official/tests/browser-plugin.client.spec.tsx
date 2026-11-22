// @vitest-environment jsdom
import { Context } from '@deepseek-ai/cordis'
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, render } from '@testing-library/react'
import { SlotRegistry } from '@deepseek-ai/dsh-client-ui-renderer/client'
import { apply, inject } from '../src/client/index.ts'
import { OfficialBrandMark, OfficialBrandName } from '../src/client/Brand.tsx'
import { apply as hostApply } from '../src/index.ts'

afterEach(cleanup)

const BRAND_HOLES = ['sidebar.brand.mark', 'sidebar.brand.name'] as const

async function bench(declare = true) {
  const ctx = new Context()
  await ctx.plugin(SlotRegistry).await()
  const slots = ctx.get('slots') as SlotRegistry
  const declareHoles = () => slots.register({
    name: 'root',
    children: Object.fromEntries([...BRAND_HOLES, 'conversation.hero.frame', 'conversation.hero.brand.mark'].map(name => [name, {
      kind: name === 'conversation.hero.frame' ? 'hoc' : 'single',
      scope: 'root',
    }])),
  } as never, () => null)
  const disposeHoles = declare ? declareHoles() : undefined
  return { ctx, slots, declareHoles, disposeHoles }
}

describe('Delta sidebar-brand plugin', () => {
  it('keeps the Host entry inert and declares only slots', () => {
    expect(hostApply).not.toThrow()
    expect(inject).toEqual(['slots'])
  })

  it('mounts only the Delta sidebar slots and never a home decoration', async () => {
    const subject = await bench()
    const fiber = subject.ctx.plugin({ inject: [...inject], apply })
    await fiber.await()
    for (const hole of BRAND_HOLES) expect(subject.slots.entries(hole)).toHaveLength(1)
    expect(subject.slots.entries('conversation.hero.frame')).toHaveLength(0)
    expect(subject.slots.entries('conversation.hero.brand.mark')).toHaveLength(0)
    await fiber.dispose()
    for (const hole of BRAND_HOLES) expect(subject.slots.entries(hole)).toHaveLength(0)
  })

  it('restores both brand slots after their declarations are replaced', async () => {
    const subject = await bench()
    const fiber = subject.ctx.plugin({ inject: [...inject], apply })
    await fiber.await()
    subject.disposeHoles?.()
    for (const hole of BRAND_HOLES) expect(subject.slots.entries(hole)).toHaveLength(0)
    subject.declareHoles()
    await Promise.resolve()
    for (const hole of BRAND_HOLES) expect(subject.slots.entries(hole)).toHaveLength(1)
    await fiber.dispose()
  })

  it('registers slots when declarations arrive after the brand plugin', async () => {
    const subject = await bench(false)
    const fiber = subject.ctx.plugin({ inject: [...inject], apply })
    await fiber.await()
    subject.declareHoles()
    await Promise.resolve()
    for (const hole of BRAND_HOLES) expect(subject.slots.entries(hole)).toHaveLength(1)
    await fiber.dispose()
  })

  it('renders the Delta name and the mark at requested sizes', () => {
    const name = render(<OfficialBrandName />)
    expect(name.container.textContent).toBe('Delta')
    name.unmount()
    const mark = render(<OfficialBrandMark size={34} />)
    expect(mark.container.querySelector('img')?.getAttribute('alt')).toBe('Delta')
    expect(mark.container.querySelector('img')?.getAttribute('width')).toBe('34')
    mark.rerender(<OfficialBrandMark size={24} />)
    expect(mark.container.querySelector('img')?.getAttribute('width')).toBe('24')
  })
})
