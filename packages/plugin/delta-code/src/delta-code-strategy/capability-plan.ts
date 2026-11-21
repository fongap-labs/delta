/** Pure, bounded planner. No model dispatch, Host permission, Skill load or task store. */
export interface DeclaredCapability {
  readonly id: string
  readonly requires?: readonly string[]
  readonly uses: readonly string[]
  readonly skills: readonly string[]
}

export interface DeclaredCapabilityPlan {
  /** Stable topological sets: simultaneous items have no required dependencies on one another. */
  readonly waves: string[][]
  /** Eligible now: only audited completed prerequisites can unlock work. */
  readonly ready: string[]
  /** Current Host-reported active tasks; never scheduled a second time. */
  readonly active: string[]
  /** Tasks that cannot run until external active or unavailable dependencies finish. */
  readonly blocked: string[]
  /** Effective target + required dependencies, never optional `uses`. */
  readonly required: string[]
  /** Optional links only; neither scheduled nor loaded. */
  readonly suggested: string[]
  /** Distinct relevant Skill names for currently ready tasks, not an installation command. */
  readonly skills: string[]
}

/** Host provides *verified* checkpoint IDs; this planner must not infer execution success. */
export function planDeclaredCapabilities(
  catalog: readonly DeclaredCapability[],
  requested: readonly string[],
  completed: readonly string[] = [],
  active: readonly string[] = [],
): DeclaredCapabilityPlan {
  if (!Array.isArray(catalog) || catalog.length < 1 || catalog.length > 64 ||
      !Array.isArray(requested) || requested.length < 1 || requested.length > 8 ||
      !Array.isArray(completed) || completed.length > 64 ||
      !Array.isArray(active) || active.length > 64) {
    throw Error('Invalid bounded capability execution plan')
  }
  const byId = new Map(catalog.map(item => [item.id, item] as const))
  if (new Set(byId.keys()).size !== catalog.length) throw Error('Duplicate capability identifier')
  const check = (values: readonly string[], label: string) => {
    if (values.some(value => typeof value !== 'string' || !byId.has(value)) ||
        new Set(values).size !== values.length) {
      throw Error('Invalid or duplicate ' + label + ' capability IDs')
    }
  }
  check(requested, 'requested')
  check(completed, 'completed')
  check(active, 'active')
  if (completed.some(id => active.includes(id))) {
    throw Error('A capability cannot be both completed and active')
  }
  const closure = new Set<string>()
  const visiting = new Set<string>()
  const visit = (id: string): void => {
    if (visiting.has(id)) throw Error('Cyclic required capability dependency: ' + id)
    if (closure.has(id)) return
    visiting.add(id)
    const item = byId.get(id)!
    const deps = item.requires ?? []
    if (!Array.isArray(deps) || deps.length > 16 || new Set(deps).size !== deps.length) {
      throw Error('Invalid capability requires: ' + id)
    }
    for (const dependency of deps) {
      if (!byId.has(dependency)) throw Error('Unknown required capability: ' + dependency)
      visit(dependency)
    }
    visiting.delete(id)
    closure.add(id)
  }
  for (const id of requested) visit(id)
  if (completed.some(id => !closure.has(id)) || active.some(id => !closure.has(id))) {
    throw Error('Checkpoint capability outside requested dependency closure')
  }
  const proven = new Set(completed)
  const busy = new Set(active)
  // A claimed finished or running task cannot skip unverified required predecessors.
  for (const id of [...completed, ...active]) {
    if ((byId.get(id)!.requires ?? []).some((dep: string) => !proven.has(dep))) {
      throw Error('Host checkpoint lacks completed required prerequisites: ' + id)
    }
  }
  const remaining = new Set([...closure].filter(id => !proven.has(id) && !busy.has(id)))
  const ready = [...remaining].filter(id =>
    (byId.get(id)!.requires ?? []).every((dep: string) => proven.has(dep)))
  const waves: string[][] = []
  const scheduled = new Set(proven)
  while (remaining.size) {
    const wave = [...remaining].filter(id =>
      (byId.get(id)!.requires ?? []).every((dep: string) => scheduled.has(dep)))
    if (!wave.length) break // Host-in-progress dependencies block, rather than simulating completion
    waves.push(wave)
    for (const id of wave) {
      remaining.delete(id)
      scheduled.add(id)
    }
  }
  const required = [...closure]
  const suggested = [...new Set(requested.flatMap(id => byId.get(id)!.uses))]
    .filter(id => !closure.has(id))
  return {
    waves, ready, active: [...active], blocked: [...remaining],
    required, suggested,
    skills: [...new Set(ready.flatMap(id => byId.get(id)!.skills))],
  }
}
