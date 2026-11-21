/** One-shot discovery -> immutable public audit -> Host-governed proposal PR.
 * A candidate is never executable or trusted before a separate CI-gated catalog PR.
 */
import type { DeltaGitHubTransport } from '../delta-code-adapter/contracts/github.js'
import type { DeltaGitHubExecution, DeltaGitHubToolSpec } from './github-operations.js'
import type { SkillProposalActions } from './github-skill-admission-proposal.js'
import { createSkillAdmissionProposalSpecs } from './github-skill-admission-proposal.js'
import { createPublicSkillDiscoverySpecs } from './github-skill-public-discovery.js'
import { createPublicSkillAuditSpecs } from './github-skill-public-audit.js'
import { isExternalSkillOptOut } from '../delta-code-strategy/trusted-github-skills.js'
import { SKILL_SOURCE_PATH } from '../delta-code-strategy/skill-source-path.js'
import { makePendingSkillProposal } from '../delta-code-strategy/skill-proposal-policy.js'

const SHA = /^[a-f0-9]{40}$/
const REPO = /^[a-z0-9-]{1,39}\/[a-z0-9_.-]{1,100}$/i
const MAX_AUDITS = 3

export interface AutoSkillCandidate {
  readonly repository: string
  readonly path: string
  readonly suggestedCommit: string
  readonly sourceBlobSha: string
  readonly freeEvidence: {
    readonly licenseBlobSha: string
    readonly checkedAtCommit: string
  }
}

/** External search metadata is untrusted; never pass arbitrary fields to a writer. */
export function eligibleAutoSkillCandidates(value: unknown): AutoSkillCandidate[] {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw Error('Skill discovery evidence is not an object')
  }
  const record = value as Record<string, unknown>
  if (!Array.isArray(record.candidates) || record.candidates.length > 6 ||
    typeof record.isRateLimited !== 'boolean') {
    throw Error('Incomplete or unbounded public Skill discovery result')
  }
  const seen = new Set<string>()
  const result: AutoSkillCandidate[] = []
  for (const row of record.candidates) {
    if (!row || typeof row !== 'object' || Array.isArray(row)) continue
    const candidate = row as Record<string, unknown>
    const free = candidate.freeEvidence
    if (!free || typeof free !== 'object' || Array.isArray(free)) continue
    const pinned = free as Record<string, unknown>
    if (typeof candidate.repository !== 'string' || !REPO.test(candidate.repository) ||
      candidate.repository.includes('..') || typeof candidate.path !== 'string' ||
      !SKILL_SOURCE_PATH.test(candidate.path) || typeof candidate.suggestedCommit !== 'string' ||
      !SHA.test(candidate.suggestedCommit) ||
      typeof candidate.sourceBlobSha !== 'string' || !SHA.test(candidate.sourceBlobSha) ||
      typeof pinned.licenseBlobSha !== 'string' || !SHA.test(pinned.licenseBlobSha) ||
      pinned.checkedAtCommit !== candidate.suggestedCommit ||
      candidate.status !== 'unreviewed' || candidate.requiresReview !== true) continue
    const id = candidate.repository.toLowerCase() + '/' + candidate.path + ':' +
      candidate.suggestedCommit + ':' + candidate.sourceBlobSha
    if (seen.has(id)) continue
    seen.add(id)
    result.push({
      repository: candidate.repository,
      path: candidate.path,
      suggestedCommit: candidate.suggestedCommit,
      sourceBlobSha: candidate.sourceBlobSha,
      freeEvidence: {
        checkedAtCommit: candidate.suggestedCommit,
        licenseBlobSha: pinned.licenseBlobSha,
      },
    })
  }
  return result.slice(0, MAX_AUDITS)
}

function parsed(response: { repository: string; account: string; result: string }): Record<string, unknown> {
  if (response.repository !== 'github.com/public' || response.account !== 'anonymous') {
    throw Error('Public Skill evidence must not use owner-bound credentials')
  }
  const data: unknown = JSON.parse(response.result)
  if (!data || typeof data !== 'object' || Array.isArray(data)) {
    throw Error('Invalid public Skill evidence')
  }
  return data as Record<string, unknown>
}

/** The Host's GitHub write approval remains mandatory. There is no second consent gate. */
export function createAutoSkillStageSpecs(
  transport: DeltaGitHubTransport,
  actions: SkillProposalActions,
  allowWrites: () => boolean,
): DeltaGitHubToolSpec[] {
  const discovery = createPublicSkillDiscoverySpecs(transport)[0]!
  const audit = createPublicSkillAuditSpecs(transport)[0]!
  const proposal = createSkillAdmissionProposalSpecs(transport, actions, allowWrites)[0]!
  return [{
    name: 'code_github_skill_auto_stage',
    description: 'For a requested specialist coding task, automatically search FREE public GitHub Skills, independently audit up to 3 pinned candidates, then stage one eligible low-risk Skill as a central-CI-gated PR. Requires owner-bound github.allowWrites and Host permission. Does NOT grant Skill trust, execute unapproved content, or bypass central CI. No per-Skill human sign-off beyond existing Host and CI policy.',
    parameters: {
      targetRepository: { type: 'string', required: true },
      query: { type: 'string', required: true },
    },
    async execute(args, exec: DeltaGitHubExecution) {
      if (!allowWrites()) throw Error('GitHub writes are disabled; set github.allowWrites=true to enable')
      if (typeof args.targetRepository !== 'string' || !REPO.test(args.targetRepository) ||
          args.targetRepository.includes('..')) {
        throw Error('Invalid Skill catalog target repository')
      }
      if (typeof args.query !== 'string' || isExternalSkillOptOut(args.query)) {
        throw Error('External Skill discovery was disabled for this task')
      }
      if (!transport.publicRead) {
        throw Error('Credential-free public discovery is unavailable; never fall back to another account')
      }
      exec.signal.throwIfAborted()
      const evidence = parsed(await discovery.execute({ query: args.query }, exec))
      const candidates = eligibleAutoSkillCandidates(evidence)
      if (evidence.isRateLimited === true) {
        return { repository: args.targetRepository, account: exec.account ?? 'unresolved',
          result: JSON.stringify({ state: 'blocked', reason: 'public-rate-limit',
            reviewed: 0, staged: false }) }
      }
      let reviewed = 0
      let excluded = 0
      for (const candidate of candidates) {
        exec.signal.throwIfAborted()
        const pinned = {
          repository: candidate.repository, path: candidate.path,
          commit: candidate.suggestedCommit, sourceBlobSha: candidate.sourceBlobSha,
          licenseBlobSha: candidate.freeEvidence.licenseBlobSha,
        }
        const verification = parsed(await audit.execute(pinned, exec))
        reviewed++
        // One source of truth for both single-file and verified bundle
        // proposal admission. A tool-provided status alone is not enough.
        try {
          makePendingSkillProposal(verification)
        } catch {
          excluded++
          continue
        }
        // One write only, after full immutable audit. Never retry an uncertain
        // GitHub branch/commit/PR write or substitute a different owner token.
        const created = await proposal.execute({
          ...pinned, targetRepository: args.targetRepository, submit: true,
        }, exec)
        return { repository: created.repository, account: created.account,
          result: JSON.stringify({
            state: 'pending-ci-review', staged: true, reviewed, excluded,
            candidate: pinned, proposal: JSON.parse(created.result),
            next: 'Wait for verified central CI + protected-branch merge; then independently re-audit and promote using code_github_skill_promote_pr.',
          }) }
      }
      return { repository: args.targetRepository, account: exec.account ?? 'unresolved',
        result: JSON.stringify({ state: 'blocked', reason: 'no-free-low-risk-candidate',
          staged: false, reviewed, excluded, candidates: candidates.length }) }
    },
  }]
}
