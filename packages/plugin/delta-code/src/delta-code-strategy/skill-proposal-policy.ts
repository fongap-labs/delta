/** Staged Skill proposal contract. Proposal files are inert: never a trusted Skill catalog. */
import { isFreeSkillLicense } from './free-skill-policy.js'
import { createHash } from 'node:crypto'
import { SKILL_SOURCE_PATH, skillNameFromPath } from './skill-source-path.js'

const SHA = /^[a-f0-9]{40}$/
const REPOSITORY = /^[A-Za-z0-9-]{1,39}\/[A-Za-z0-9_.-]{1,100}$/

export interface PendingSkillProposal {
  readonly schemaVersion: 1 | 2
  readonly state: 'pending-ci-review'
  readonly source: {
    readonly repository: string
    readonly path: string
    readonly commit: string
    readonly sourceBlobSha: string
    readonly licenseBlobSha: string
    readonly license: string
    /** Only evidence from immutable, credential-free public review. */
    readonly references?: readonly { readonly path: string; readonly blobSha: string }[]
  }
  readonly staticAudit: {
    readonly status: 'static-review-ready' | 'pending-reference-bundle-review'
    readonly riskIndicators: readonly []
    readonly freeUseEvidence: true
  }
  readonly approvalRequired: true
  readonly autoEnabled: false
  readonly executable: false
}

export function makePendingSkillProposal(report: unknown): PendingSkillProposal {
  const data = report as {
    candidate?: Record<string, unknown>; license?: unknown
    reviewStatus?: unknown; freeEligibility?: { eligible?: unknown; status?: unknown }
    referenceEvidence?: unknown; bundleApprovalRequired?: unknown
    riskIndicators?: unknown; approvalRequired?: unknown; autoEnabled?: unknown; executable?: unknown
  }
  const isBundle = data?.reviewStatus === 'pending-reference-bundle-review'
  if (!data || (!isBundle && data.reviewStatus !== 'static-review-ready') ||
      (!isBundle && (data.freeEligibility?.eligible !== true ||
        data.freeEligibility.status !== 'free-evidence')) ||
      (isBundle && (data.freeEligibility?.status !== 'unknown' ||
        data.freeEligibility.eligible !== false)) ||
      !isFreeSkillLicense(data.license) || !Array.isArray(data.riskIndicators) ||
      data.riskIndicators.length !== 0 || data.approvalRequired !== true ||
      data.autoEnabled !== false || data.executable !== false) {
    throw new Error('Skill requires additional security/free-use review; refusing automatic catalog admission')
  }
  const source = data.candidate
  if (!source || typeof source.repository !== 'string' || !REPOSITORY.test(source.repository) ||
      typeof source.path !== 'string' || !SKILL_SOURCE_PATH.test(source.path) ||
      source.repository.includes('..') || ['.', '..'].includes(source.repository.split('/')[1]!) ||
      !['commit', 'sourceBlobSha', 'licenseBlobSha'].every(key =>
        typeof source[key] === 'string' && SHA.test(source[key]))) {
    throw new Error('Unverified immutable Skill proposal source')
  }
  const refs = data.referenceEvidence
  if (isBundle && (data.bundleApprovalRequired !== true ||
      !Array.isArray(refs) || refs.length < 1 || refs.length > 4)) {
    throw Error('Incomplete pinned secondary Skill reference evidence')
  }
  const folder = source.path.slice(0, source.path.lastIndexOf('/') + 1)
  const seen = new Set<string>()
  const references: Array<{ path: string; blobSha: string }> = []
  if (isBundle) {
    for (const ref of refs as unknown[]) {
      if (!ref || typeof ref !== 'object' || Array.isArray(ref)) {
        throw Error('Invalid secondary Skill reference evidence')
      }
      const row = ref as Record<string, unknown>
      const path = row.path
      if (typeof path !== 'string' || !path.startsWith(folder) ||
          !/^[A-Za-z0-9][A-Za-z0-9._/-]*\.(?:md|txt)$/i.test(path) ||
          path.split('/').some(segment => !segment || segment === '.' || segment === '..') ||
          path === source.path || seen.has(path.toLowerCase()) ||
          typeof row.blobSha !== 'string' || !SHA.test(row.blobSha) ||
          row.hasUnreviewedDependencies !== false || row.hasPaidRequirement !== false ||
          !Array.isArray(row.riskIndicators) || row.riskIndicators.length !== 0) {
        throw Error('Secondary Skill reference requires further risk or license review')
      }
      seen.add(path.toLowerCase())
      references.push({ path, blobSha: row.blobSha })
    }
  }
  return {
    schemaVersion: isBundle ? 2 : 1,
    state: 'pending-ci-review',
    source: {
      repository: source.repository, path: source.path,
      commit: source.commit as string, sourceBlobSha: source.sourceBlobSha as string,
      licenseBlobSha: source.licenseBlobSha as string, license: data.license as string,
      ...(isBundle ? { references } : {}),
    },
    staticAudit: {
      status: isBundle ? 'pending-reference-bundle-review' : 'static-review-ready',
      riskIndicators: [], freeUseEvidence: true,
    },
    approvalRequired: true, autoEnabled: false, executable: false,
  }
}

export function validatePendingSkillProposal(input: unknown): PendingSkillProposal {
  const value = input as PendingSkillProposal
  if (!value || ![1, 2].includes(value.schemaVersion) || value.state !== 'pending-ci-review' ||
      value.approvalRequired !== true || value.autoEnabled !== false ||
      value.executable !== false || !value.staticAudit ||
      value.staticAudit.status !== (value.schemaVersion === 2 ? 'pending-reference-bundle-review' : 'static-review-ready') ||
      value.staticAudit.freeUseEvidence !== true ||
      !Array.isArray(value.staticAudit.riskIndicators) ||
      value.staticAudit.riskIndicators.length !== 0 ||
      !value.source || !isFreeSkillLicense(value.source.license)) {
    throw new Error('Invalid inert Skill proposal manifest')
  }
  const check = makePendingSkillProposal({
    candidate: value.source, license: value.source.license,
    reviewStatus: value.staticAudit.status,
    freeEligibility: value.schemaVersion === 2 ? { status: 'unknown', eligible: false } :
      { status: 'free-evidence', eligible: value.staticAudit.freeUseEvidence },
    riskIndicators: value.staticAudit.riskIndicators,
    approvalRequired: value.approvalRequired,
    referenceEvidence: value.source.references?.map(ref => ({ ...ref,
      hasPaidRequirement: false, hasUnreviewedDependencies: false, riskIndicators: [] })),
    bundleApprovalRequired: value.schemaVersion === 2,
    autoEnabled: value.autoEnabled, executable: value.executable,
  })
  if (JSON.stringify(value) !== JSON.stringify(check)) {
    throw new Error('Skill proposal has unrecognized fields or different canonical representation')
  }
  return check
}

export function pendingSkillProposalIdentity(proposal: PendingSkillProposal): string {
  const source = proposal.source
  return createHash('sha256').update([
    source.repository.toLowerCase(), source.path, source.commit,
    source.sourceBlobSha, source.licenseBlobSha,
    ...(proposal.schemaVersion === 2 ? (source.references ?? []).map(ref => ref.path + ':' + ref.blobSha) : []),
  ].join('\0')).digest('hex').slice(0, 20)
}

export function pendingSkillProposalPath(proposal: PendingSkillProposal): string {
  const { repository, path } = proposal.source
  const skill = skillNameFromPath(path)!
  return 'packages/dsh-delta-code/skill-proposals/' +
    skill + '-' + pendingSkillProposalIdentity(proposal) + '.json'
}
