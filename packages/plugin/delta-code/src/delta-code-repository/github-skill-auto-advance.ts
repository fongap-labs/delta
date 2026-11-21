/** CI-confirmed continuation of a previously staged free Skill proposal.
 * Only stage PRs that GitHub confirms merged with all required gates may advance.
 * The separate catalog PR still needs its own CI before any runtime activation.
 */
import type { Settings } from '../types.js'
import type { DeltaGitHubTransport } from '../delta-code-adapter/contracts/github.js'
import type { DeltaGitHubExecution, DeltaGitHubToolSpec } from './github-operations.js'
import type { SkillProposalActions } from './github-skill-admission-proposal.js'
import { createSkillCatalogPromotionSpecs } from './github-skill-catalog-promotion.js'
import { selectGitHubAccount } from './github-policy.js'
import {
  assertSkillTargetEnvelope, parseSkillTargetReceipt,
} from './github-skill-target-evidence.js'

const REPO = /^[a-z0-9-]{1,39}\/[a-z0-9_.-]{1,100}$/i
const SHA = /^[a-f0-9]{40}$/
const REQUIRED = ['PR Governance', 'CI Evidence', 'validate-merge'] as const

/** A reported green CI badge never overrides a failed or missing required context. */
export function assertMergedSkillProposalEvidence(
  pr: unknown, checks: unknown, repository: string, expectedHead: string,
): void {
  if (!pr || typeof pr !== 'object' || !checks || typeof checks !== 'object' ||
    typeof repository !== 'string' || !REPO.test(repository) ||
    typeof expectedHead !== 'string' || !SHA.test(expectedHead)) {
    throw Error('Incomplete Skill proposal review evidence')
  }
  const merged = pr as Record<string, any>
  const run = checks as Record<string, any>
  if (merged.state !== 'closed' || merged.merged !== true ||
    typeof merged.merged_at !== 'string' || !merged.merged_at ||
    merged.head?.sha !== expectedHead ||
    merged.head?.repo?.full_name?.toLowerCase() !== repository.toLowerCase() ||
    merged.base?.repo?.full_name?.toLowerCase() !== repository.toLowerCase() ||
    run.head !== expectedHead || !Array.isArray(run.statuses) ||
    !Array.isArray(run.checks) || run.statuses.length > 100 || run.checks.length > 100) {
    throw Error('Skill proposal is not an exact merged CI-reviewed source revision')
  }
  for (const context of REQUIRED) {
    const matches = run.statuses.filter((row: any) => row?.context === context)
    if (matches.length !== 1 || matches[0]?.state !== 'success') {
      throw Error('Skill proposal missing successful required CI gate: ' + context)
    }
  }
  if (run.statuses.some((row: any) => !row || row.state !== 'success') ||
    run.checks.some((row: any) => !row || row.status !== 'completed' ||
      !['success', 'skipped', 'neutral'].includes(row.conclusion))) {
    throw Error('Skill proposal contains failed, cancelled or unconfirmed checks')
  }
}

/** No per-Skill manual signoff; GitHub merge evidence + Host permissions are authoritative. */
export function createAutoSkillAdvanceSpecs(
  settings: Settings, transport: DeltaGitHubTransport,
  actions: SkillProposalActions, checks: DeltaGitHubToolSpec,
  allowWrites: () => boolean,
): DeltaGitHubToolSpec[] {
  const promotion = createSkillCatalogPromotionSpecs(transport, actions, allowWrites)[0]!
  return [{
    name: 'code_github_skill_auto_advance',
    description: 'After a low-risk free Skill proposal has passed every required central CI gate and merged, automatically re-audit the immutable source and create its separate catalog promotion PR. Requires current Host GitHub write permission. Never enables Skill content before the second PR merges.',
    parameters: {
      targetRepository: { type: 'string', required: true },
      proposalPrNumber: { type: 'integer', required: true },
      expectedProposalHead: { type: 'string', required: true },
      repository: { type: 'string', required: true },
      path: { type: 'string', required: true },
      commit: { type: 'string', required: true },
      sourceBlobSha: { type: 'string', required: true },
      licenseBlobSha: { type: 'string', required: true },
    },
    async execute(args, exec: DeltaGitHubExecution) {
      if (!allowWrites()) throw Error('GitHub writes are disabled; set github.allowWrites=true to enable')
      if (typeof args.targetRepository !== 'string' || !REPO.test(args.targetRepository) ||
        args.targetRepository.includes('..') ||
        !Number.isSafeInteger(args.proposalPrNumber) || args.proposalPrNumber < 1 ||
        typeof args.expectedProposalHead !== 'string' || !SHA.test(args.expectedProposalHead)) {
        throw Error('Invalid source Skill proposal identity')
      }
      exec.signal.throwIfAborted()
      const target = args.targetRepository as string
      const owner = selectGitHubAccount(settings.githubAccounts, target, exec.account).owner
      const path = '/repos/' + target.split('/').map(encodeURIComponent).join('/') +
        '/pulls/' + args.proposalPrNumber
      const response = await transport.request({
        repository: target, account: owner, method: 'GET', path, signal: exec.signal,
      })
      assertSkillTargetEnvelope(response, target, exec.account)
      const checked = await checks.execute({
        repository: target, prNumber: args.proposalPrNumber,
      }, exec)
      const evidence = parseSkillTargetReceipt(checked, target, exec.account)
      assertMergedSkillProposalEvidence(response.data, evidence, target, args.expectedProposalHead)
      exec.signal.throwIfAborted()
      // No direct catalog writes: this creates a distinct reviewed PR, which
      // must be independently checked and merged before Host Provider sees it.
      const promoted = await promotion.execute({
        targetRepository: target, repository: args.repository,
        path: args.path, commit: args.commit,
        sourceBlobSha: args.sourceBlobSha, licenseBlobSha: args.licenseBlobSha,
        submit: true,
      }, exec)
      return {
        repository: promoted.repository, account: promoted.account,
        result: JSON.stringify({
          state: 'awaiting-catalog-ci', proposalPrNumber: args.proposalPrNumber,
          verifiedProposalHead: args.expectedProposalHead,
          promotion: JSON.parse(promoted.result),
          autoEnabled: false,
          note: 'Central CI approved the source proposal; a separate catalog PR remains pending. Host Skill activation requires its actual merge and plugin update.',
        }),
      }
    },
  }]
}
