// @vitest-environment jsdom
import { render, screen, fireEvent } from '@testing-library/react'
import { makeTranslate } from '@deepseek-ai/dsh-client-test-runtime'
import { en as commonEn } from '@deepseek-ai/dsh-client-locale/src/locales/en.ts'
import { zh as commonZh } from '@deepseek-ai/dsh-client-locale/src/locales/zh.ts'
import type { CommandNode } from '@deepseek-ai/dsh-client-ui-conversation/client'
import { expect, it } from 'vitest'
import { GenericCommandCard } from '../src/client/chat/GenericCommandCard.tsx'
import { en, zh } from '../src/client/locale.ts'

it('renders a recorded worktree result in the active application language', () => {
  const path = 'D:\\delta\\task'
  const node = { kind: 'command', seq: 1, time: 1, commandId: 'a', name: 'worktree', args: ' create task', outcome: { kind: 'success', text: `Working directory: ${path}` } } as CommandNode
  const view = render(<GenericCommandCard node={node} t={makeTranslate(zh, commonZh)} />)
  expect(screen.getByText(zh['command.worktree.directory'].replace('{path}', path))).toBeTruthy()
  view.rerender(<GenericCommandCard node={node} t={makeTranslate(en, commonEn)} />)
  expect(screen.getByText(en['command.worktree.directory'].replace('{path}', path))).toBeTruthy()
  expect(node.outcome?.text).toBe(`Working directory: ${path}`)
})

it('localizes the failure summary while preserving diagnostic details', () => {
  const node = { kind: 'command', seq: 1, time: 1, commandId: 'a', name: 'worktree', args: '', outcome: { kind: 'error', text: 'Error: target already exists' } } as CommandNode
  render(<GenericCommandCard node={node} t={makeTranslate(zh, commonZh)} />)
  expect(screen.getByText(zh['command.worktree.error'])).toBeTruthy()
  fireEvent.click(screen.getByText(zh['command.worktree.error']))
  expect(screen.getByText('Error: target already exists')).toBeTruthy()
})

it('renders a Forge result using the current application language without translating source text', () => {
  const value = { provider: 'github', project: 'owner/repo', number: 12, title: 'Original title', state: 'open', sourceBranch: 'feature', targetBranch: 'main', headSha: 'abc', description: 'Original content' }
  const node = { kind: 'command', seq: 2, time: 2, commandId: 'b', name: 'forge', args: null, outcome: { kind: 'success', text: JSON.stringify(value) } } as CommandNode
  const view = render(<GenericCommandCard node={node} t={makeTranslate(zh, commonZh)} />)
  fireEvent.click(screen.getByText('owner/repo #12 · Original title'))
  expect(screen.getByText(new RegExp(`${zh['command.forge.state']}: ${zh['command.forge.open']}`))).toBeTruthy()
  expect(screen.getByText(/Original content/)).toBeTruthy()
  view.rerender(<GenericCommandCard node={node} t={makeTranslate(en, commonEn)} />)
  expect(screen.getByText(/State: Open/)).toBeTruthy()
  expect(screen.getByText(/Original content/)).toBeTruthy()
})
