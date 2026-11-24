import { makeTranslate } from '@deepseek-ai/dsh-client-test-runtime'
import type { CommandDescriptor } from '@deepseek-ai/dsh-commands/types'
import { expect, it } from 'vitest'
import { builtinRowFace } from '../src/client/presentation.ts'
import { en, zh } from '../src/client/locales.ts'

it('translates only the Delta-owned worktree command face', () => {
  const command = { name: 'worktree', description: 'Create or switch an isolated Git worktree', definitionId: '@delta/worktree' } as CommandDescriptor
  expect(builtinRowFace(command, makeTranslate(zh))?.label).toBe('工作树')
  expect(builtinRowFace(command, makeTranslate(en))?.label).toBe('Worktree')
  expect(builtinRowFace({ ...command, definitionId: undefined }, makeTranslate(zh))).toBeUndefined()
})

it('translates the Delta Forge command without changing its machine identity', () => {
  const command = { name: 'forge', description: 'Read a change request', definitionId: '@delta/forge' } as CommandDescriptor
  expect(builtinRowFace(command, makeTranslate(zh))?.label).toBe('代码托管')
  expect(builtinRowFace(command, makeTranslate(en))?.label).toBe('Forge')
})
