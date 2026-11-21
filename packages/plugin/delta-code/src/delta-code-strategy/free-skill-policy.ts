/** Free-only Skill discovery and loading policy. Deliberately fail closed on missing evidence. */
export const FREE_ONLY_TOOL_GUIDANCE =
  'Free-only user policy: use only Skills and external tools with verifiably no-cost access. ' +
  'Exclude subscriptions, premium features, paid APIs, metered bills and unknown prices; ' +
  'free quotas are allowed only with confirmed hard no-overage limits. ' +
  'Availability in the Host tool list is not proof that use is free. ' +
  'Do not trigger a billable native Host tool when cost cannot be confirmed.'

import { createHash } from 'node:crypto'

/** Reviewed open licenses. Licensing is necessary but does not prove a tool/API is free. */
export const FREE_SKILL_LICENSES: readonly string[] = Object.freeze([
  'MIT', 'Apache-2.0', 'BSD-2-Clause', 'BSD-3-Clause', 'ISC', 'MPL-2.0',
  'GPL-2.0-only', 'GPL-3.0-only', 'LGPL-2.1-only', 'LGPL-3.0-only',
  'AGPL-3.0-only', 'Unlicense', 'CC0-1.0', 'CC-BY-4.0',
])

export function isFreeSkillLicense(value: unknown): boolean {
  return typeof value === 'string' && FREE_SKILL_LICENSES.includes(value)
}

/** Infer only strongly identifiable free licenses from a pinned license body. */
export function freeLicenseFromText(text: string): string | null {
  const header = text.slice(0, 1024)
  const identified = /^SPDX-License-Identifier:\s*([A-Za-z0-9-.]+)\s*$/m.exec(header)?.[1]
  if (identified && isFreeSkillLicense(identified)) return identified
  if (/\bMIT License\b/i.test(header) && /permission is hereby granted/i.test(text.slice(0, 8192))) return 'MIT'
  if (/\bApache License\b/i.test(header) && /Version 2\.0/i.test(header)) return 'Apache-2.0'
  if (/\bBSD 3-Clause License\b/i.test(header)) return 'BSD-3-Clause'
  if (/\bBSD 2-Clause License\b/i.test(header)) return 'BSD-2-Clause'
  if (/\bISC License\b/i.test(header)) return 'ISC'
  if (/\bMozilla Public License\b/i.test(header) && /Version 2\.0/i.test(header)) return 'MPL-2.0'
  return null
}

/** Only affirmative payment requirements. Generic mentions of "cost" are not evidence. */
const PAID_REQUIREMENTS = [
  /\b(?:must|need(?:s)?\s+to|requires?)\s+(?:purchase|buy)\s+(?:api\s+)?(?:credits?|tokens?|subscription|access|licenses?)\b/i,
  /\b(?:requires?|needs?|must\s+(?:use|have|purchase|buy)|only\s+available\s+(?:with|on))\b[^\n.!?]{0,90}\b(?:paid|premium|pro\s+plan|enterprise\s+plan|subscription|billing\s+account|payment|credit\s+card|purchased?\s+credits?)\b/i,
  /\b(?:paid|premium|subscription(?:-based)?|pro-only|enterprise-only|pay-per-use|pay-as-you-go|usage-based\s+billing)\b[^\n.!?]{0,90}\b(?:api|tool|service|plan|account|access|key|token|credits?)\b/i,
  /(?:\u5fc5\u987b|\u9700\u8981|\u8981\u6c42|\u4ec5\u9650|\u4f9d\u8d56)[^\n\u3002\uff01\uff1f]{0,40}(?:\u4ed8\u8d39|\u6536\u8d39|\u8ba2\u9605|\u5145\u503c|\u8d2d\u4e70\u989d\u5ea6|\u8d2d\u4e70\u79ef\u5206|\u4ed8\u8d39\u63a5\u53e3|\u4ed8\u8d39API|\u4ed8\u8d39\s*API)/,
  /(?:\u4ed8\u8d39|\u6536\u8d39|\u8ba2\u9605|\u5145\u503c|\u8d2d\u4e70\u989d\u5ea6)[^\n\u3002\uff01\uff1f]{0,30}(?:\u624d\u80fd|\u65b9\u53ef|\u670d\u52a1|\u63a5\u53e3|\u5de5\u5177|API|api)/,
]

export function paidSkillRequirement(text: string): string | null {
  const bounded = text.slice(0, 64 * 1024)
  for (const pattern of PAID_REQUIREMENTS) {
    const match = bounded.match(pattern)
    if (match) return match[0].slice(0, 120)
  }
  return null
}

/**
 * Relative Markdown destinations are secondary Skill instructions, regardless
 * of whether the author wrote "refactoring.mini.md" or "./references/foo.md".
 * A path-only discovery/audit must not claim self-contained approval.
 * Future bundle admission can independently pin and verify those references.
 */
export function referencedLocalSkillFiles(markdown: string): string[] {
  const names = new Set<string>()
  const bounded = markdown.slice(0, 64 * 1024)
  for (const match of bounded.matchAll(/!?\[[^\]\r\n]{0,160}\]\(\s*<?([^)\s>]+)>?(?:\s+["'][^"'\r\n]{0,120}["'])?\s*\)/g)) {
    const ref = match[1]!
    if (ref.startsWith('#') || /^(?:https?:\/\/|mailto:|data:)/i.test(ref)) continue
    names.add(ref)
  }
  return [...names]
}

/** Secondary references, scripts or assets are never implicitly executable or free. */
export function hasUnverifiedSkillDependencies(markdown: string): boolean {
  return referencedLocalSkillFiles(markdown).length > 0 ||
    /(?:^|\s)(?:\.\.?\/)?(?:references|scripts|assets)\//im.test(markdown)
}

/**
 * A bundle may explicitly review Markdown links, but such links cannot hide
 * additional bare relative paths or script/asset dependencies in its body.
 * Only the approved links are masked; remaining references fail closed.
 * This applies equally at public audit, CI admission and native Host loading.
 */
export function hasUnlinkedSkillDependencies(markdown: string): boolean {
  const withoutLinks = markdown.replace(
    /!?\[[^\]\r\n]{0,160}\]\(\s*<?([^\s)>]+)>?(?:\s+["'][^"'\r\n]{0,120}["'])?\s*\)/g,
    '',
  )
  // File names next to retrieval instructions are also dependencies, even
  // when the author omitted Markdown link syntax.
  const unlinkedDocument = /(?:\b(?:read|open|see|load|consult|follow|include)\s+|(?:\u9605\u8bfb|\u67e5\u770b|\u52a0\u8f7d|\u53c2\u7167|\u53c2\u8003)\s*)(?:["'`]?)(?:\.{1,2}\/)?(?:[A-Za-z0-9._-]+\/)*[A-Za-z0-9._-]+\.(?:md|txt)\b/iu
  return hasUnverifiedSkillDependencies(withoutLinks) ||
    /(?:^|[\s("'])\.{1,2}\//im.test(withoutLinks) ||
    unlinkedDocument.test(withoutLinks)
}

/** Structural verification for a pinned GitHub blob; never run the Markdown as commands. */
export function decodePinnedSkillBlob(data: unknown, expectedSha: string, maximum = 64 * 1024): string {
  const value = data as { encoding?: unknown; content?: unknown; size?: unknown; sha?: unknown }
  if (!value || value.encoding !== 'base64' || typeof value.content !== 'string' ||
    typeof value.size !== 'number' || !Number.isSafeInteger(value.size) ||
    value.size <= 0 || value.size > maximum || value.sha !== expectedSha) {
    throw Error('Incomplete or oversized pinned Skill blob evidence')
  }
  const compact = value.content.replace(/\s/g, '')
  if (!/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(compact)) {
    throw Error('Invalid base64 Skill blob evidence')
  }
  const bytes = Buffer.from(compact, 'base64')
  if (bytes.length !== value.size) throw Error('Pinned Skill blob size mismatch')
  const calculated = createHash('sha1').update('blob ' + bytes.length + '\0').update(bytes).digest('hex')
  if (calculated !== expectedSha) throw Error('Pinned Skill blob SHA mismatch')
  return new TextDecoder('utf-8', { fatal: true }).decode(bytes)
}

export function inspectFreeSkillText(
  markdown: string, license: unknown,
): { readonly eligible: boolean; readonly status: 'free-evidence' | 'paid' | 'unknown'; readonly reason: string } {
  if (!isFreeSkillLicense(license)) return { eligible: false, status: 'unknown', reason: 'Free license not verified' }
  const charge = paidSkillRequirement(markdown)
  if (charge) return { eligible: false, status: 'paid', reason: 'Skill requires a paid service or subscription' }
  if (hasUnverifiedSkillDependencies(markdown)) {
    return { eligible: false, status: 'unknown', reason: 'Relative Skill dependencies need free-use review' }
  }
  return { eligible: true, status: 'free-evidence', reason: 'Open license and no detected paid prerequisites; external service costs are not guaranteed' }
}
