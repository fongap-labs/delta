/** Read-only, account-scoped GitHub Skill discovery and candidate evidence. */
import { createHash } from 'node:crypto'
import type { Settings } from '../types.js'
import type { DeltaGitHubTransport } from '../delta-code-adapter/contracts/github.js'
import { GitHubApiFailure } from '../delta-code-adapter/contracts/github.js'
import { selectGitHubAccount, validatedCommitSha } from './github-policy.js'
import { getActiveApprovedGitHubSkills } from '../delta-code-strategy/trusted-github-skills.js'
import { rankSkillEvidence } from './github-skill-ranking.js'
import { SKILL_SOURCE_PATH, skillNameFromPath } from '../delta-code-strategy/skill-source-path.js'
import { freeLicenseFromText, isFreeSkillLicense, inspectFreeSkillText } from '../delta-code-strategy/free-skill-policy.js'
import { createPublicSkillDiscoverySpecs } from './github-skill-public-discovery.js'
import type { DeltaGitHubExecution, DeltaGitHubRepositoryResolver, DeltaGitHubToolSpec } from './github-operations.js'

const MAX_SKILL_BYTES = 64 * 1024
const MAX_LICENSE_BYTES = 32 * 1024
const LICENSE_PATH = /^(?:(?:[a-z0-9]+(?:-[a-z0-9]+)*\/){0,3})LICENSE(?:\.md|\.txt)?$/i

function decodeFile(data: any, maximum: number): { text: string; sha: string } {
  if (!data || Array.isArray(data) || data.type !== 'file' ||
    data.encoding !== 'base64' || typeof data.content !== 'string' ||
    !Number.isSafeInteger(data.size) || data.size <= 0 || data.size > maximum) {
    throw Error('Skill review requires one bounded GitHub UTF-8 file')
  }
  const clean = data.content.replace(/\s/g, '')
  if (!/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(clean)) {
    throw Error('Invalid GitHub file encoding')
  }
  const bytes = Buffer.from(clean, 'base64')
  if (!bytes.length || bytes.length !== data.size || bytes.length > maximum) {
    throw Error('Invalid GitHub file size')
  }
  const sha = createHash('sha1').update('blob ' + bytes.length + '\0').update(bytes).digest('hex')
  if (sha !== data.sha) throw Error('GitHub file content hash mismatch')
  return { text: new TextDecoder('utf-8', { fatal: true }).decode(bytes), sha }
}

function candidateMetadata(markdown: string): { name: string; description: string; hasRelativeReferences: boolean; hasCommandInstructions: boolean } {
  const frontmatter = /^---\r?\n([\s\S]{1,4096}?)\r?\n---\r?\n/.exec(markdown)
  if (!frontmatter || !markdown.slice(frontmatter[0].length).trim()) {
    throw Error('Skill candidate has no bounded SKILL.md frontmatter or body')
  }
  const name = /^name:\s*['"]?([a-z0-9-]+)['"]?\s*$/m.exec(frontmatter[1])?.[1]
  const description = /^description:\s*(.+?)\s*$/m.exec(frontmatter[1])?.[1]
  if (!name || !description) throw Error('Skill candidate name or description unavailable')
  return {
    name, description: description.slice(0, 200),
    hasRelativeReferences: /\]\((?:\.\.?\/|references\/|scripts\/|assets\/)[^)]*\)|(?:^|\s)(?:\.\.?\/)?(?:references|scripts|assets)\//im.test(markdown),
    hasCommandInstructions: /(?:^|\s)(?:curl|wget|npm\s+install|pip\s+install|bash|chmod|eval|npx)\b/im.test(markdown),
  }
}

export function createGitHubSkillDiscoverySpecs(
  settings: Settings, transport: DeltaGitHubTransport, resolveRepo: DeltaGitHubRepositoryResolver,
): DeltaGitHubToolSpec[] {
  const get = async (repository: string, exec: DeltaGitHubExecution, path: string) => {
    exec.signal.throwIfAborted()
    const account = selectGitHubAccount(settings.githubAccounts, repository, exec.account)
    const response = await transport.request({
      repository, account: account.owner, method: 'GET', path, signal: exec.signal,
    })
    exec.signal.throwIfAborted()
    return response
  }
  const prefix = (repository: string) => '/repos/' + repository.split('/').map(encodeURIComponent).join('/')
  const output = (repository: string, account: string, result: unknown) => ({
    repository, account, result: JSON.stringify(result),
  })
  return [
    {
      name: 'code_github_skill_candidates',
      description: 'Search only free-license and no-paid-prerequisite GitHub SKILL.md candidates in the authorized repository; unknown-cost and paid results are excluded, and none is invocable before review.',
      parameters: { repository: { type: 'string', required: true }, query: { type: 'string', required: true } },
      async execute(args, exec) {
        if (typeof args.query !== 'string' || !/^[\p{L}\p{N}_ .+\-]{2,100}$/u.test(args.query.trim())) {
          throw Error('Skill search requires a bounded plain-text query')
        }
        const repository = await resolveRepo(args.repository, exec)
        const q = 'repo:' + repository + ' filename:SKILL.md ' + args.query.trim()
        const result = await get(repository, exec, '/search/code?per_page=20&q=' + encodeURIComponent(q))
        if (!Array.isArray(result.data?.items) || !Number.isSafeInteger(result.data?.total_count)) {
          throw Error('GitHub Skill search response is incomplete')
        }
        const latest = await get(repository, exec, prefix(repository) + '/commits?per_page=1')
        const suggestedCommit = Array.isArray(latest.data) &&
          /^[a-f0-9]{40}$/.test(latest.data[0]?.sha) ? latest.data[0].sha : null
        const matched = result.data.items.slice(0, 20)
          .filter((item: any) => item && typeof item.path === 'string' && SKILL_SOURCE_PATH.test(item.path))
        // One repository and one public maintainer-profile lookup per search,
        // never per candidate. Both use the token already bound to this owner;
        // no cross-owner token reuse or secondary HTTP client is introduced.
        const advisoryMetadata = async (path: string): Promise<any | null> => {
          try {
            return (await get(repository, exec, path)).data
          } catch (error) {
            if (error instanceof GitHubApiFailure && [403, 404, 429].includes(error.status)) return null
            throw error
          }
        }
        const owner = repository.split('/')[0]!
        const repoMeta = matched.length ? await advisoryMetadata(prefix(repository)) : null
        const ownerMeta = matched.length ? await advisoryMetadata('/users/' + encodeURIComponent(owner)) : null
        if (repoMeta?.full_name !== undefined &&
            (typeof repoMeta.full_name !== 'string' ||
            repoMeta.full_name.toLowerCase() !== repository.toLowerCase())) {
          throw Error('GitHub repository evidence does not match requested owner/repository')
        }
        if (ownerMeta?.login !== undefined &&
            (typeof ownerMeta.login !== 'string' ||
            ownerMeta.login.toLowerCase() !== owner.toLowerCase())) {
          throw Error('GitHub maintainer evidence does not match requested owner')
        }
        const ranking = rankSkillEvidence({
          stars: repoMeta?.stargazers_count, forks: repoMeta?.forks_count,
          ownerCreatedAt: ownerMeta?.created_at,
          repositoryCreatedAt: repoMeta?.created_at,
          lastPushAt: repoMeta?.pushed_at,
          archived: repoMeta?.archived, disabled: repoMeta?.disabled,
        })
        const candidates: any[] = []
        let excludedByCost = 0
        const licenseName = repoMeta?.license?.spdx_id
        // Metadata alone is insufficient. Verify the license and exact Skill
        // content at the same immutable commit before showing recommendations.
        if (isFreeSkillLicense(licenseName) && suggestedCommit && matched.length) {
          try {
            const license = await get(repository, exec, prefix(repository) +
              '/contents/LICENSE?ref=' + suggestedCommit)
            const licenseText = decodeFile(license.data, MAX_LICENSE_BYTES).text
            if (freeLicenseFromText(licenseText) === licenseName) {
              for (const item of matched.slice(0, 4)) {
                try {
                  const source = await get(repository, exec, prefix(repository) +
                    '/contents/' + item.path + '?ref=' + suggestedCommit)
                  const markdown = decodeFile(source.data, MAX_SKILL_BYTES).text
                  const cost = inspectFreeSkillText(markdown, licenseName)
                  const metadata = candidateMetadata(markdown)
                  if (!cost.eligible || metadata.name !== skillNameFromPath(item.path)) {
                    excludedByCost++
                    continue
                  }
                  candidates.push({
                    repository, path: item.path, name: metadata.name,
                    suggestedCommit, status: 'unreviewed', requiresReview: true,
                    freeEvidence: { license: licenseName, checkedAtCommit: suggestedCommit },
                    ranking,
                  })
                } catch (error) {
                  if (error instanceof GitHubApiFailure && [403, 404, 429].includes(error.status)) {
                    excludedByCost++
                    if (error.status !== 404) break
                    continue
                  }
                  throw error
                }
              }
            } else excludedByCost += matched.length
          } catch (error) {
            if (!(error instanceof GitHubApiFailure && [403, 404, 429].includes(error.status))) throw error
            excludedByCost += matched.length
          }
        } else excludedByCost += matched.length
        return output(repository, result.account, {
          totalMatches: result.data.total_count, candidates, truncated: result.data.total_count > 4,
          excludedByCost,
          rankingStatus: ranking.completeness,
          note: 'Free-only: paid or unknown-use-cost candidates are excluded. Pinned free-license evidence and a basic paid-prerequisite check are not proof external services will never charge. Results remain unreviewed and cannot be invoked without approval.',
        })
      },
    },
    {
      name: 'code_github_skill_review',
      description: 'Inspect one pinned GitHub SKILL.md and its license evidence without running or loading it; always requires independent approval.',
      parameters: {
        repository: { type: 'string', required: true }, path: { type: 'string', required: true },
        commit: { type: 'string', required: true }, licensePath: { type: 'string' },
      },
      async execute(args, exec) {
        const repository = await resolveRepo(args.repository, exec)
        const commit = validatedCommitSha(args.commit, 'Skill candidate')
        if (typeof args.path !== 'string' || !SKILL_SOURCE_PATH.test(args.path)) {
          throw Error('Skill candidate must be a bounded named SKILL.md file')
        }
        const licensePath = args.licensePath ?? 'LICENSE'
        if (typeof licensePath !== 'string' || !LICENSE_PATH.test(licensePath)) {
          throw Error('Unsupported Skill license evidence path')
        }
        const path = prefix(repository) + '/contents/' + args.path + '?ref=' + commit
        const skill = await get(repository, exec, path)
        const source = decodeFile(skill.data, MAX_SKILL_BYTES)
        const metadata = candidateMetadata(source.text)
        if (metadata.name !== skillNameFromPath(args.path)) {
          throw Error('Skill candidate name does not match its source path')
        }
        let licenseEvidence: { path: string; blobSha: string } | null = null
        let freeLicense: string | null = null
        try {
          const license = await get(repository, exec, prefix(repository) +
            '/contents/' + licensePath + '?ref=' + commit)
          const verified = decodeFile(license.data, MAX_LICENSE_BYTES)
          licenseEvidence = { path: licensePath, blobSha: verified.sha }
          freeLicense = freeLicenseFromText(verified.text)
        } catch (error) {
          if (!(error instanceof GitHubApiFailure && error.status === 404)) throw error
        }
        const cost = inspectFreeSkillText(source.text, freeLicense)
        const approved = cost.eligible && getActiveApprovedGitHubSkills().some(entry =>
          (entry.owner + '/' + entry.repository).toLowerCase() === repository.toLowerCase() &&
          entry.commit === commit && entry.path === args.path && entry.blobSha === source.sha)
        return output(repository, skill.account, {
          reviewState: !cost.eligible ? 'excluded-paid-or-unknown' :
            approved ? 'already-approved' : 'requires-human-review',
          freeEligibility: cost,
          metadata, source: { repository, path: args.path, commit, blobSha: source.sha },
          licenseEvidence, licenseVerified: false,
          hasRequiredBundleReview: metadata.hasRelativeReferences,
          warnings: [
            ...(licenseEvidence ? [] : ['No pinned license file found']),
            ...(metadata.hasRelativeReferences ? ['Relative resources require individually pinned bundle review'] : []),
            ...(metadata.hasCommandInstructions ? ['Command instructions require explicit security review'] : []),
            ...(!cost.eligible ? ['Free-only policy excludes paid or unverified-use-cost Skills'] : []),
          ],
          approvalRequired: !approved,
          note: 'Read-only evidence, not installation, trust approval or a model-invocable Skill. Update the reviewed catalog by PR only.',
        })
      },
    },
    ...createPublicSkillDiscoverySpecs(transport),
  ]
}
