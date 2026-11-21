/** Host-independent GitHub repository operations and tool contract definitions. */
import type { Settings } from '../types.js'
import {
  selectGitHubAccount, validBranch, validPath, validatedCommitSha, githubActionsTargetFromUrl,
} from './github-policy.js'
import { GitHubApiFailure, GitHubWriteUncertain } from '../delta-code-adapter/contracts/github.js'
import type { DeltaGitHubTransport } from '../delta-code-adapter/contracts/github.js'
import { createGitHubSkillDiscoverySpecs } from './github-skill-discovery.js'
import { createSkillAdmissionProposalSpecs } from './github-skill-admission-proposal.js'
import { createSkillCatalogPromotionSpecs } from './github-skill-catalog-promotion.js'
import { createAutoSkillStageSpecs } from './github-skill-auto-stage.js'
import { createAutoSkillAdvanceSpecs } from './github-skill-auto-advance.js'

export interface DeltaGitHubExecution {
  readonly signal: AbortSignal
  readonly agent?: unknown
  readonly account?: string
}
export type DeltaGitHubRepositoryResolver =
  (requested: string | undefined, exec: DeltaGitHubExecution) => Promise<string>
export interface DeltaGitHubToolSpec {
  name: string
  description: string
  parameters: Record<string, any>
  execute(args: any, exec: DeltaGitHubExecution): Promise<{ repository: string; account: string; result: string }>
}

function createGitHubRequest(
  settings: Settings, transport: DeltaGitHubTransport, resolveRepo: DeltaGitHubRepositoryResolver,
) {
  return async function githubRequest(
    exec: DeltaGitHubExecution,
    repository: string | undefined,
    method: 'GET' | 'POST' | 'PUT' | 'PATCH',
    path: (repo: string) => string,
    data?: unknown,
  ): Promise<{ repository: string; account: string; data: any }> {
    const repo = await resolveRepo(repository, exec)
    const account = selectGitHubAccount(settings.githubAccounts, repo, exec.account)
    return transport.request({
      repository: repo, account: account.owner,
      method, path: path(repo), data, signal: exec.signal,
    })
  }
}

function requireWrites(settings: Settings): void {
  if (!settings.githubAllowWrites) throw new Error('GitHub writes are disabled; set github.allowWrites=true to enable')
}

const string = { type: 'string', required: true } as const
const optional = { type: 'string' } as const
function tool(
  name: string,
  description: string,
  parameters: Record<string, any>,
  action: (args: any, exec: DeltaGitHubExecution) => Promise<{ repository: string; account: string; result: string }>,
): DeltaGitHubToolSpec {
  return {
    name: 'code_github_' + name, description,
    parameters: { ...parameters, account: optional },
    execute: (args, exec) => action(args, { ...exec, account: args.account }),
  }
}

export function createGitHubToolSpecs(
  settings: Settings, transport: DeltaGitHubTransport, resolveRepo: DeltaGitHubRepositoryResolver,
): DeltaGitHubToolSpec[] {
  const githubRequest = createGitHubRequest(settings, transport, resolveRepo)
  const encodeRepo = (repo: string) => '/repos/' + repo.split('/').map(encodeURIComponent).join('/')
  const envelope = (response: {repository: string; account: string; data: any}, result: unknown) => ({
    repository: response.repository,
    account: response.account,
    result: JSON.stringify(result),
  })
  const repositoryDefault = async (exec: DeltaGitHubExecution, supplied?: string) => {
    const info = await githubRequest(exec, supplied, 'GET', encodeRepo)
    const name = info.data?.default_branch
    if (typeof name !== 'string' || !name.trim()) throw new Error('GitHub default branch unavailable')
    return { repository: info.repository, defaultBranch: validBranch(name) }
  }
  const specs: DeltaGitHubToolSpec[] = [
    tool('connection', 'Verify the selected GitHub account for a repository. Automatically use the local origin when omitted.', { repository: optional }, async (args, exec) => {
      const response = await githubRequest(exec, args.repository, 'GET', encodeRepo)
      return envelope(response, { repository: response.repository, account: response.account, private: response.data.private, permissions: response.data.permissions ?? {} })
    }),
    tool('file', 'Read one remote GitHub repository file or directory listing.', { repository: optional, path: string, ref: optional }, async (args, exec) => {
      const response = await githubRequest(exec, args.repository, 'GET', (repo) =>
        encodeRepo(repo) + '/contents/' + validPath(args.path) + (args.ref ? '?ref=' + encodeURIComponent(args.ref) : ''))
      const item = response.data
      if (Array.isArray(item)) return envelope(response, item.map((entry) => ({ name: entry.name, path: entry.path, type: entry.type })))
      if (item.size > 262144) throw new Error('GitHub file exceeds the 256 KiB response limit')
      const content = item.encoding === 'base64' ? Buffer.from(item.content.replace(/\s/g, ''), 'base64').toString('utf8') : undefined
      return envelope(response, { path: item.path, sha: item.sha, type: item.type, content })
    }),
    tool('pulls', 'List remote GitHub pull requests. Does not modify the repository.', { repository: optional, state: optional }, async (args, exec) => {
      const state = args.state ?? 'open'
      if (!['open', 'closed', 'all'].includes(state)) throw new Error('invalid PR state')
      const response = await githubRequest(exec, args.repository, 'GET', (repo) => encodeRepo(repo) + '/pulls?per_page=30&state=' + state)
      return envelope(response, response.data.map((pr: any) => ({ number: pr.number, title: pr.title, state: pr.state, draft: pr.draft, html_url: pr.html_url })))
    }),
    tool('checks', 'Inspect all check runs and commit statuses for a GitHub pull request, including links to external CI workflows.', {
      repository: optional, prNumber: { type: 'integer', required: true },
    }, async (args, exec) => {
      if (!Number.isSafeInteger(args.prNumber) || args.prNumber <= 0) throw new Error('invalid pull request number')
      const pr = await githubRequest(exec, args.repository, 'GET', (repo) => encodeRepo(repo) + '/pulls/' + args.prNumber)
      const sha = pr.data.head?.sha
      if (typeof sha !== 'string' || !/^[a-f0-9]{40}$/.test(sha)) throw new Error('pull request head SHA unavailable')
      const checks = await githubRequest(exec, pr.repository, 'GET', (repo) => encodeRepo(repo) + '/commits/' + sha + '/check-runs?per_page=100')
      const statuses = await githubRequest(exec, pr.repository, 'GET', (repo) => encodeRepo(repo) + '/commits/' + sha + '/status?per_page=100')
      return envelope(pr, {
        number: args.prNumber, head: sha,
        checks: (checks.data.check_runs ?? []).slice(0, 100).map((check: any) => ({
          name: check.name, status: check.status, conclusion: check.conclusion,
          url: check.html_url, summary: typeof check.output?.summary === 'string' ? check.output.summary.slice(0, 800) : '',
        })),
        statuses: (statuses.data.statuses ?? []).slice(0, 100).map((status: any) => ({
          context: status.context, state: status.state, description: status.description, url: status.target_url,
        })),
      })
    }),
    tool('jobs', 'Inspect GitHub Actions jobs using runId and repository, or supply runUrl from a GitHub check/status link. The URL selects the actual CI repository; use its owner-bound Delta Code Token. Use code_github_job_logs with the failed job ID.', {
      repository: optional, runId: { type: 'integer' }, runUrl: optional,
    }, async (args, exec) => {
      const link = args.runUrl === undefined ? undefined : githubActionsTargetFromUrl(args.runUrl)
      if (link && args.repository !== undefined &&
          (typeof args.repository !== 'string' || args.repository.toLowerCase() !== link.repository.toLowerCase())) {
        throw new Error('GitHub Actions URL repository differs from explicit repository')
      }
      if (link && args.runId !== undefined && args.runId !== link.runId) {
        throw new Error('GitHub Actions run URL differs from explicit runId')
      }
      const runId = args.runId ?? link?.runId
      if (!Number.isSafeInteger(runId) || runId <= 0) throw new Error('invalid workflow run ID')
      const response = await githubRequest(exec, link?.repository ?? args.repository, 'GET', (repo) =>
        encodeRepo(repo) + '/actions/runs/' + runId + '/jobs?per_page=100')
      return envelope(response, {
        runId,
        total: response.data.total_count,
        jobs: (response.data.jobs ?? []).slice(0, 100).map((job: any) => ({
          id: job.id, name: job.name, conclusion: job.conclusion, url: job.html_url,
          failedSteps: (job.steps ?? []).filter((step: any) => step.conclusion === 'failure')
            .map((step: any) => ({ name: step.name, number: step.number })),
        })),
      })
    }),
    tool('job_logs', 'Read a bounded, redacted GitHub Actions job log by jobId and repository, or by its jobUrl. The link selects the actual CI repository and matching account credential. No artifacts or annotations are needed.', {
      repository: optional, jobId: { type: 'integer' }, jobUrl: optional,
    }, async (args, exec) => {
      const link = args.jobUrl === undefined ? undefined : githubActionsTargetFromUrl(args.jobUrl)
      if (link && link.jobId === undefined) throw new Error('GitHub Actions job URL must include a job ID')
      if (link && args.repository !== undefined &&
          (typeof args.repository !== 'string' || args.repository.toLowerCase() !== link.repository.toLowerCase())) {
        throw new Error('GitHub Actions URL repository differs from explicit repository')
      }
      if (link && args.jobId !== undefined && args.jobId !== link.jobId) {
        throw new Error('GitHub Actions job URL differs from explicit jobId')
      }
      const jobId = args.jobId ?? link?.jobId
      if (!Number.isSafeInteger(jobId) || jobId <= 0) throw new Error('invalid GitHub job ID')
      const repo = await resolveRepo(link?.repository ?? args.repository, exec)
      const account = selectGitHubAccount(settings.githubAccounts, repo, exec.account)
      if (!transport.jobLogs) throw new Error('GitHub job log transport unavailable; update Delta Code provider')
      const response = await transport.jobLogs({
        repository: repo, account: account.owner, jobId, signal: exec.signal,
      })
      return envelope({ repository: response.repository, account: response.account, data: null }, {
        jobId, excerpt: response.excerpt, truncated: response.truncated,
        bytesRead: response.bytesRead,
        note: response.truncated ? 'Excerpt is incomplete; inspect the GitHub job page if the failure lies outside this excerpt.' : 'Complete job log excerpt.',
      })
    }),
    tool('branch', 'Create a feature branch from the repository default (or a supplied base) without modifying protected branches.', { repository: optional, branch: string, base: optional }, async (args, exec) => {
      requireWrites(settings)
      const branch = validBranch(args.branch)
      if (branch === 'main' || branch === 'master') throw new Error('refusing to write to the default branch')
      const info = await repositoryDefault(exec, args.repository)
      const base = validBranch(args.base ?? info.defaultBranch)
      if (branch === base || branch === info.defaultBranch) throw new Error('refusing to write to the default branch')
      const source = await githubRequest(exec, info.repository, 'GET', (repo) => encodeRepo(repo) + '/git/ref/heads/' + base.split('/').map(encodeURIComponent).join('/'))
      const result = await githubRequest(exec, source.repository, 'POST', (repo) => encodeRepo(repo) + '/git/refs', { ref: 'refs/heads/' + branch, sha: source.data.object.sha })
      return envelope(result, { branch, sha: result.data.object.sha })
    }),
    tool('commit_file',
      'Commit one UTF-8 file to a feature branch with a non-force fast-forward ref update; stale branch heads and concurrent commits fail closed.',
      { repository: optional, branch: string, path: string, content: string, message: string, expectedHead: optional },
      async (args, exec) => {
        requireWrites(settings)
        const branch = validBranch(args.branch)
        if (branch === 'main' || branch === 'master') throw new Error('refusing to write to the default branch')
        const path = validPath(args.path)
        if (typeof args.content !== 'string' || Buffer.byteLength(args.content, 'utf8') > 262144) {
          throw new Error('GitHub file content must be UTF-8 text no larger than 256 KiB')
        }
        if (typeof args.message !== 'string' || !args.message.trim()) throw new Error('commit message required')
        if (args.expectedHead !== undefined) validatedCommitSha(args.expectedHead, 'expectedHead')
        const info = await repositoryDefault(exec, args.repository)
        if (branch === info.defaultBranch) throw new Error('refusing to write to the default branch')
        const endpoint = (repo: string) => encodeRepo(repo) + '/git/ref/heads/' + branch.split('/').map(encodeURIComponent).join('/')
        const branchRef = await githubRequest(exec, info.repository, 'GET', endpoint)
        const previousHead = validatedCommitSha(branchRef.data?.object?.sha, 'branch HEAD')
        if (args.expectedHead && previousHead !== args.expectedHead) {
          throw new Error('GitHub branch HEAD differs from expectedHead; fetch latest state before editing')
        }
        // Pin all observations to the same snapshot. A changing branch must
        // not silently select a new base after the user reviewed the file.
        const fileEndpoint = (repo: string) => encodeRepo(repo) + '/contents/' + path
        const existing = await githubRequest(exec, info.repository, 'GET',
          (repo) => fileEndpoint(repo) + '?ref=' + previousHead)
          .catch((error: unknown) => {
            if (error instanceof GitHubApiFailure && error.status === 404) return null
            throw error
          })
        if (existing && (Array.isArray(existing.data) || existing.data?.type !== 'file')) {
          throw new Error('target is not a regular GitHub file')
        }
        const parent = await githubRequest(exec, info.repository, 'GET',
          (repo) => encodeRepo(repo) + '/git/commits/' + previousHead)
        const treeSha = validatedCommitSha(parent.data?.tree?.sha, 'parent tree')
        // Preserve executable bits when replacing an existing regular file.
        // Git Contents API does not expose the Git tree mode, so walk only
        // this path's ancestor trees rather than enumerating the repository.
        let fileMode = '100644'
        if (existing) {
          let currentTree = treeSha
          const segments = args.path.split('/')
          for (const [index, segment] of segments.entries()) {
            const listing = await githubRequest(exec, info.repository, 'GET',
              (repo) => encodeRepo(repo) + '/git/trees/' + currentTree)
            if (!Array.isArray(listing.data?.tree) || listing.data.truncated === true) {
              throw new Error('GitHub tree evidence is incomplete')
            }
            const entry = listing.data.tree.find((item: any) => item.path === segment)
            if (!entry) throw new Error('file missing from pinned Git tree; inspect repository state')
            if (index === segments.length - 1) {
              if (entry.type !== 'blob' || !['100644', '100755'].includes(entry.mode)) {
                throw new Error('GitHub file is not a regular blob; refusing write')
              }
              fileMode = entry.mode
            } else {
              if (entry.type !== 'tree') throw new Error('GitHub file ancestor is not a directory')
              currentTree = validatedCommitSha(entry.sha, 'nested tree')
            }
          }
        }
        // Git Trees API can create the text blob while preserving the full
        // base tree. Unlike Contents PUT, the ref update below is a separate
        // non-force fast-forward operation, so racing branches cannot overwrite.
        const tree = await githubRequest(exec, info.repository, 'POST',
          (repo) => encodeRepo(repo) + '/git/trees', {
            base_tree: treeSha,
            tree: [{ path: args.path, mode: fileMode, type: 'blob', content: args.content }],
          })
        const nextTree = validatedCommitSha(tree.data?.sha, 'new tree')
        const commit = await githubRequest(exec, info.repository, 'POST',
          (repo) => encodeRepo(repo) + '/git/commits', {
            message: args.message, tree: nextTree, parents: [previousHead],
          })
        const commitSha = validatedCommitSha(commit.data?.sha, 'new')
        try {
          await githubRequest(exec, info.repository, 'PATCH', endpoint, {
            sha: commitSha, force: false,
          })
        } catch (error) {
          if (error instanceof GitHubApiFailure &&
              (error.status === 409 || error.status === 422)) {
            throw new Error('GitHub non-fast-forward branch conflict; nothing was force-pushed; inspect HEAD and rebase before retry')
          }
          if (error instanceof GitHubWriteUncertain) {
            // Only a GET is allowed after an ambiguous PATCH outcome.
            const current = await githubRequest(exec, info.repository, 'GET', endpoint)
              .catch(() => null)
            if (current?.data?.object?.sha === commitSha) {
              return envelope(current, { path: args.path, branch, commit: commitSha, previousHead, confirmedAfterRead: true })
            }
          }
          throw error
        }
        return envelope(commit, { path: args.path, branch, commit: commitSha, previousHead })
      }),
    tool('create_pr',
      'Create or reuse one open pull request for a feature head and base. Never duplicate an existing PR or retry a write blindly.',
      { repository: optional, head: string, base: optional, title: string, body: optional },
      async (args, exec) => {
        requireWrites(settings)
        const head = validBranch(args.head)
        const info = await repositoryDefault(exec, args.repository)
        const base = validBranch(args.base ?? info.defaultBranch)
        if (head === base) throw new Error('head must differ from base')
        if (typeof args.title !== 'string' || !args.title.trim()) throw new Error('PR title required')
        const owner = info.repository.split('/')[0]!
        const findExisting = async () => {
          const response = await githubRequest(exec, info.repository, 'GET', (repo) =>
            encodeRepo(repo) + '/pulls?state=open&per_page=100&head=' +
              encodeURIComponent(owner + ':' + head) + '&base=' + encodeURIComponent(base))
          if (!Array.isArray(response.data)) throw new Error('GitHub PR listing is incomplete')
          const matching = response.data.find((pr: any) =>
            pr.head?.ref === head && pr.base?.ref === base &&
            pr.head?.repo?.full_name?.toLowerCase() === info.repository.toLowerCase())
          if (!matching) return null
          if (!Number.isSafeInteger(matching.number) || matching.number <= 0 ||
              typeof matching.html_url !== 'string') throw new Error('invalid existing GitHub PR response')
          return envelope(response, { number: matching.number, url: matching.html_url, reused: true })
        }
        const existing = await findExisting()
        if (existing) return existing
        try {
          const result = await githubRequest(exec, info.repository, 'POST', (repo) =>
            encodeRepo(repo) + '/pulls', {
              head, base, title: args.title, body: args.body ?? '',
            })
          return envelope(result, { number: result.data.number, url: result.data.html_url, reused: false })
        } catch (error) {
          // Concurrent callers can both observe no PR. GitHub rejects the
          // second with 422; an ambiguous POST might already have created it.
          // Re-read once but NEVER retry POST, and never turn a 401/403 into
          // an apparently successful operation.
          const isRecoverable = error instanceof GitHubWriteUncertain ||
            (error instanceof GitHubApiFailure &&
              (error.status === 409 || error.status === 422))
          if (isRecoverable) {
            const recovered = await findExisting().catch(() => null)
            if (recovered) return recovered
          }
          throw error
        }
      }),

    ...createGitHubSkillDiscoverySpecs(settings, transport, resolveRepo).map(spec => ({
      ...spec,
      parameters: { ...spec.parameters, account: optional },
      execute: (args: any, exec: DeltaGitHubExecution) =>
        spec.execute(args, { ...exec, account: args.account }),
    })),
  ]
  const byName = (name: string): DeltaGitHubToolSpec => {
    const match = specs.find(spec => spec.name === name)
    if (!match) throw Error('GitHub tool dependency missing: ' + name)
    return match
  }
  const writeActions = {
    file: byName('code_github_file'),
    branch: byName('code_github_branch'),
    commitFile: byName('code_github_commit_file'),
    createPr: byName('code_github_create_pr'),
  }
  const admissions = [
    ...createSkillAdmissionProposalSpecs(transport, writeActions, () => settings.githubAllowWrites),
    ...createSkillCatalogPromotionSpecs(transport, writeActions, () => settings.githubAllowWrites),
    ...createAutoSkillStageSpecs(transport, writeActions, () => settings.githubAllowWrites),
    ...createAutoSkillAdvanceSpecs(settings, transport, writeActions, byName('code_github_checks'), () => settings.githubAllowWrites),
  ]
  return [...specs, ...admissions.map(spec => ({
    ...spec,
    parameters: { ...spec.parameters, account: optional },
    execute: (args: any, exec: DeltaGitHubExecution) =>
      spec.execute(args, { ...exec, account: args.account }),
  }))]
}
