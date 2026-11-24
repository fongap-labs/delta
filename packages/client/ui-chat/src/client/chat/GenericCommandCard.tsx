import { memo } from 'react'
import type { ChatViewSlotProps, CommandRowOwnerProps } from '../contract/slots.ts'
import { CommandStatusRow } from './CommandStatusRow.tsx'

/** Card props: the owner payload plus the render site's locale seat (plain prop). */
export interface GenericCommandCardProps extends CommandRowOwnerProps {
  t: ChatViewSlotProps['t']
  /** Command-specific running copy; absent uses the generic command label. */
  runningSummary?: string | undefined
}

/**
 * Render a command summary and its lazily mounted multiline output.
 * @param props - command, locale, and optional running label.
 * @returns the command disclosure.
 */
export const GenericCommandCard = memo(function GenericCommandCard({ node, t, runningSummary }: GenericCommandCardProps) {
  const text = node.outcome?.text
  const worktree = node.name === 'worktree'
  const forge = node.name === 'forge'
  let forgeSummary: string | undefined
  let forgeBody: string | undefined
  if (forge && node.outcome?.kind === 'success' && text !== undefined) {
    try {
      const data: unknown = JSON.parse(text)
      if (typeof data === 'object' && data !== null && 'title' in data && typeof data.title === 'string'
        && 'project' in data && typeof data.project === 'string' && 'number' in data && typeof data.number === 'number'
        && 'state' in data && typeof data.state === 'string' && 'sourceBranch' in data && typeof data.sourceBranch === 'string'
        && 'targetBranch' in data && typeof data.targetBranch === 'string' && 'headSha' in data && typeof data.headSha === 'string'
        && 'description' in data && typeof data.description === 'string') {
        const state = data.state === 'open' || data.state === 'opened' ? t('command.forge.open')
          : data.state === 'closed' ? t('command.forge.closed') : data.state === 'merged' ? t('command.forge.merged') : data.state
        forgeSummary = `${data.project} #${String(data.number)} · ${data.title}`
        forgeBody = `${t('command.forge.state')}: ${state}\n${t('command.forge.source')}: ${data.sourceBranch}\n${t('command.forge.target')}: ${data.targetBranch}\n${t('command.forge.revision')}: ${data.headSha}\n\n${data.description}`
      }
    } catch { /* Keep unknown or historical command text unchanged. */ }
  }
  const pathPrefix = 'Working directory: '
  let localizedText = forgeSummary ?? text
  if (forge && text === 'Usage: /forge github|gitlab <owner/project> <number>') localizedText = t('command.forge.usage')
  else if (forge && node.outcome?.kind === 'error' && text !== undefined) localizedText = t('command.forge.error')
  else if (worktree && text?.startsWith(pathPrefix)) localizedText = t('command.worktree.directory', { path: text.slice(pathPrefix.length) })
  else if (worktree && text === 'Usage: /worktree create <task> | enter <task> | leave') localizedText = t('command.worktree.usage')
  else if (worktree && node.outcome?.kind === 'error' && text !== undefined) localizedText = t('command.worktree.error')
  const summary = node.outcome === null
    ? runningSummary ?? t('command.running')
    : localizedText ?? (node.outcome.kind === 'error' ? t('command.failed') : t('command.done'))
  // The summary already carries the settlement text, so the title is the bare
  // command name.
  const title = node.name ?? t('command.title')
  const state = node.outcome === null ? 'running' : node.outcome.kind === 'error' ? 'error' : 'ok'
  const body = forgeBody ?? (text !== undefined && (text.includes('\n') || ((worktree || forge) && node.outcome?.kind === 'error' && localizedText !== text)) ? text : null)
  return <CommandStatusRow title={title} state={state} summary={summary} body={body} t={t} />
})
