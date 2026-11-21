/** Validate owner-bound GitHub tool responses before any dependent Skill PR write.
 * Public source audits never pass through this credential-bearing boundary.
 */
import type { DeltaGitHubToolSpec } from './github-operations.js'

export type GitHubSkillToolResponse = Awaited<ReturnType<DeltaGitHubToolSpec['execute']>>

const TARGET = /^[A-Za-z0-9-]{1,39}\/[A-Za-z0-9_.-]{1,100}$/
const SHA = /^[a-f0-9]{40}$/

export function assertSkillTargetEnvelope(
  response: Pick<GitHubSkillToolResponse, 'repository' | 'account'>,
  targetRepository: string,
  selectedAccount?: string,
): void {
  if (!TARGET.test(targetRepository) || targetRepository.includes('..')) {
    throw new Error('Invalid Skill PR target repository')
  }
  const owner = targetRepository.split('/')[0]!.toLowerCase()
  const account = response?.account?.toLowerCase()
  // The transport authenticates the Host-native account against the target
  // owner; explicit owner bindings must always be isolated to that owner.
  if (typeof response?.repository !== 'string' ||
      response.repository.toLowerCase() !== targetRepository.toLowerCase() ||
      typeof response.account !== 'string' ||
      (account !== owner && account !== 'native') ||
      (selectedAccount !== undefined && (
        typeof selectedAccount !== 'string' ||
        account !== selectedAccount.toLowerCase()
      ))) {
    throw new Error('GitHub Skill PR target account/repository binding mismatch')
  }
}

export function parseSkillTargetReceipt(
  response: GitHubSkillToolResponse,
  targetRepository: string,
  selectedAccount?: string,
): Record<string, any> {
  assertSkillTargetEnvelope(response, targetRepository, selectedAccount)
  let result: unknown
  try { result = JSON.parse(response.result) }
  catch { throw new Error('Invalid GitHub Skill PR operation receipt') }
  if (!result || typeof result !== 'object' || Array.isArray(result)) {
    throw new Error('Invalid GitHub Skill PR operation receipt')
  }
  return result as Record<string, any>
}

export function confirmSkillTargetFile(
  response: GitHubSkillToolResponse,
  targetRepository: string,
  expectedPath: string,
  selectedAccount?: string,
): { content: string; sha: string } {
  const file = parseSkillTargetReceipt(response, targetRepository, selectedAccount)
  if (file.path !== expectedPath || typeof file.content !== 'string' ||
      typeof file.sha !== 'string' || !SHA.test(file.sha) ||
      file.type !== undefined && file.type !== 'file') {
    throw new Error('Pinned GitHub Skill PR target file is unavailable')
  }
  return { content: file.content, sha: file.sha }
}

export function confirmSkillTargetBranch(
  response: GitHubSkillToolResponse,
  targetRepository: string,
  expectedBranch: string,
  selectedAccount?: string,
): string {
  const branch = parseSkillTargetReceipt(response, targetRepository, selectedAccount)
  if (branch.branch !== expectedBranch ||
      typeof branch.sha !== 'string' || !SHA.test(branch.sha)) {
    throw new Error('GitHub Skill PR feature branch confirmation unavailable')
  }
  return branch.sha
}

export function confirmSkillTargetCommit(
  response: GitHubSkillToolResponse,
  targetRepository: string,
  expectedPath: string,
  selectedAccount?: string,
): string {
  const commit = parseSkillTargetReceipt(response, targetRepository, selectedAccount)
  if (commit.path !== expectedPath ||
      typeof commit.commit !== 'string' || !SHA.test(commit.commit)) {
    throw new Error('GitHub Skill PR commit confirmation unavailable')
  }
  return commit.commit
}

export function confirmSkillTargetPullRequest(
  response: GitHubSkillToolResponse,
  targetRepository: string,
  selectedAccount?: string,
): { number: number; url: string; reused: boolean } {
  const pr = parseSkillTargetReceipt(response, targetRepository, selectedAccount)
  if (!Number.isSafeInteger(pr.number) || pr.number <= 0 ||
      typeof pr.url !== 'string') {
    throw new Error('GitHub Skill PR confirmation unavailable')
  }
  let url: URL
  try { url = new URL(pr.url) }
  catch { throw new Error('GitHub Skill PR confirmation unavailable') }
  if (url.protocol !== 'https:' || url.hostname !== 'github.com' ||
      url.username || url.password || url.port || url.search || url.hash ||
      url.pathname.toLowerCase() !==
      ('/' + targetRepository + '/pull/' + pr.number).toLowerCase()) {
    throw new Error('GitHub Skill PR URL does not match authorized target repository')
  }
  return { number: pr.number, url: pr.url, reused: pr.reused === true }
}
