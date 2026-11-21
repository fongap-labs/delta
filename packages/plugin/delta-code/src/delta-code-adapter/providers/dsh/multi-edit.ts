import { createHash } from 'node:crypto'
import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import { defineTool, SandboxViolationError, resolveInside } from '@fongap-labs/delta-plugin-shared'
import type { EditResult, Settings } from '../../../types.js'
import { TOOL_PREFIX, resolveRoot, readTextSafe, writeTextSafe, validateWriteTargetSafe, errorMessage } from '../../../runtime.js'
import { lookupDshHostFs } from './host-adapter.js'
import type { HostFsCapabilities } from './host-fs.js'
import type { GuardedWriteIntent } from '../../../runtime.js'
import { planTextEdit, applyTextEdit, assertEditableText } from '../../../delta-code-coding/edit-transform.js'
import { shouldPreflightMultiEdit, canCompensateMultiEdit } from '../../../delta-code-coding/multi-edit-policy.js'

export function createMultiEditTool(ctx: Context, settings: Settings): ToolDefinition {
  return defineTool({
    name: `${TOOL_PREFIX}multi_edit`,
    description:
      'Apply workspace edits with per-edit results (not a filesystem transaction). Multi-file batches are preflight-validated by default, so invalid later edits cannot trigger earlier writes. Set preflight=false only for explicit legacy best-effort behavior. Compensation remains opt-in and requires guarded-intent Host FS. dryRun does not write.',
    parameters: {
      edits: {
        type: 'array',
        required: true,
        description:
          'Edits to apply, each an object with `path` plus either `content` or `find`/`replace`.',
        items: {
          type: 'object',
          // The edit objects are validated per property; unlisted keys stay ignored.
          additionalProperties: true,
          properties: {
            path: { type: 'string', required: true, description: 'Workspace-relative file path.' },
            content: { type: 'string', description: 'Full replacement content for the file.' },
            find: { type: 'string', description: 'Literal substring to locate (used with `replace`).' },
            replace: { type: 'string', description: 'Replacement text for the located substring.' },
            expectedSha256: { type: 'string', description: 'Optional SHA-256 of the current UTF-8 file content; requires active preflight (automatic for multi-file batches).' },
          },
        },
      },
      dryRun: { type: 'boolean', description: 'When true, validate without writing.' },
      preflight: { type: 'boolean', description: 'Check the entire batch before writing; enabled by default for more than one edit. Explicit false selects legacy best-effort semantics. Preflight does not guarantee atomic writes.' },
      rollbackOnFailure: { type: 'boolean', description: 'Opt-in guarded compensation for existing files only; requires preflight (default for multi-file batches) and execution.fsWriteContract=guarded-intent. Restores confirmed earlier writes only if the Host write version is unchanged. Not atomic.' },
    },
    output: {
      schema: {
        type: 'object',
        additionalProperties: false,
        properties: {
          applied: { type: 'integer', required: true, description: 'Number of edits applied.' },
          results: {
            type: 'array',
            required: true,
            description: 'Per-edit outcome list.',
            items: {
              type: 'object',
              additionalProperties: false,
              properties: {
                path: { type: 'string', required: true },
                action: { type: 'string', required: true, enum: ['overwrite', 'replace', 'skipped'] },
                ok: { type: 'boolean', required: true },
                note: { type: 'string' },
              },
            },
          },
          dryRun: { type: 'boolean' },
          rollback: {
            type: 'object',
            additionalProperties: false,
            properties: {
              attempted: { type: 'integer', required: true },
              restored: { type: 'integer', required: true },
              failed: { type: 'integer', required: true },
              skippedReason: { type: 'string' },
            },
          },
        },
      },
      render(_args, value) {
        const lines = [
          `MultiEdit: ${value.applied} edit(s)${value.dryRun ? ' (dry run)' : ''} applied.`,
        ]
        if (value.rollback) {
          lines.push(`  Recovery: ${value.rollback.restored}/${value.rollback.attempted} restored; ${value.rollback.failed} failed.`)
          if (value.rollback.skippedReason) lines.push(`  Recovery skipped: ${value.rollback.skippedReason}`)
        }
        for (const result of value.results) {
          const flag = result.ok ? 'ok' : 'fail'
          lines.push(
            `  [${flag}] ${result.action} ${result.path}${result.note ? ` — ${result.note}` : ''}`,
          )
        }
        return [{ type: 'text', text: lines.join('\n') }]
      },
    },
    async execute(args, exec) {
      const root = await resolveRoot(ctx, exec, settings)
      const results: EditResult[] = []
      let applied = 0
      const edits = Array.isArray(args.edits) ? args.edits : []
      // Invalid later edits should not mutate earlier files by default.
      // Preserve explicit legacy opt-out and single-edit behavior.
      const usePreflight = shouldPreflightMultiEdit(args.preflight, edits.length)
      const shouldRollback = args.rollbackOnFailure === true
      if (shouldRollback && !canCompensateMultiEdit(usePreflight, Boolean(args.dryRun), settings.fsWriteContract)) {
        return {
          applied: 0,
          results: edits.map((edit) => ({
            path: typeof edit?.path === 'string' ? edit.path : '',
            action: 'skipped' as const,
            ok: false,
            note: 'rollbackOnFailure requires active preflight, dryRun=false and guarded-intent Host FS',
          })),
          dryRun: Boolean(args.dryRun),
        }
      }
      if (usePreflight || args.dryRun) {
        // Every dry run must validate targets and Host permissions before claiming success.
        // Multi-file writes are preflighted unless the caller explicitly opts out.
        const prepared: Array<{
          path: string
          action: EditResult['action']
          content: string
          note: string
          original?: string
          pinnedIntent?: GuardedWriteIntent
        }> = []
        const checked: EditResult[] = []
        const targets = new Set<string>()
        // A Host can map two lexical paths (case-folding, aliases or
        // symlinks) to the same canonical file. Keep opaque targets and use
        // bidirectional Host containment to reject duplicate identities.
        const hostIdentities: unknown[] = []
        let isInvalid = false
        for (const edit of edits) {
          const path = typeof edit?.path === 'string' ? edit.path : ''
          try {
            if (exec.signal.aborted) throw new Error('operation cancelled')
            const target = resolveInside(root, path)
            if (targets.has(target)) throw new Error('duplicate edit target in preflight batch')
            targets.add(target)
            // Check symlinks and Host FS permissions before any batch write.
            const pinnedIntent = await validateWriteTargetSafe(ctx, exec, root, path, settings)
            const hostFs = lookupDshHostFs(ctx) as HostFsCapabilities | undefined
            if (hostFs && typeof hostFs.resolve === 'function' &&
                typeof hostFs.contains === 'function') {
              const canonicalTarget = await hostFs.resolve(target, { cwd: root, signal: exec.signal })
              exec.signal.throwIfAborted()
              if (hostIdentities.some(previous =>
                hostFs.contains!(previous, canonicalTarget) && hostFs.contains!(canonicalTarget, previous))) {
                throw new Error('duplicate Host FS target identity in preflight batch')
              }
              hostIdentities.push(canonicalTarget)
            }
            if (shouldRollback && pinnedIntent?.kind !== 'replaceIfVersion') {
              throw new Error('rollbackOnFailure requires an existing file with a Host version')
            }
            const plan = planTextEdit(edit, true)
            if (!plan) throw new Error('provide exactly one of content or find/replace')
            if (plan.action === 'replace') {
              const original = await readTextSafe(ctx, exec, root, path)
              if (shouldRollback && Buffer.byteLength(original, 'utf8') > 2 * 1024 * 1024) {
                throw new Error('rollbackOnFailure snapshot exceeds 2 MiB')
              }
              const content = applyTextEdit(original, plan, true)
              prepared.push({ path, action: 'replace', content, original, pinnedIntent, note: 'replaced 1 occurrence' })
              checked.push({ path, action: 'replace', ok: true, note: 'replaced 1 occurrence' })
            } else {
              let original: string | undefined
              if (shouldRollback) {
                original = await readTextSafe(ctx, exec, root, path)
                assertEditableText(original)
                if (Buffer.byteLength(original, 'utf8') > 2 * 1024 * 1024) {
                  throw new Error('rollbackOnFailure snapshot exceeds 2 MiB')
                }
              }
              if (edit.expectedSha256 !== undefined) {
                if (!usePreflight) throw new Error('expectedSha256 requires preflight=true')
                if (typeof edit.expectedSha256 !== 'string' || !/^[a-f0-9]{64}$/.test(edit.expectedSha256)) {
                  throw new Error('expectedSha256 must be a 64-character lowercase SHA-256 digest')
                }
                original ??= await readTextSafe(ctx, exec, root, path)
                assertEditableText(original)
                const actualSha256 = createHash('sha256').update(original, 'utf8').digest('hex')
                if (actualSha256 !== edit.expectedSha256) throw new Error('expectedSha256 mismatch; file changed')
              }
              prepared.push({ path, action: 'overwrite', content: plan.content, original, pinnedIntent, note: plan.content.length + ' characters' })
              checked.push({ path, action: 'overwrite', ok: true, note: plan.content.length + ' characters' })
            }
          } catch (error) {
            isInvalid = true
            checked.push({
              path,
              action: 'skipped',
              ok: false,
              note: error instanceof SandboxViolationError ? 'path escapes workspace sandbox' : errorMessage(error),
            })
          }
        }
        if (isInvalid) {
          return {
            applied: 0,
            results: checked.map(item => item.ok
              ? { path: item.path, action: 'skipped' as const, ok: false, note: 'batch preflight blocked by another invalid edit' }
              : item),
            dryRun: Boolean(args.dryRun),
          }
        }
        if (args.dryRun) {
          // Preserve the legacy dryRun convention: "applied" counts validated edits.
          return { applied: prepared.length, results: checked, dryRun: true }
        }
        const written: EditResult[] = []
        const confirmed: Array<{ resultIndex: number; path: string; original: string; version: string }> = []
        const compensate = async (): Promise<{
          attempted: number; restored: number; failed: number; skippedReason?: string
        }> => {
          const rollback = { attempted: 0, restored: 0, failed: 0, skippedReason: undefined as string | undefined }
          if (exec.signal.aborted) {
            // Do not use a new signal to circumvent a cancelled Host operation.
            rollback.skippedReason = 'operation cancelled; no further writes authorized'
            return rollback
          }
          for (const entry of confirmed.reverse()) {
            if (exec.signal.aborted) {
              rollback.skippedReason = 'operation cancelled during recovery'
              break
            }
            rollback.attempted += 1
            try {
              // The exact version returned by OUR write is mandatory.
              // Never fetch a newer version and treat it as permission to restore.
              const restoredOutcome = await writeTextSafe(ctx, exec, root, entry.path, entry.original, settings,
                { kind: 'replaceIfVersion', version: entry.version })
              const restoredVersion = (restoredOutcome as { version?: unknown } | null)?.version
              if (typeof restoredVersion !== 'string' || restoredVersion.length === 0) {
                // The Host may have applied the restoration but its final state
                // is unconfirmed. Do not count it as a successful rollback.
                throw new Error('recovery outcome missing a valid Host version; state unconfirmed')
              }
              // A Host version acknowledges the write but is not evidence
              // that the restored bytes are readable and equal to our snapshot.
              // Fail closed if readback is denied, cancelled, or mismatched.
              const recoveredText = await readTextSafe(ctx, exec, root, entry.path)
              if (recoveredText !== entry.original) {
                throw new Error('recovery readback mismatch; state unconfirmed')
              }
              rollback.restored += 1
              applied -= 1
              written[entry.resultIndex] = {
                ...written[entry.resultIndex]!,
                ok: false, note: 'restored after later batch failure',
              }
            } catch (error) {
              rollback.failed += 1
              written[entry.resultIndex] = {
                ...written[entry.resultIndex]!,
                note: 'write may remain; recovery unconfirmed or refused: ' + errorMessage(error),
              }
            }
          }
          return rollback
        }
        for (const [index, item] of prepared.entries()) {
          let isWriteAttempted = false
          try {
            if (exec.signal.aborted) throw new Error('operation cancelled')
            if (item.original !== undefined) {
              const current = await readTextSafe(ctx, exec, root, item.path)
              if (current !== item.original) throw new Error('file changed after preflight')
            }
            isWriteAttempted = true
            const outcome = await writeTextSafe(ctx, exec, root, item.path, item.content, settings, item.pinnedIntent)
            // A guarded Host acknowledgment is not sufficient when the final
            // bytes disagree. Stop the batch and mark this write unconfirmed.
            if (settings.fsWriteContract === 'guarded-intent') {
              const observed = await readTextSafe(ctx, exec, root, item.path)
              if (observed !== item.content) {
                throw new Error('guarded Host write readback mismatch; outcome unconfirmed')
              }
            }
            applied += 1
            written.push({ path: item.path, action: item.action, ok: true, note: item.note })
            if (shouldRollback) {
              const version = (outcome as { version?: unknown } | null)?.version
              if (typeof version !== 'string' || version.length === 0) {
                // The current file has been written but cannot be safely restored:
                // do not risk a blind rollback or execute additional batch writes.
                written[index]!.note = 'write succeeded; Host returned no version, recovery unavailable'
                const rollback = await compensate()
                rollback.skippedReason = 'Host write outcome lacked version for ' + item.path
                for (const pending of prepared.slice(index + 1)) {
                  written.push({ path: pending.path, action: 'skipped', ok: false, note: 'recovery unavailable; batch stopped' })
                }
                return { applied, results: written, dryRun: false, rollback }
              }
              confirmed.push({ resultIndex: index, path: item.path, original: item.original!, version })
            }
          } catch (error) {
            written.push({
              path: item.path,
              action: 'skipped',
              ok: false,
              note: (error instanceof SandboxViolationError ? 'path escapes workspace sandbox' : errorMessage(error)) +
                (isWriteAttempted ? '; Host write outcome unconfirmed; inspect target before retry' : ''),
            })
            for (const pending of prepared.slice(index + 1)) {
              written.push({ path: pending.path, action: 'skipped', ok: false, note: 'earlier batch write failed' })
            }
            if (shouldRollback) {
              const rollback = await compensate()
              // The failing Host write may have partially mutated its own target.
              // Only earlier confirmed writes have an authoritative version.
              return { applied, results: written, dryRun: false, rollback }
            }
            break
          }
        }
        return { applied, results: written, dryRun: false }
      }
      for (const edit of edits) {
        const rel = typeof edit?.path === 'string' ? edit.path : ''
        let isWriteAttempted = false
        try {
          if (!edit || typeof edit !== 'object' || Array.isArray(edit) || typeof edit.path !== 'string') {
            throw new Error('edit must be an object with a string path')
          }
          const plan = planTextEdit(edit, false)
          if (!plan) {
            results.push({ path: rel, action: 'skipped', ok: false, note: 'missing content or find/replace' })
            continue
          }
          let note: string
          if (plan.action === 'replace') {
            const pinnedIntent = settings.fsWriteContract === 'guarded-intent'
              ? await validateWriteTargetSafe(ctx, exec, root, rel, settings)
              : undefined
            const original = await readTextSafe(ctx, exec, root, rel)
            const next = applyTextEdit(original, plan)
            // Catch a stale read before attempting a best-effort replacement.
            // Legacy Host FS cannot guarantee atomicity; guarded intent can.
            const current = await readTextSafe(ctx, exec, root, rel)
            if (current !== original) throw new Error('file changed before replacement')
            isWriteAttempted = true
            await writeTextSafe(ctx, exec, root, rel, next, settings, pinnedIntent)
            note = 'replaced 1 occurrence'
          } else {
            isWriteAttempted = true
            await writeTextSafe(ctx, exec, root, rel, plan.content, settings)
            // Preserve the established legacy response text; formatting is
            // outside the shared text transformation contract.
            note = `${plan.content.length} bytes`
          }
          applied += 1
          results.push({ path: rel, action: plan.action, ok: true, note })
        } catch (error) {
          results.push({
            path: rel,
            action: 'skipped',
            ok: false,
            note:
              (error instanceof SandboxViolationError
                ? 'path escapes workspace sandbox'
                : errorMessage(error)) +
              (isWriteAttempted ? '; Host write outcome unconfirmed; inspect target before retry' : ''),
          })
        }
      }
      return { applied, results, dryRun: Boolean(args.dryRun) }
    },
  })
}
