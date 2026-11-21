import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import { defineTool } from '@fongap-labs/delta-plugin-shared'
import type { Settings } from '../../../types.js'
import { isAllowedVerificationScript, selectImpactVerification, type VerificationScript } from '../../../delta-code-quality/verify-presets.js'
import { prepareVerify, runPreparedVerify, type PreparedVerification } from './verify.js'
import { classifyFailure, classifyVerificationEvidence, verificationNextAction } from '../../../delta-code-quality/failure-policy.js'

const STEP_SCHEMA = {
  type: 'object' as const,
  additionalProperties: false,
  properties: {
    script: { type: 'string' as const, required: true },
    project: { type: 'string' as const, required: true },
    command: { type: 'string' as const, required: true },
    exitCode: { type: 'integer' as const, required: true },
    signal: { type: 'string' as const, required: true },
    outcome: { type: 'string' as const, required: true, enum: ['succeeded', 'failed', 'uncertain'] },
    passed: { type: 'boolean' as const, required: true },
    timedOut: { type: 'boolean' as const, required: true },
    output: { type: 'string' as const, required: true },
    truncated: { type: 'boolean' as const, required: true },
    failureKind: { type: 'string' as const, required: true },
  },
} as const

/** Bounded sequential execution; every requested step is checked before the first spawn. */
export function createVerifySuiteTool(ctx: Context, settings: Settings): ToolDefinition {
  return defineTool({
    name: 'code_verify_suite',
    description: 'Prevalidate 1–5 distinct scripts, then execute sequentially in the DSH Host. Optional changedPaths selects a focused advisory subset; full delivery verification remains required. Stop on first failure.',
    parameters: {
      scripts: {
        type: 'array', required: true,
        description: '1..5 distinct allowlisted base checks or declared Node variants such as test:unit.',
        items: { type: 'string' },
      },
      changedPaths: { type: 'array', description: 'Optional 1..64 repository-relative changed paths supplied after inspecting Git; selects a focused subset of scripts, never proves completion.',
        items: { type: 'string' } },
      cwd: { type: 'string', description: 'Optional workspace-relative directory.' },
      project: { type: 'string', enum: ['auto', 'node', 'rust', 'python', 'go'], description: 'Project type; auto requires exactly one manifest type.' },
      timeoutMs: { type: 'integer', description: 'Per-step timeout, 10000..600000 ms.' },
    },
    output: {
      schema: { type: 'object', additionalProperties: false, properties: {
        passed: { type: 'boolean', required: true },
        completed: { type: 'integer', required: true },
        skipped: { type: 'array', required: true, items: { type: 'string' } },
        nextAction: { type: 'string', required: true },
        scope: { type: 'string', required: true, enum: ['full', 'focused'] },
        deferred: { type: 'array', required: true, items: { type: 'string' } },
        selectionReason: { type: 'string', required: true },
        results: { type: 'array', required: true, items: STEP_SCHEMA },
      } },
      render(_args, value) {
        return [{ type: 'text', text: [
          'Verify suite (' + value.scope + '): ' + (value.passed ? 'PASS' : 'FAIL') + ', ' + value.completed + ' run, ' + value.skipped.length + ' skipped.',
          ...(value.deferred.length ? ['Deferred until complete verification: ' + value.deferred.join(', ')] : []),
          ...value.results.map(step => step.command + ': ' + (step.passed ? 'PASS' : 'FAIL') + ' (' + step.exitCode + ', ' + step.outcome + (step.signal ? ', signal ' + step.signal : '') + ')\n' + step.output),
          ...(value.skipped.length ? ['Skipped: ' + value.skipped.join(', ')] : []),
          'Next: ' + value.nextAction,
        ].join('\n') }]
      },
    },
    async execute(args, exec) {
      const scripts = args.scripts as VerificationScript[]
      if (!Array.isArray(scripts) || scripts.length < 1 || scripts.length > 5) {
        throw new Error('verification suite requires 1..5 scripts')
      }
      if (new Set(scripts).size !== scripts.length) throw new Error('verification scripts must not be duplicated')
      if (scripts.some(script => !isAllowedVerificationScript(script))) throw new Error('script is not allowlisted')
      const selection = selectImpactVerification(scripts, args.changedPaths)
      // Validate even deferred candidates before any command. A focused pass
      // is never authorization to run undeclared scripts or bypass Host FS.
      // No command may run if a later script is undeclared or the workspace is unsafe.
      const prepared: PreparedVerification[] = []
      for (const script of scripts) {
        if (exec.signal.aborted) throw new Error('verification cancelled')
        // The first check probes project manifests/lockfiles/package scripts.
        // Later checks reuse that snapshot while still reauthorizing the Host
        // cwd and checking processPath for each planned script.
        prepared.push(await prepareVerify(ctx, settings, exec, {
          script, cwd: args.cwd, project: args.project, timeoutMs: args.timeoutMs,
        }, prepared[0]?.probe))
      }
      // Every step must target the same Host execution filesystem.
      // A changing processPath during preflight is not safe to execute.
      const executionCwd = prepared[0]!.cwd
      if (prepared.some(step => step.cwd !== executionCwd)) {
        throw new Error('Host verification execution directory changed during preflight')
      }
      const selected = new Set(selection.selected)
      const executionSteps = prepared.filter(step => selected.has(step.script))
      const results = []
      for (const step of executionSteps) {
        if (exec.signal.aborted) throw new Error('verification cancelled')
        try {
          const evidence = await runPreparedVerify(ctx, settings, exec, step)
          const failureKind = classifyVerificationEvidence(evidence)
          results.push({ ...evidence, failureKind })
          if (!evidence.passed) break
        } catch (error) {
          if (exec.signal.aborted) throw error
          results.push({
            script: step.script, project: step.plan.project,
            failureKind: classifyFailure(error),
            command: [step.plan.command, ...step.plan.args].join(' '),
            exitCode: -1, signal: '', outcome: 'uncertain' as const, passed: false, timedOut: false, truncated: false,
            output: 'Host subprocess refused or failed; inspect Host logs.',
          })
          break
        }
      }
      const passed = results.length === executionSteps.length && results.every(result => result.passed)
      const failure = results.find(result => !result.passed)
      const nextAction = passed && selection.scope === 'focused'
        ? 'Focused checks passed; complete verification and delivery gate remain required before merge or release.'
        : verificationNextAction(failure?.failureKind ?? 'none')
      return {
        passed, completed: results.length,
        skipped: executionSteps.slice(results.length).map(step => step.script),
        deferred: selection.deferred, scope: selection.scope,
        selectionReason: selection.reason, nextAction, results,
      }
    },
  })
}
