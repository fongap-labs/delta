import type { Context } from '@deepseek-ai/cordis'
import { isCodingTaskContinuation } from '../../../delta-code-strategy/task-routing.js'
import { isExternalSkillOptOut } from '../../../delta-code-strategy/trusted-github-skills.js'

interface ScopedSkillLookup {
  readonly scope?: unknown
}

interface ClaimedUserMessage {
  readonly agent?: object
  readonly message?: {
    readonly source?: { readonly kind?: string }
    readonly content?: readonly { readonly type?: string; readonly text?: string }[]
  }
}

interface AgentLifecycleEvent {
  readonly agent?: object
}

interface TaskState {
  readonly controller: AbortController
  readonly isExternalDisabled: boolean
}

/**
 * The official skill loader borrows SkillViewOptions (including scope) when it
 * invokes the provider. Scope is optional at the provider API boundary: without
 * it, the Host's call signal and registration signal remain authoritative.
 *
 * This does not persist or mutate previously emitted Skill tool messages.
 */
export function createTaskSkillCancellation(ctx: Context, invalidate: () => void): {
  readonly lookup: (options?: ScopedSkillLookup) => { signal?: AbortSignal; isExternalDisabled: boolean }
  readonly dispose: () => void
} {
  const states = new WeakMap<object, TaskState>()
  // A disposed Agent must never be re-created by a late Host Skill request.
  const disposedAgents = new WeakSet<object>()
  const activeControllers = new Set<AbortController>()
  const disposers: Array<() => void> = []
  let isActive = true

  const runtime = ctx as unknown as {
    on?: (name: string, handler: (event: unknown) => void) => (() => void) | void
  }

  function revoke(agent: object, reason: string): void {
    const previous = states.get(agent)
    if (!previous) return
    states.delete(agent)
    activeControllers.delete(previous.controller)
    previous.controller.abort(new Error(reason))
  }

  function stateFor(agent: object): TaskState {
    const existing = states.get(agent)
    if (existing) return existing
    const created = { controller: new AbortController(), isExternalDisabled: false }
    activeControllers.add(created.controller)
    states.set(agent, created)
    return created
  }

  function subscribe(name: string, handler: (event: unknown) => void): void {
    const unsubscribe = runtime.on?.(name, handler)
    if (typeof unsubscribe === 'function') disposers.push(unsubscribe)
  }

  subscribe('agent/inbox/claimed', payload => {
    if (!isActive || !payload || typeof payload !== 'object') return
    const { agent, message } = payload as ClaimedUserMessage
    if (!agent || disposedAgents.has(agent) || message?.source?.kind !== 'user') return
    const text = Array.isArray(message.content)
      ? message.content.filter(part => part.type === 'text' && typeof part.text === 'string')
        .map(part => part.text).join('\n')
      : ''
    // Continuations retain the current task until the user explicitly opts out.
    const previous = states.get(agent)
    const isContinuation = isCodingTaskContinuation(text)
    const isDenied = isExternalSkillOptOut(text)
    if (isContinuation && previous && !isDenied) return
    const wasDenied = previous?.isExternalDisabled ?? false
    revoke(agent, isDenied ? 'external Skill use denied by user' : 'skill task superseded')
    const next = stateFor(agent)
    if (isDenied) {
      states.set(agent, { ...next, isExternalDisabled: true })
      next.controller.abort(new Error('external Skill use denied by user'))
      activeControllers.delete(next.controller)
    }
    if (wasDenied !== isDenied) invalidate()
  })
  subscribe('agent/disposed', payload => {
    if (!isActive || !payload || typeof payload !== 'object') return
    const { agent } = payload as AgentLifecycleEvent
    if (!agent) return
    disposedAgents.add(agent)
    const wasDenied = states.get(agent)?.isExternalDisabled ?? false
    revoke(agent, 'skill agent disposed')
    if (wasDenied) invalidate()
  })

  return {
    lookup(options) {
      const scope = options?.scope
      if (!isActive || !scope || typeof scope !== 'object') {
        return { isExternalDisabled: !isActive }
      }
      if (disposedAgents.has(scope)) return { isExternalDisabled: true }
      const state = stateFor(scope)
      return { signal: state.controller.signal, isExternalDisabled: state.isExternalDisabled }
    },
    dispose() {
      if (!isActive) return
      isActive = false
      for (const unsubscribe of disposers.splice(0).reverse()) unsubscribe()
      for (const controller of activeControllers) controller.abort(new Error('skill plugin disposed'))
      activeControllers.clear()
    },
  }
}
