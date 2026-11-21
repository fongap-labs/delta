import type { WorkflowSkill } from './workflow-skills.js'

/** Focused procedures for performance, runtime, visual and Skill behavior work. */
export const SPECIALIST_WORKFLOW_SKILLS: readonly WorkflowSkill[] = [
  {
    name: 'code-perf-issue',
    title: 'Measure and fix performance problems',
    description: 'Use for a reported slowdown, high resource use or performance regression; establish a baseline and prove the change against it.',
    instructions: [
      'Identify the affected user-visible operation, environment, representative input and target metric before changing code. Separate a measured problem from a hunch.',
      'Reproduce the issue and capture a repeatable baseline with the existing profiler, benchmark or timing path. Record warmup, sample count, variance and relevant machine/runtime conditions.',
      'Trace the measured bottleneck to its cause using a profile or other runtime evidence. Do not optimize unrelated code or infer a hot path from source shape alone.',
      'State one falsifiable optimization hypothesis and change the smallest causal area. Preserve correctness and compare against the same workload and conditions.',
      'Repeat the measurement enough to distinguish a real change from noise; report absolute values, relative change, sample conditions and any tradeoffs. Do not claim a speedup when the baseline or measurement is inconclusive.',
      'Run behavior tests and inspect the final diff. If the regression cannot be reproduced or the profiler is unavailable, report that limit and do not present an unmeasured change as a performance fix.',
    ],
  },
  {
    name: 'code-runtime-forensics',
    title: 'Investigate runtime symptoms and trace artifacts',
    description: 'Use for live hangs, crashes, leaks or resource spikes, and for analyzing CPU profiles, traces or heap snapshots.',
    instructions: [
      'Classify the evidence first: a live symptom needs a controlled reproduction and instrumentation; a captured profile or trace needs artifact-specific inspection. Do not claim access to artifacts that were not supplied.',
      'Preserve the failing environment and record the trigger, workload, runtime version and time window. Reproduce before changing code when possible; distinguish product behavior from tooling or environment failure.',
      'For live symptoms, add the least invasive temporary instrumentation that can distinguish competing causes, observe state while the issue occurs, then remove or justify any instrumentation kept.',
      'For a CPU profile, trace or heap snapshot, identify capture settings and likely sampling limits; follow the dominant path or retaining references back to their owner and confirm with a second observation.',
      'Redact credentials, personal data, request payloads and unrelated process details before sharing logs or artifacts. Do not upload sensitive artifacts to external services without explicit authorization.',
      'Form and test one causal explanation at a time. Verify the fix against the original trigger or a focused equivalent, and state clearly when evidence is incomplete or from a different surface.',
    ],
  },
  {
    name: 'code-visual-parity',
    title: 'Compare an interface against a reference',
    description: 'Use when an interface must match a reference screenshot, design or existing implementation across layout and interaction states.',
    instructions: [
      'Identify the reference and target surface, viewport, device scale, theme, route and interaction state. Capture both under matching conditions using available Host UI tools.',
      'Compare the actual rendered result, not source code alone. Note concrete mismatches in geometry, typography, color, spacing, assets, responsive behavior and interaction states.',
      'Fix the largest visible mismatch first, then capture again. Keep each iteration small so the visual effect can be attributed to the change.',
      'Check relevant states such as loading, empty, error, hover, focus and narrow viewport when they are in scope; verify keyboard access and reduced-motion behavior where applicable.',
      'Do not invent screenshots or report pixel-level parity from an unrendered mock. If browser or screenshot access is unavailable, provide a code-based review and mark visual verification as unrun.',
      'Finish with evidence from the actual target surface and inspect the diff for unrelated visual or behavior changes.',
    ],
  },
  {
    name: 'code-skill-evaluation',
    title: 'Evaluate a Skill or prompt change',
    description: 'Use when changing a Skill or agent prompt and you need evidence that behavior improved without regressions.',
    instructions: [
      'Define the behavior change and a small set of representative user tasks before editing. Include ordinary cases, edge cases and at least one prompt that tempts the old failure.',
      'Write a short rubric with observable criteria such as correctness, required evidence, safety boundaries and output usefulness. Keep the rubric hidden from candidate runs when a blinded comparison is feasible.',
      'Run the same prompts against the previous and proposed Skill under comparable model, tool and project conditions. Label outputs neutrally; do not leak the rubric or variant identity to candidate agents.',
      'Have a separate reviewer or model score anonymized outputs against the rubric when available. Otherwise score them yourself and disclose that the comparison was not blind or independent.',
      'Inspect the actual work products and tool evidence, not unverified claims about following instructions. Record regressions, failures and meaningful uncertainty alongside improvements.',
      'Keep the change only when it improves target behavior without material safety or quality regressions. Bound runs, context and cost; never expose private transcripts or use hidden evaluation metadata in candidate prompts.',
    ],
  },
  {
    name: 'code-quality-strength',
    title: 'Assess meaningful test strength and maintainability',
    description: 'For higher-risk code changes, find untested behavior, misleading assertions, risky complexity and duplicated business rules.',
    instructions: [
      'Start from the changed behavior and reachable call sites. Identify concrete failure cases that existing tests should reject, not just aggregate coverage.',
      'Inspect current tests for assertions that would still pass if important behavior changed. Add or propose focused regression cases for confirmed gaps, within the authorized task scope.',
      'Use existing project-supported mutation testing or focused fault injection only when the runner and an approved project script are actually available. Never install a new tool, run arbitrary commands or create billable requests to satisfy a metric.',
      'If testing a mutation, confirm a clean passing baseline, apply a safe isolated mutation, verify the correct test fails, and restore the original source before concluding. Do not treat compilation errors or test harness failures as a meaningful killed mutation.',
      'Examine complex functions and candidate duplication only in the affected area. Confirm behavior and ownership before recommending an abstraction; do not refactor solely to manipulate scores.',
      'Read back actual test exit codes and relevant assertions. Distinguish measured results, source-only observations and checks that were not run. The mandatory CI and Host permissions always remain authoritative.',
    ],
  },
  {
    name: 'code-diagnostic-readiness',
    title: 'Preserve enough safe evidence for the next recurrence',
    description: 'After a bug or performance repair, check whether future failures can be diagnosed from existing logs and traces.',
    instructions: [
      'For this confirmed failure, ask whether a recurrence can be linked to the affected version, operation and stage using existing evidence. Reuse current logging and diagnostics before adding anything.',
      'Where the evidence is insufficient and edits are authorized, add the smallest safe stage/elapsed-time or error-category observation needed to distinguish competing causes. Do not add a logging platform, permanent debug mode or speculative metrics.',
      'Never log credentials, request bodies, private transcripts, tokens or unbounded context. Preserve user-facing concise errors and separate internal diagnostics.',
      'Run the representative path with existing permitted tools and read the produced evidence back. A log statement in source without an observed entry is not verified instrumentation.',
      'Do not simulate production incidents or background monitoring without authorization. Report missing Host access or a safely untestable failure path instead of claiming coverage.',
    ],
  },
  {
    name: 'code-experience-review',
    title: 'Propose reuse of proven engineering experience',
    description: 'Convert a repeatedly useful, verified lesson into a review candidate without automatically changing trusted Skills.',
    instructions: [
      'Consider a reusable lesson only after the target behavior was reproduced or explained, a real fix was verified and relevant commit/test evidence is available.',
      'After observing the actual source revision and CI run, use code_experience_candidate to format a bounded record of the problem, root cause, fix pattern, applicability and failure boundaries. Its pending stage is not evidence that CI passed. Redact sensitive paths, payloads and identities.',
      'Compare with existing project rules and approved Skills before proposing a new entry; avoid duplicate or one-off guidance. A historical result is a reference, not authority over the current code.',
      'Do not persist a global transcript database or change the trusted Skill catalog outside governance. When authorized by Host permissions, progress eligible free GitHub Skills through automated evidence checks, the existing PR/admission workflow and central CI without imposing an extra blanket per-Skill human confirmation. Reject unsafe or uncertain candidates; do not claim automatic admission when its checks were not run.',
      'Review or reject the candidate independently of the last task outcome; never treat a model summary or unrun check as verified evidence.',
    ],
  },
]