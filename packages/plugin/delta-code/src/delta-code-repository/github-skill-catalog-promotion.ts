/**
 * Controlled write path from an already staged free Skill proposal to a
 * separately CI-gated catalog upgrade PR. No background scanning or runtime
 * activation occurs in this tool; only the existing owner-scoped GitHub tools
 * can create branches/commits/PRs.
 */
import type { DeltaGitHubTransport } from '../delta-code-adapter/contracts/github.js'
import type { DeltaGitHubExecution, DeltaGitHubToolSpec } from './github-operations.js'
import { createPublicSkillAuditSpecs } from './github-skill-public-audit.js'
import { decodePinnedSkillBlob } from '../delta-code-strategy/free-skill-policy.js'
import {
  makePendingSkillProposal, pendingSkillProposalPath,
  pendingSkillProposalIdentity, validatePendingSkillProposal,
} from '../delta-code-strategy/skill-proposal-policy.js'
import {
  prepareReviewedSkillPromotion,
} from '../delta-code-strategy/skill-catalog-promotion.js'
import type { ReviewedSkillAdmission } from '../delta-code-strategy/skill-catalog-admission.js'
import type { SkillProposalActions } from './github-skill-admission-proposal.js'
import {
  assertSkillTargetEnvelope, confirmSkillTargetFile,
  confirmSkillTargetPullRequest,
} from './github-skill-target-evidence.js'

const REPOSITORY = /^[A-Za-z0-9-]{1,39}\/[A-Za-z0-9_.-]{1,100}$/
const SHA = /^[a-f0-9]{40}$/
const CHANGELOG_HEADER = '## [Unreleased]\n\n'
const CATALOG_PATH = 'packages/dsh-delta-code/src/delta-code-strategy/trusted-github-skills.ts'

function parseToolResult(tool: { result: string }): any {
  const parsed = JSON.parse(tool.result)
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw Error('GitHub Skill promotion tool response is not an object')
  }
  return parsed
}

function assertOwnerBound(
  response: { repository: string; account: string },
  repository: string,
  account?: string,
): void {
  assertSkillTargetEnvelope(response, repository, account)
}

function matchFile(response: { result: string; repository: string; account: string },
  target: string, path: string, account?: string): { content: string; sha: string } {
  return confirmSkillTargetFile(response, target, path, account)
}

/**
 * Promotion is *separate* from the inert proposal PR. The proposal must
 * already exist in the target repository on the base branch, and must match
 * a fresh anonymous audit of the source revision exactly.
 */
export function createSkillCatalogPromotionSpecs(
  transport: DeltaGitHubTransport,
  actions: SkillProposalActions,
  allowWrites: () => boolean,
): DeltaGitHubToolSpec[] {
  const audit = createPublicSkillAuditSpecs(transport)[0]!
  return [{
    name: 'code_github_skill_promote_pr',
    description: 'Create a separate central-CI-gated PR to promote an already staged, pinned, free Skill into the static approved catalog. Re-audits source and license anonymously; requires submit=true and github.allowWrites. Never bypasses Host permissions or enables a Skill before PR merge.',
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
      if (args.submit !== true) throw Error('Skill catalog promotion requires explicit submit=true')
      if (!allowWrites()) throw Error('GitHub writes are disabled; set github.allowWrites=true to enable')
      if (typeof args.targetRepository !== 'string' || !REPOSITORY.test(args.targetRepository) ||
          args.targetRepository.includes('..')) throw Error('Invalid Skill catalog target repository')
      if (!transport.publicRead) throw Error('Anonymous Skill review is required; no credential fallback')
      const target = args.targetRepository as string
      const account = exec.account
      const reviewed = parseToolResult(await audit.execute(args, exec))
      const staged = makePendingSkillProposal(reviewed)
      const proposalPath = pendingSkillProposalPath(staged)
      const source = staged.source
      const base = '/repos/' + source.repository.split('/').map(encodeURIComponent).join('/')
      // The publicRead transport enforces immutable SHA-only endpoints, strict
      // byte limits, no Authorization header and no redirects.
      exec.signal.throwIfAborted()
      const [skillData, licenseData] = await Promise.all([
        transport.publicRead(base + '/git/blobs/' + source.sourceBlobSha, exec.signal),
        transport.publicRead(base + '/git/blobs/' + source.licenseBlobSha, exec.signal),
      ])
      const skillMarkdown = decodePinnedSkillBlob(skillData, source.sourceBlobSha)
      const licenseText = decodePinnedSkillBlob(licenseData, source.licenseBlobSha, 32 * 1024)
      const refs = await Promise.all((source.references ?? []).map(async reference => {
        exec.signal.throwIfAborted()
        const blob = await transport.publicRead!(
          base + '/git/blobs/' + reference.blobSha, exec.signal,
        )
        return {
          path: reference.path, blobSha: reference.blobSha,
          markdown: decodePinnedSkillBlob(blob, reference.blobSha, 32 * 1024),
        }
      }))
      const admission: ReviewedSkillAdmission = {
        schemaVersion: staged.schemaVersion, proposal: staged,
        evidence: { skillMarkdown, licenseText,
          ...(staged.schemaVersion === 2 ? { references: refs } : {}) },
        permissions: {
          scope: 'documentation-only', runCommands: false,
          externalNetwork: false, readSecrets: false, writeFiles: false,
        },
      }

      // Read existing evidence and catalog before any write, then verify that
      // the branch was created from precisely the same immutable snapshot.
      const fetchFile = async (path: string, ref?: string) => {
        const response = await actions.file.execute({
          repository: target, path, account, ...(ref === undefined ? {} : { ref }),
        }, exec)
        return matchFile(response, target, path, account)
      }
      const [originalProposal, originalCatalog, originalChangelog] = await Promise.all([
        fetchFile(proposalPath), fetchFile(CATALOG_PATH), fetchFile('CHANGELOG.md'),
      ])
      const savedProposal = validatePendingSkillProposal(JSON.parse(originalProposal.content))
      if (JSON.stringify(savedProposal) !== JSON.stringify(staged)) {
        throw Error('Previously staged Skill proposal differs from immutable anonymous audit')
      }
      const prepared = prepareReviewedSkillPromotion(admission, originalCatalog.content)
      if (typeof originalChangelog.content !== 'string' ||
          !originalChangelog.content.includes(CHANGELOG_HEADER) ||
          originalChangelog.content.length > 128 * 1024) {
        throw Error('Target repository lacks an Unreleased CHANGELOG')
      }
      const branchName = 'chore/skill-promote-' + pendingSkillProposalIdentity(staged)
      const opened = await actions.branch.execute({
        repository: target, branch: branchName, account,
      }, exec)
      assertOwnerBound(opened, target, account)
      const branch = parseToolResult(opened)
      if (branch.branch !== branchName || typeof branch.sha !== 'string' || !SHA.test(branch.sha)) {
        throw Error('Skill promotion feature branch creation not confirmed')
      }
      const [pinnedProposal, pinnedCatalog, pinnedChangelog] = await Promise.all([
        fetchFile(proposalPath, branch.sha),
        fetchFile(CATALOG_PATH, branch.sha),
        fetchFile('CHANGELOG.md', branch.sha),
      ])
      if (pinnedProposal.sha !== originalProposal.sha ||
          pinnedCatalog.sha !== originalCatalog.sha ||
          pinnedChangelog.sha !== originalChangelog.sha) {
        throw Error('Target base branch changed during Skill promotion; discard the feature branch')
      }
      const entry = '- \u65b0\u589e: stage pinned free Skill catalog promotion ' +
        source.repository + '/' + source.path + ' for independent CI review.\n'
      const changelogContent = pinnedChangelog.content.replace(
        CHANGELOG_HEADER, CHANGELOG_HEADER + entry + '\n')
      let head: string = branch.sha
      const commits = [
        { path: prepared.admissionPath, content: prepared.admissionJson,
          message: 'chore(delta-code): add pinned free Skill admission review evidence' },
        { path: prepared.catalogPath, content: prepared.catalogContent,
          message: 'feat(delta-code): stage versioned approved Skill catalog update' },
        { path: 'CHANGELOG.md', content: changelogContent,
          message: 'docs(delta-code): record CI-reviewed Skill catalog promotion' },
      ]
      for (const change of commits) {
        const response = await actions.commitFile.execute({
          repository: target, account, branch: branchName,
          ...change, expectedHead: head,
        }, exec)
        assertOwnerBound(response, target, account)
        const result = parseToolResult(response)
        if (result.path !== change.path || typeof result.commit !== 'string' ||
            !SHA.test(result.commit)) {
          throw Error('Skill promotion commit not confirmed; do not open a PR')
        }
        head = result.commit
      }
      const response = await actions.createPr.execute({
        repository: target, account, head: branchName,
        title: 'feat(delta-code): admit pinned free ' + prepared.entry.name + ' Skill',
        body: 'Immutable Skill source: ' + source.repository + '/' + source.path +
          ' at ' + source.commit + '\n\nSource Git blob: ' + source.sourceBlobSha +
          '\nLicense Git blob: ' + source.licenseBlobSha +
          '\n\nThis is a separate catalog activation PR following an existing inert proposal. ' +
          'CI must verify the full pinned review bundle, exact prior proposal, no paid ' +
          'dependencies, read-only scope, and the catalog version bump. ' +
          'Do not bypass central CI or Host permission controls. Static validation ' +
          'does not guarantee arbitrary Skill instructions are safe.',
      }, exec)
      const pr = confirmSkillTargetPullRequest(response, target, account)
      return {
        repository: target, account: response.account,
        result: JSON.stringify({
          pr: { number: pr.number, url: pr.url, reused: pr.reused === true },
          branch: branchName, head,
          admissionPath: prepared.admissionPath, catalogPath: prepared.catalogPath,
          newCatalogVersion: prepared.nextVersion,
          candidate: { name: prepared.entry.name, ...source },
          state: 'awaiting-central-ci',
          note: 'No in-memory Skill activation. A successful separate catalog PR can activate the pinned Skill only after full central CI and merge.',
        }),
      }
    },
  }]
}
