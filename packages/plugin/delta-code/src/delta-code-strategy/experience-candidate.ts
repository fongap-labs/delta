import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import { defineTool } from '@fongap-labs/delta-plugin-shared'

/**
 * A reproducible lesson is reviewable data, not a Skill registration.
 * Automated admission may consume this record only after independently
 * verifying CI evidence, source provenance, free use and Host permissions.
 */
export interface ExperienceEvidenceInput {
  readonly runUrl: string
  readonly verifies: string
}

export interface ExperienceCandidateInput {
  readonly title: string
  readonly problem: string
  readonly rootCause: string
  readonly fixPattern: string
  readonly appliesWhen: readonly string[]
  readonly limitations: readonly string[]
  readonly sourceRepository: string
  readonly sourceCommit: string
  readonly evidence: readonly ExperienceEvidenceInput[]
}

export interface ExperienceCandidate {
  readonly schemaVersion: 1
  readonly id: string
  readonly title: string
  readonly problem: string
  readonly rootCause: string
  readonly fixPattern: string
  readonly appliesWhen: string[]
  readonly limitations: string[]
  readonly source: { repository: string; commit: string }
  readonly evidence: Array<{ runUrl: string; verifies: string }>
  /** A submitted link never attests to a successful run or matching SHA. */
  readonly stage: 'pending-automated-evidence-check'
}

const MAX_TEXT = 1_000
const SECRET_HINT = /(?:gh[pousr]_[a-z0-9]{12,}|github_pat_[a-z0-9_]{12,}|AKIA[A-Z0-9]{16}|(?:bearer|password|secret|token|api[-_ ]?key)\s*[:=]\s*\S+)/i
const REPOSITORY = /^([a-z0-9](?:[a-z0-9-]{0,38}))\/([a-z0-9_.-]{1,100})$/i
const ACTION_RUN = /^https:\/\/github\.com\/([a-z0-9-]{1,39})\/([a-z0-9_.-]{1,100})\/actions\/runs\/([1-9][0-9]*)(?:\/job\/[1-9][0-9]*)?$/i

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function requiredText(value: unknown, field: string): string {
  if (typeof value !== 'string' || value.length > MAX_TEXT ||
    !value.trim() || /[\u0000-\u001f\u007f]/.test(value) || SECRET_HINT.test(value)) {
    throw new Error('invalid or sensitive experience field: ' + field)
  }
  return value.trim()
}

function requiredList(value: unknown, field: string): string[] {
  if (!Array.isArray(value) || value.length < 1 || value.length > 8) {
    throw new Error('expected 1..8 experience entries for ' + field)
  }
  const items = value.map((item: unknown) => requiredText(item, field))
  if (new Set(items).size !== items.length) {
    throw new Error('duplicate experience entries for ' + field)
  }
  return items
}

function repositoryName(value: unknown): string {
  if (typeof value !== 'string' || !REPOSITORY.test(value)) {
    throw new Error('invalid experience source repository')
  }
  const [owner, repo] = value.split('/')
  if (!owner || !repo || repo === '.' || repo === '..' || repo.endsWith('.git')) {
    throw new Error('invalid experience source repository')
  }
  return owner.toLowerCase() + '/' + repo.toLowerCase()
}

/**
 * Converts task evidence into a bounded, whitelist-only candidate.
 * Does not fetch, execute, approve, store, or install third-party content.
 * The returned pending stage is consumed by separate automated CI admission.
 */
export function buildExperienceCandidate(input: unknown): ExperienceCandidate {
  if (!isRecord(input)) throw new Error('invalid experience candidate input')
  const repository = repositoryName(input.sourceRepository)
  if (typeof input.sourceCommit !== 'string' || !/^[a-f0-9]{40}$/.test(input.sourceCommit)) {
    throw new Error('expected immutable 40-character source commit SHA')
  }
  if (!Array.isArray(input.evidence) || input.evidence.length < 1 || input.evidence.length > 5) {
    throw new Error('expected 1..5 CI evidence references')
  }
  const evidence = input.evidence.map((item: unknown) => {
    if (!isRecord(item) || typeof item.runUrl !== 'string' ||
      !ACTION_RUN.test(item.runUrl)) {
      throw new Error('invalid GitHub Actions evidence URL')
    }
    const matches = ACTION_RUN.exec(item.runUrl)
    const runOwner = matches?.[1]
    const runRepo = matches?.[2]
    const runId = matches?.[3]
    if (!runOwner || !runRepo || !runId || runRepo === '.' || runRepo === '..') {
      throw new Error('invalid GitHub Actions evidence URL')
    }
    return {
      runUrl: 'https://github.com/' + runOwner.toLowerCase() +
        '/' + runRepo.toLowerCase() + '/actions/runs/' + runId +
        (item.runUrl.includes('/job/') ? '/job/' + item.runUrl.split('/job/')[1] : ''),
      verifies: requiredText(item.verifies, 'evidence.verifies'),
    }
  })
  if (new Set(evidence.map(item => item.runUrl)).size !== evidence.length) {
    throw new Error('duplicate CI evidence URL')
  }

  return {
    schemaVersion: 1,
    id: repository.replace('/', '-') + '-' + input.sourceCommit.slice(0, 12),
    title: requiredText(input.title, 'title'),
    problem: requiredText(input.problem, 'problem'),
    rootCause: requiredText(input.rootCause, 'rootCause'),
    fixPattern: requiredText(input.fixPattern, 'fixPattern'),
    appliesWhen: requiredList(input.appliesWhen, 'appliesWhen'),
    limitations: requiredList(input.limitations, 'limitations'),
    source: { repository, commit: input.sourceCommit },
    evidence,
    stage: 'pending-automated-evidence-check',
  }
}

export function createExperienceCandidateTool(): ToolDefinition {
  return defineTool({
    name: 'code_experience_candidate',
    description: 'Structure a reusable engineering lesson with immutable source revision and GitHub Actions evidence. Pure read-only validation; evidence is unverified until independent CI inspection. This is an input to governed automatic admission, not an authority to execute or load a Skill.',
    parameters: {
      title: { type: 'string', required: true },
      problem: { type: 'string', required: true },
      rootCause: { type: 'string', required: true },
      fixPattern: { type: 'string', required: true },
      appliesWhen: { type: 'array', required: true, items: { type: 'string' } },
      limitations: { type: 'array', required: true, items: { type: 'string' } },
      sourceRepository: { type: 'string', required: true },
      sourceCommit: { type: 'string', required: true },
      evidence: { type: 'array', required: true, items: {
        type: 'object', additionalProperties: false, properties: {
          runUrl: { type: 'string', required: true },
          verifies: { type: 'string', required: true },
        },
      } },
    },
    output: {
      schema: { type: 'object', additionalProperties: false, properties: {
        schemaVersion: { type: 'integer', required: true },
        id: { type: 'string', required: true },
        title: { type: 'string', required: true },
        problem: { type: 'string', required: true },
        rootCause: { type: 'string', required: true },
        fixPattern: { type: 'string', required: true },
        appliesWhen: { type: 'array', required: true, items: { type: 'string' } },
        limitations: { type: 'array', required: true, items: { type: 'string' } },
        source: { type: 'object', required: true, additionalProperties: false, properties: {
          repository: { type: 'string', required: true },
          commit: { type: 'string', required: true },
        } },
        evidence: { type: 'array', required: true, items: {
          type: 'object', additionalProperties: false, properties: {
            runUrl: { type: 'string', required: true },
            verifies: { type: 'string', required: true },
          },
        } },
        stage: { type: 'string', required: true },
      } },
      render(_args, value) {
        return [{ type: 'text', text: JSON.stringify(value) }]
      },
    },
    execute: async args => buildExperienceCandidate(args),
  })
}
