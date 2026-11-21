import type { GitStatusResult } from '../types.js'

/**
 * Parse the complete, NUL-delimited porcelain v1 --branch output.
 * A partial/malformed Host response must never become an apparently clean
 * working tree for code_resume or code_delivery_gate.
 */
export function parseGitStatus(raw: string): GitStatusResult {
  if (!raw || !raw.endsWith('\0')) throw new Error('invalid or incomplete Git porcelain status output')
  const records = raw.slice(0, -1).split('\0')
  if (records.length === 0 || records.some(record => !record)) {
    throw new Error('invalid or incomplete Git porcelain status records')
  }
  const header = records[0]!
  if (!header.startsWith('## ') || !header.slice(3).trim()) {
    throw new Error('missing Git porcelain branch header')
  }
  const head = header.slice(3)
  const branch = head.split('...')[0]!.split(' [')[0]!.trim()
  const aheadMatch = head.match(/\bahead (\d+)\b/)
  const behindMatch = head.match(/\bbehind (\d+)\b/)
  const ahead = aheadMatch ? Number(aheadMatch[1]) : 0
  const behind = behindMatch ? Number(behindMatch[1]) : 0
  if (!branch || !Number.isSafeInteger(ahead) || !Number.isSafeInteger(behind)) {
    throw new Error('invalid Git porcelain branch information')
  }

  const staged: string[] = []
  const unstaged: string[] = []
  const untracked: string[] = []
  for (let index = 1; index < records.length; index += 1) {
    const record = records[index]!
    if (record.length < 4 || record[2] !== ' ') {
      throw new Error('invalid Git porcelain file status record')
    }
    const x = record[0]!
    const y = record[1]!
    const file = record.slice(3)
    // Git porcelain v1 uses exactly two status columns. Reject unknown or
    // partially present pairs rather than silently dropping a changed file.
    const isSpecial = (x === '?' && y === '?') || (x === '!' && y === '!')
    const isNormal = ' MADRCUT'.includes(x) && ' MADRCUT'.includes(y) &&
      (x !== ' ' || y !== ' ')
    if (!file || !(isSpecial || isNormal)) {
      throw new Error('invalid Git porcelain status pair or path')
    }
    if (x === 'R' || x === 'C' || y === 'R' || y === 'C') {
      // In -z mode the destination comes first, followed by the old pathname.
      const source = records[++index]
      if (!source) throw new Error('incomplete git rename/copy record')
    }
    if (isSpecial) {
      // Ignored entries cannot appear with the current Git invocation;
      // conservatively count them as dirty if a Host returns them anyway.
      untracked.push(file)
    } else {
      if (x !== ' ') staged.push(x + ' ' + file)
      if (y !== ' ') unstaged.push(y + ' ' + file)
    }
  }
  return {
    branch, ahead, behind, staged, unstaged, untracked,
    clean: staged.length === 0 && unstaged.length === 0 && untracked.length === 0,
  }
}
