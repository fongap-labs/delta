/** Automatic public-source audit and CI PR submission; never activates an untrusted Skill. */
import type { DeltaGitHubTransport } from '../delta-code-adapter/contracts/github.js'
import type { DeltaGitHubExecution, DeltaGitHubToolSpec } from './github-operations.js'
import { createPublicSkillAuditSpecs } from './github-skill-public-audit.js'
import {
  confirmSkillTargetFile, confirmSkillTargetBranch,
  confirmSkillTargetCommit, confirmSkillTargetPullRequest,
} from './github-skill-target-evidence.js'
import {
  makePendingSkillProposal, pendingSkillProposalPath, pendingSkillProposalIdentity,
} from '../delta-code-strategy/skill-proposal-policy.js'

const REPOSITORY = /^[A-Za-z0-9-]{1,39}\/[A-Za-z0-9_.-]{1,100}$/
const CHANGELOG_HEADER = '## [Unreleased]\n\n'

export interface SkillProposalActions {
  readonly file: DeltaGitHubToolSpec
  readonly branch: DeltaGitHubToolSpec
  readonly commitFile: DeltaGitHubToolSpec
  readonly createPr: DeltaGitHubToolSpec
}

function parseToolResult(tool: { result: string }): any {
  const value = JSON.parse(tool.result)
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('GitHub Skill admission tool returned invalid evidence')
  }
  return value
}

/**
 * Explicit PR operation: requires configured GitHub writes, a target owner
 * binding and submit=true. Only inert JSON is collected, never a provider
 * registry entry. Main/production branch writes remain forbidden.
 */
export function createSkillAdmissionProposalSpecs(
  transport: DeltaGitHubTransport,
  actions: SkillProposalActions,
  allowWrites: () => boolean,
): DeltaGitHubToolSpec[] {
  const audit = createPublicSkillAuditSpecs(transport)[0]!
  return [{
    name: 'code_github_skill_propose_pr',
    description: 'Audit a pinned free public Skill without any source-account token, then submit an inert pending-CI-review manifest to a feature branch and PR in an explicitly authorized target repository. Requires github.allowWrites and submit=true. Never enables or executes a Skill.',
    parameters: {
      targetRepository: { type: 'string', required: true },
      repository: { type: 'string', required: true },
      path: { type: 'string', required: true },
      commit: { type: 'string', required: true },
      sourceBlobSha: { type: 'string', required: true },
      licenseBlobSha: { type: 'string', required: true },
      submit: { type: 'boolean', required: true },
    },
    async execute(args, exec: DeltaGitHubExecution) {
      if (args.submit !== true) throw new Error('Skill proposal PR requires explicit submit=true')
      if (!allowWrites()) throw new Error('GitHub writes are disabled; set github.allowWrites=true to enable')
      if (typeof args.targetRepository !== 'string' || !REPOSITORY.test(args.targetRepository)) {
        throw new Error('Invalid Skill proposal target repository')
      }
      // This review is re-run from immutable public evidence, never accepted
      // from user-supplied report JSON or a model-generated safety judgment.
      const auditResult = parseToolResult(await audit.execute(args, exec))
      const proposal = makePendingSkillProposal(auditResult)
      const source = proposal.source
      const path = pendingSkillProposalPath(proposal)
      const branchName = 'chore/skill-proposal-' + pendingSkillProposalIdentity(proposal)
      const repository = args.targetRepository as string

      // Verify the target's owner-bound read access *before* creating a branch.
      // Recheck the exact file blob at the feature branch snapshot so a
      // concurrently changed base cannot silently alter the user's proposal.
      const original = confirmSkillTargetFile(await actions.file.execute({
        repository, path: 'CHANGELOG.md', account: exec.account,
      }, exec), repository, 'CHANGELOG.md', exec.account)
      if (!original.content.startsWith('# ') ||
          !original.content.includes(CHANGELOG_HEADER) ||
          Buffer.byteLength(original.content, 'utf8') > 128 * 1024) {
        throw new Error('Target repository lacks a bounded Unreleased CHANGELOG')
      }
      const branchedResponse = await actions.branch.execute({
        repository, branch: branchName, account: exec.account,
      }, exec)
      const baseSha = confirmSkillTargetBranch(
        branchedResponse, repository, branchName, exec.account)
      const changelog = confirmSkillTargetFile(await actions.file.execute({
        repository, path: 'CHANGELOG.md', ref: baseSha, account: exec.account,
      }, exec), repository, 'CHANGELOG.md', exec.account)
      if (changelog.sha !== original.sha) {
        throw new Error('Target CHANGELOG changed during Skill proposal; discard the branch')
      }
      const current = changelog.content
      const entry = '- \u7ef4\u62a4: stage pinned free Skill proposal ' +
        source.repository + '/' + source.path + ' for CI review only; no runtime activation.\n'
      if (current.includes(entry)) throw new Error('Skill revision is already staged')
      const manifest = JSON.stringify(proposal, null, 2) + '\n'

      // Normal protected branch/optimistic HEAD rules remain authoritative.
      const stagedSha = confirmSkillTargetCommit(await actions.commitFile.execute({
        repository, branch: branchName, path, content: manifest, account: exec.account,
        expectedHead: baseSha,
        message: 'chore(delta-code): stage inert free Skill proposal',
      }, exec), repository, path, exec.account)
      const change = CHANGELOG_HEADER + entry + '\n'
      const updatedSha = confirmSkillTargetCommit(await actions.commitFile.execute({
        repository, branch: branchName, path: 'CHANGELOG.md', account: exec.account,
        content: current.replace(CHANGELOG_HEADER, change),
        expectedHead: stagedSha,
        message: 'docs(delta-code): record free Skill proposal CI admission',
      }, exec), repository, 'CHANGELOG.md', exec.account)
      const prResponse = await actions.createPr.execute({
        repository, head: branchName, account: exec.account,
        title: 'chore(delta-code): stage verified free Skill proposal',
        body: 'Immutable source: ' + source.repository + '/' + source.path +
          ' at ' + source.commit + '\n\n' +
          'Source Git blob: ' + source.sourceBlobSha + '\n' +
          'License Git blob: ' + source.licenseBlobSha + '\n\n' +
          'Anonymous audit found no known static risk indicators and identified a free license. ' +
          'Static review is not a security approval. The manifest remains inert after CI merge; ' +
          'runtime activation requires a separate reviewed catalog change. Do not bypass central CI.',
      }, exec)
      const pr = confirmSkillTargetPullRequest(
        prResponse, repository, exec.account)
      return {
        repository, account: branchedResponse.account,
        result: JSON.stringify({
          pr: { number: pr.number, url: pr.url, reused: pr.reused === true },
          source, path, branch: branchName, head: updatedSha,
          state: 'pending-ci-review', approvalRequired: true,
          autoEnabled: false, executable: false,
          note: 'PR only; central CI approval never grants runtime Skill execution. Activation remains a separate, pinned reviewed catalog change.',
        }),
      }
    },
  }]
}
