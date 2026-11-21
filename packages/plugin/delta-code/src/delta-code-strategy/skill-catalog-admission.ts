/**
 * CI boundary for immutable, free-use GitHub Skill catalog promotions.
 *
 * A staged candidate is never a trust grant. A new catalog revision must have
 * its own self-contained review bundle, independently checked by CI. This
 * module never installs, executes, or fetches remote Skill instructions.
 */
import { createHash } from 'node:crypto'
import {
  freeLicenseFromText, inspectFreeSkillText, paidSkillRequirement,
  hasUnverifiedSkillDependencies, hasUnlinkedSkillDependencies,
} from './free-skill-policy.js'
import {
  validatePendingSkillProposal,
  pendingSkillProposalIdentity,
  type PendingSkillProposal,
} from './skill-proposal-policy.js'
import type { ApprovedGitHubSkill } from './trusted-github-skills.js'
import { skillStaticRiskIndicators } from '../delta-code-repository/github-skill-public-audit.js'
import { SKILL_SOURCE_PATH, skillNameFromPath } from './skill-source-path.js'
import { resolveSkillReferences } from './skill-reference-policy.js'

export interface ReviewedSkillAdmission {
  readonly schemaVersion: 1 | 2
  readonly proposal: PendingSkillProposal
  /** Immutable bytes carried in the PR, not fetched or run by CI. */
  readonly evidence: {
    readonly skillMarkdown: string
    readonly licenseText: string
    readonly references?: readonly {
      readonly path: string; readonly blobSha: string; readonly markdown: string
    }[]
  }
  /** New Skills cannot grant subprocess, network, secrets or write permissions. */
  readonly permissions: {
    readonly scope: 'documentation-only'
    readonly runCommands: false
    readonly externalNetwork: false
    readonly readSecrets: false
    readonly writeFiles: false
  }
}

interface LegacySkillRoot {
  readonly name: string
  readonly owner: string
  readonly repository: string
  readonly path: string
  readonly commit: string
  readonly blobSha: string
  readonly license: string
}

/**
 * Four existing reviewed roots, fixed before the admission gate was introduced.
 * These are trust anchors, not task-routing rules. New entries must use a
 * separately reviewed admission bundle rather than extending this list.
 */
const LEGACY_REVIEWED_ROOTS: readonly LegacySkillRoot[] = Object.freeze([
  {
    name: 'vercel-react-best-practices', owner: 'vercel-labs',
    repository: 'agent-skills', path: 'skills/react-best-practices/SKILL.md',
    commit: '1a2c79d4e6c4a2283dc2c614b6538cbe1ed5ca62',
    blobSha: '237988de4a66dd8a71d30a2c24ebe1a86b58d04e', license: 'MIT',
  },
  {
    name: 'vercel-composition-patterns', owner: 'vercel-labs',
    repository: 'agent-skills', path: 'skills/composition-patterns/SKILL.md',
    commit: '1a2c79d4e6c4a2283dc2c614b6538cbe1ed5ca62',
    blobSha: 'd07025bf943345bc1cf15eb923594f889ff881f0', license: 'MIT',
  },
  {
    name: 'anthropic-frontend-design', owner: 'anthropics',
    repository: 'skills', path: 'skills/frontend-design/SKILL.md',
    commit: 'e4bb5d960d87126ff3b3a8af1a104b7db67560ba',
    blobSha: 'a5333457c414d20d625f307df945842c0952ecc3', license: 'Apache-2.0',
  },
  {
    name: 'addy-api-and-interface-design', owner: 'addyosmani',
    repository: 'agent-skills', path: 'skills/api-and-interface-design/SKILL.md',
    commit: '39b723ceebf81474d0557c1fecd01eddab585563',
    blobSha: 'ce58d72ed0ee6568400f36564c94becfaeede717', license: 'MIT',
  },
])

const NAME = /^[a-z0-9]+(?:-[a-z0-9]+)*$/
const REV = /^[a-f0-9]{40}$/
const REVIEW_DIR = 'packages/dsh-delta-code/skill-admissions/'

function ensureKeys(value: unknown, keys: readonly string[], label: string): asserts value is Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value) ||
      Object.keys(value).sort().join('|') !== [...keys].sort().join('|')) {
    throw Error('Invalid ' + label + ' fields')
  }
}

function blobSha(text: string): string {
  const bytes = Buffer.from(text, 'utf8')
  return createHash('sha1').update('blob ' + bytes.length + '\0').update(bytes).digest('hex')
}

function sourceName(markdown: string): string | null {
  const frontmatter = /^---\r?\n([\s\S]{1,4096}?)\r?\n---\r?\n/.exec(markdown)
  if (!frontmatter || !markdown.slice(frontmatter[0].length).trim()) return null
  return /^name:\s*['"]?([a-z0-9-]+)['"]?\s*$/m.exec(frontmatter[1])?.[1] ?? null
}

function assertCatalogMetadata(entry: ApprovedGitHubSkill): void {
  if (!entry || !NAME.test(entry.name) ||
      !/^[a-z0-9-]{1,39}$/.test(entry.owner) ||
      !/^[a-z0-9_.-]{1,100}$/.test(entry.repository) ||
      entry.repository.includes('..') || !SKILL_SOURCE_PATH.test(entry.path) ||
      !REV.test(entry.commit) || !REV.test(entry.blobSha) ||
      typeof entry.description !== 'string' ||
      entry.description.length < 8 || entry.description.length > 300 ||
      skillStaticRiskIndicators(entry.description).length > 0 ||
      !Array.isArray(entry.keywords) || !entry.keywords.length ||
      entry.keywords.length > 40 ||
      entry.keywords.some(word => typeof word !== 'string' || word.length < 2 ||
        word.length > 64 || /[\r\n\x00-\x1f]/.test(word) ||
        skillStaticRiskIndicators(word).length > 0)) {
    throw Error('Invalid reviewed Skill catalog metadata')
  }
  if (entry.sourceName !== undefined &&
      (!NAME.test(entry.sourceName) || entry.sourceName !== skillNameFromPath(entry.path))) {
    throw Error('Catalog sourceName does not match pinned Skill path')
  }
  if (entry.references !== undefined) {
    if (!Array.isArray(entry.references) || entry.references.length < 1 ||
        entry.references.length > 4) throw Error('Invalid reviewed Skill reference list')
    const folder = entry.path.slice(0, entry.path.lastIndexOf('/') + 1)
    const markdown = entry.references.map(ref => {
      if (!ref || typeof ref.path !== 'string' ||
          !ref.path.startsWith(folder) || !REV.test(ref.blobSha)) {
        throw Error('Invalid pinned Skill reference metadata')
      }
      return '[reference](' + ref.path.slice(folder.length) + ')'
    }).join(' ')
    const resolved = resolveSkillReferences(entry.path, markdown)
    if (resolved.some((ref, index) => ref.path !== entry.references![index]!.path)) {
      throw Error('Unsafe approved Skill reference path')
    }
  }
}

/**
 * Verify an explicit, reviewable immutable evidence bundle. The result is
 * an admission *gate*, not proof that arbitrary natural language is safe.
 */
export function validateReviewedSkillAdmission(value: unknown): ReviewedSkillAdmission {
  ensureKeys(value, ['schemaVersion', 'proposal', 'evidence', 'permissions'], 'Skill admission')
  if (value.schemaVersion !== 1 && value.schemaVersion !== 2) throw Error('Unsupported Skill admission version')
  const proposal = validatePendingSkillProposal(value.proposal)
  ensureKeys(value.evidence, value.schemaVersion === 2
    ? ['skillMarkdown', 'licenseText', 'references'] : ['skillMarkdown', 'licenseText'], 'Skill admission evidence')
  ensureKeys(value.permissions, [
    'scope', 'runCommands', 'externalNetwork', 'readSecrets', 'writeFiles',
  ], 'Skill permission evidence')
  const e = value.evidence
  const permissions = value.permissions
  if (typeof e.skillMarkdown !== 'string' || typeof e.licenseText !== 'string' ||
      Buffer.byteLength(e.skillMarkdown, 'utf8') > 64 * 1024 ||
      Buffer.byteLength(e.licenseText, 'utf8') > 32 * 1024 ||
      !e.skillMarkdown || !e.licenseText) {
    throw Error('Skill admission source/license exceeds bounded evidence limits')
  }
  if (permissions.scope !== 'documentation-only' ||
      permissions.runCommands !== false || permissions.externalNetwork !== false ||
      permissions.readSecrets !== false || permissions.writeFiles !== false) {
    throw Error('Skill admission cannot grant external tools or privileges')
  }
  const source = proposal.source
  if (blobSha(e.skillMarkdown) !== source.sourceBlobSha ||
      blobSha(e.licenseText) !== source.licenseBlobSha ||
      freeLicenseFromText(e.licenseText) !== source.license ||
      sourceName(e.skillMarkdown) !== skillNameFromPath(source.path)) {
    throw Error('Skill admission immutable source, frontmatter or license mismatch')
  }
  const cost = inspectFreeSkillText(e.skillMarkdown, source.license)
  const isBundle = value.schemaVersion === 2
  if ((!isBundle && !cost.eligible) ||
      (isBundle && (paidSkillRequirement(e.skillMarkdown) !== null ||
        hasUnlinkedSkillDependencies(e.skillMarkdown))) ||
      skillStaticRiskIndicators(e.skillMarkdown).length > 0 ||
      /(?:\]\(https?:\/\/|\b(?:sudo|rm\s+-rf|git\s+push|powershell|sh\s+-c)\b)/i.test(e.skillMarkdown) ||
      (!isBundle && /\.{1,2}\//.test(e.skillMarkdown))) {
    throw Error('Skill admission requires a self-contained, low-risk, free reference')
  }
  if (isBundle) {
    if (proposal.schemaVersion !== 2 || !Array.isArray(e.references) ||
        !Array.isArray(source.references) ||
        e.references.length === 0 || e.references.length > 4 ||
        e.references.length !== source.references.length) {
      throw Error('Incomplete reviewed Skill reference bundle')
    }
    const declared = resolveSkillReferences(source.path, e.skillMarkdown)
    if (declared.length !== e.references.length) {
      throw Error('Reviewed bundle source reference count differs')
    }
    // Runtime only accepts 96 KiB in total. Admission must not mint a
    // trusted catalog entry that the native provider can never load.
    let bundleBytes = Buffer.byteLength(e.skillMarkdown, 'utf8')
    for (let index = 0; index < declared.length; index++) {
      const ref = e.references[index]!
      const pinned = source.references[index]!
      ensureKeys(ref, ['path', 'blobSha', 'markdown'], 'reviewed Skill reference')
      if (ref.path !== declared[index]!.path || ref.path !== pinned.path ||
          ref.blobSha !== pinned.blobSha ||
          typeof ref.markdown !== 'string' ||
          Buffer.byteLength(ref.markdown, 'utf8') > 32 * 1024 ||
          !ref.markdown.trim() || blobSha(ref.markdown) !== pinned.blobSha ||
          paidSkillRequirement(ref.markdown) !== null ||
          hasUnverifiedSkillDependencies(ref.markdown) ||
          hasUnlinkedSkillDependencies(ref.markdown) ||
          skillStaticRiskIndicators(ref.markdown).length > 0 ||
          /\b(?:sudo|rm\s+-rf|git\s+push|powershell|sh\s+-c)\b/i.test(ref.markdown)) {
        throw Error('Reviewed secondary Skill content, license or immutable hash mismatch')
      }
      bundleBytes += Buffer.byteLength(ref.markdown, 'utf8')
      if (bundleBytes > 96 * 1024) {
        throw Error('Reviewed Skill bundle exceeds native context size budget')
      }
    }
  } else if (proposal.schemaVersion !== 1) {
    throw Error('Unsupported versioned Skill review evidence')
  }
  return value as unknown as ReviewedSkillAdmission
}

export function reviewedSkillAdmissionPath(admission: ReviewedSkillAdmission): string {
  const verified = validateReviewedSkillAdmission(admission)
  const name = skillNameFromPath(verified.proposal.source.path)!
  return REVIEW_DIR + name + '-' + pendingSkillProposalIdentity(verified.proposal) + '.json'
}

function sourceMatches(entry: ApprovedGitHubSkill, proposal: PendingSkillProposal): boolean {
  const src = proposal.source
  return entry.owner === src.repository.split('/')[0] &&
    entry.repository === src.repository.split('/')[1] &&
    entry.path === src.path && entry.commit === src.commit &&
    entry.blobSha === src.sourceBlobSha && entry.license === src.license &&
    JSON.stringify(entry.references ?? []) === JSON.stringify(src.references ?? [])
}

/**
 * Every new catalog pin must be backed by an independently checked review
 * bundle. Staged files alone never activate a Skill.
 */
export function assertReviewedSkillCatalog(
  catalog: readonly ApprovedGitHubSkill[],
  admissions: readonly ReviewedSkillAdmission[],
  catalogVersion = 1,
): void {
  if (!Number.isSafeInteger(catalogVersion) || catalogVersion < 1) {
    throw Error('Invalid approved Skill catalog version')
  }
  const names = new Set<string>()
  const reviewed = new Set<string>()
  for (const record of admissions) {
    const evidence = validateReviewedSkillAdmission(record)
    const id = pendingSkillProposalIdentity(evidence.proposal)
    if (reviewed.has(id)) throw Error('Duplicate reviewed Skill admission evidence')
    reviewed.add(id)
  }
  for (const entry of catalog) {
    assertCatalogMetadata(entry)
    if (names.has(entry.name)) throw Error('Duplicate reviewed GitHub Skill name: ' + entry.name)
    names.add(entry.name)
    const legacy = LEGACY_REVIEWED_ROOTS.some(root =>
      Object.keys(root).every(key => (entry as unknown as Record<string, unknown>)[key] ===
        (root as unknown as Record<string, unknown>)[key]))
    if (legacy) continue
    if (!admissions.some(record => sourceMatches(entry, record.proposal))) {
      throw Error('Unreviewed GitHub Skill catalog entry: ' + entry.name)
    }
    if (catalogVersion <= 1) throw Error('Approved Skill catalog version must increase for new admissions')
    if ((entry.sourceName ?? entry.name) !== skillNameFromPath(entry.path)) {
      throw Error('New catalog Skill must match the reviewed sourceName')
    }
  }
}
