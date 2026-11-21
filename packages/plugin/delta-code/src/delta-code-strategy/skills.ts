import { WORKFLOW_SKILLS } from './workflow-skills.js'

interface CodingSkill {
  name: string
  title: string
  description: string
  instructions: string[]
}

// Use the Host skill registry for progressive disclosure. These are procedures,
// not replacements for the Host's editor, LSP, subagents, workflow or terminal.
export const CODING_SKILLS: readonly CodingSkill[] = [
  {
    name: 'code-fast-change',
    title: 'Fast, verified code change',
    description: 'Small, well-scoped coding task: find the right files, edit minimally and verify without a heavyweight plan.',
    instructions: [
      'Use for a focused change with clear acceptance criteria. Do not expand it into a multi-agent workflow.',
      'Read AGENTS.md and applicable local instructions. Use code_project_profile for project checks and native fs_search/LSP for precise discovery.',
      'Inspect the affected implementation and its nearest test. State the intended behavior in one sentence.',
      'Make the smallest cohesive diff with the Host editor or code_multi_edit. Do not silently refactor unrelated code.',
      'Run the narrowest meaningful test first; if enabled, use code_verify with the narrowest declared script (usually test or typecheck). Read failures before editing again.',
      'Inspect code_git_diff for accidental changes. Report changed files, evidence and unverified items; never claim tests passed without output.',
    ],
  },
  {
    name: 'code-feature-delivery',
    title: 'Feature delivery from intent to verified PR',
    description: 'Multi-file feature or refactor requiring acceptance criteria, small implementation steps, tests and PR readiness.',
    instructions: [
      'Identify user goal, constraints, non-goals and observable acceptance tests. Ask only when a consequential ambiguity blocks safe progress.',
      'Read project rules and use code_project_profile. Prefer existing modules and Host tools before introducing a package or architectural layer.',
      'For multi-step work write an executable plan: goal, non-goals, affected files, dependencies, acceptance checks and per-task verification. Use native todo_write if mounted; avoid a second planner, task store or model call. On explicit continuation, compaction recovery or transport interruption, use native Todo and code_resume once when the Host projection and authorized Git baseline are available; reconcile HEAD, diff and prior evidence before replaying edits. Never treat a completed Todo as passed tests or invent a missing checkpoint.',
      'If configured, use code_git_worktree for isolation; otherwise respect the current branch and existing uncommitted changes.',
      'For substantial cross-module work, if DSH native subagents are verified available, request one independent read-only diff review automatically after implementation. Never invent subagent availability or delegate overlapping writes; small changes do not require a review team.',
      'For each ready task, create or adjust a failing test when practical, make the minimum change, run focused verification and inspect the diff; update Host todo_write only when that task has concrete completion evidence. Never make passing the delivery gate a self-referential Todo.',
      'Prefer native focused verification; use code_verify or code_verify_suite only with authorized Host execution and declared scripts. Use code_delivery_gate only for substantial Git-backed work with a valid HEAD and completed implementation Todos; do not call it on non-Git workspaces or when execution is denied. Replan on contradictory failures; preserve actual commands, results and diff.',
      'If GitHub writes are enabled, open a feature-branch PR. Never auto-merge, bypass CI or treat a pending check as a pass.',
    ],
  },
  {
    name: 'code-root-cause-debugging',
    title: 'Root-cause debugging',
    description: 'Reproduce a bug, trace its source and prove a minimal fix with regression evidence.',
    instructions: [
      'Capture exact symptoms, inputs, failure output and the last known good behavior before changing code.',
      'Reproduce with the smallest available command or test. If it cannot reproduce, state the uncertainty.',
      'Trace from the observed failure to the owner of the data or state. Check assumptions at each boundary; distinguish cause from symptom.',
      'Form one falsifiable hypothesis, then write a regression test that fails for the identified cause whenever feasible.',
      'Change the smallest causal location; rerun the regression and adjacent tests. Do not simultaneously apply speculative fixes.',
      'After repeated unsuccessful hypotheses, revisit the architecture and missing evidence rather than making more blind edits. Avoid rereading unchanged files or reloading an already loaded Skill without a specific evidence gap; inspect new failure output and update the causal hypothesis before retrying.',
      'Inspect final diff, report root cause and exact validation evidence. Preserve logs without copying secrets into reports.',
    ],
  },
  {
    name: 'code-review-readiness',
    title: 'Evidence-based code review',
    description: 'Review a local change or PR for correctness, safety, scope and test evidence before declaring it ready.',
    instructions: [
      'Read the intended behavior and the diff; include changes in generated files and configuration when relevant.',
      'Prioritize correctness, security, data loss, compatibility and missed edge cases over style preferences.',
      'Check caller/callee contracts, error paths, secrets, path traversal, concurrent updates and platform-specific behavior.',
      'Use native LSP references, fs_search and focused tests to validate suspected issues. Do not infer defects from a pattern match alone.',
      'Run the relevant code_verify scripts where enabled. Otherwise mark validation explicitly as not run, not passed.',
      'Report actionable findings with file locations, severity and a way to reproduce. If none, state remaining test and environment gaps.',
      'For GitHub changes, inspect check runs and preserve Action Worker gates. Do not merge or waive a red check.',
    ],
  },
]

export const ALL_CODING_SKILLS: readonly CodingSkill[] = [...CODING_SKILLS, ...WORKFLOW_SKILLS]
