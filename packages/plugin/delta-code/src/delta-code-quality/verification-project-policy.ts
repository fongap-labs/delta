/** Pure project classification and declared-script selection; Host reads remain in verify.ts. */
import { isAllowedVerificationScript, type VerificationProject } from './verify-presets.js'

export type DetectedVerificationProject = Exclude<VerificationProject, 'auto'>

const MANIFESTS: readonly (readonly [DetectedVerificationProject, string])[] = [
  ['node', 'package.json'], ['rust', 'Cargo.toml'],
  ['python', 'pyproject.toml'], ['python', 'requirements.txt'],
  ['go', 'go.mod'],
]
const LOCKFILES = ['pnpm-lock.yaml', 'yarn.lock', 'bun.lockb', 'bun.lock', 'Cargo.lock'] as const

/** Candidate filenames only: listing them never authorizes Host FS reads. */
export const verificationMetadataFiles: readonly string[] = [
  ...new Set([...MANIFESTS.map(([, name]) => name), ...LOCKFILES]),
]

export function detectVerificationProjects(files: ReadonlySet<string>): DetectedVerificationProject[] {
  const found = new Set<DetectedVerificationProject>()
  for (const [project, filename] of MANIFESTS) {
    if (files.has(filename)) found.add(project)
  }
  return [...found]
}

export function selectVerificationPackageManager(files: ReadonlySet<string>): 'npm' | 'pnpm' | 'bun' | 'yarn' {
  if (files.has('pnpm-lock.yaml')) return 'pnpm'
  if (files.has('yarn.lock')) return 'yarn'
  if (files.has('bun.lockb') || files.has('bun.lock')) return 'bun'
  return 'npm'
}

/** Only non-empty, allowlisted package.json script declarations qualify. */
export function parseDeclaredNodeVerificationScripts(pkg: unknown): string[] {
  if (!pkg || typeof pkg !== 'object' || Array.isArray(pkg)) throw new Error('invalid package.json')
  const scripts = (pkg as { scripts?: Record<string, unknown> }).scripts
  if (scripts && (typeof scripts !== 'object' || Array.isArray(scripts))) {
    throw new Error('package.json scripts must be an object')
  }
  const allowed: string[] = []
  for (const [name, command] of Object.entries(scripts ?? {})) {
    if (isAllowedVerificationScript(name) && typeof command === 'string' && command.trim()) {
      allowed.push(name)
    }
  }
  return allowed
}
