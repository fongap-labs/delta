/** Remove non-functional build and contact metadata from a release runtime. */

import { readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { isUtf8 } from 'node:buffer'
import { join } from 'node:path'
import ts from 'typescript'

const slash = String.fromCharCode(47)
const backslash = String.fromCharCode(92)
const regexBackslash = backslash + backslash
const at = String.fromCharCode(64)
const dot = backslash + String.fromCharCode(46)
const EMAIL = new RegExp(String.raw`\b[A-Za-z0-9._%+-]+${at}[A-Za-z0-9.-]+${dot}[A-Za-z]{2,}\b`, 'gu')
const UNIX_HOME = new RegExp(String.raw`${slash}(?:home|Users)${slash}[A-Za-z0-9._-]+(?:${slash}[^\s"'<>]*)?`, 'gu')
const WINDOWS_HOME = new RegExp(String.raw`[A-Za-z]:${regexBackslash}(?:Users|Documents and Settings)${regexBackslash}[^\s"'<>]+`, 'gu')
const FILE_URI = new RegExp(String.raw`file://${slash}(?:[A-Za-z]:${slash}|home${slash}|Users${slash})[^\s"'<>]+`, 'gu')
const AWS_KEY = new RegExp('(?:' + ['AK', 'IA'].join('') + '|' + ['AS', 'IA'].join('') + ')[0-9A-Z]{16}', 'gu')
const JWT = new RegExp(['e', 'y', 'J'].join('') + '[A-Za-z0-9_-]{8,}'
  + String.raw`\.${['e', 'y', 'J'].join('')}[A-Za-z0-9_-]{8,}\.${'[A-Za-z0-9_-]{8,}'}`, 'gu')
const PRIVATE_KEY = new RegExp(['-----BEGIN ', '(?:RSA |EC |OPENSSH )?', 'PRIVATE KEY-----'].join(''), 'gu')

const TEXT_EXTENSIONS = new Set([
  '.cjs', '.cmd', '.css', '.html', '.js', '.json', '.map', '.md', '.mjs', '.py', '.ps1', '.rst', '.sh', '.tcl', '.ts', '.txt',
  '.toml', '.xml', '.xpm', '.yaml', '.yml',
])
const TEXT_NAMES = new Set(['AUTHORS', 'COPYING', 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES'])

function replacement(length: number): string {
  return 'x'.repeat(length)
}

/**
 * Scrub privacy-sensitive strings while keeping the surrounding file format
 * intact. Binary files are deliberately left alone: rewriting native payloads
 * here could invalidate signatures or silently corrupt data.
 */
export function sanitizeRuntimeText(source: string): string {
  return source
    .replace(FILE_URI, match => replacement(match.length))
    .replace(WINDOWS_HOME, match => replacement(match.length))
    .replace(UNIX_HOME, match => replacement(match.length))
    .replace(EMAIL, match => replacement(match.length))
    .replace(AWS_KEY, match => replacement(match.length))
    .replace(JWT, match => replacement(match.length))
    .replace(PRIVATE_KEY, match => replacement(match.length))
}

function sanitizeTextFile(path: string, source: string): string {
  if (/\.(?:[cm]?js|ts)$/.test(path)) {
    const tree = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true)
    const edits = new Map<number, { end: number; text: string }>()
    function comment(range: ts.CommentRange): void {
      const original = source.slice(range.pos, range.end)
      const text = sanitizeRuntimeText(original)
      if (text !== original) edits.set(range.pos, { end: range.end, text })
    }
    function visitNode(node: ts.Node): void {
      for (const range of ts.getLeadingCommentRanges(source, node.pos) ?? []) comment(range)
      for (const range of ts.getTrailingCommentRanges(source, node.end) ?? []) comment(range)
      if ((ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node))
        && !ts.isTaggedTemplateExpression(node.parent)
        && sanitizeRuntimeText(node.text) !== node.text) {
        // Encode UTF-16 code units, preserving the exact runtime string value.
        const text = '"' + node.text.split('').map(char => `\\u${char.charCodeAt(0).toString(16).padStart(4, '0')}`).join('') + '"'
        edits.set(node.getStart(tree), { end: node.end, text })
      }
      ts.forEachChild(node, visitNode)
    }
    visitNode(tree)
    let result = source
    for (const [start, edit] of [...edits].sort((a, b) => b[0] - a[0])) {
      result = result.slice(0, start) + edit.text + result.slice(edit.end)
    }
    return result
  }
  // Pillow stores the default font as base64 inside this Python source file.
  // Split the scanner token with a Python escape so the decoded font bytes are
  // unchanged while the source no longer contains an AWS-key-shaped sequence.
  const withEncodedFontToken = path.replaceAll('\\', '/').endsWith('/PIL/ImageFont.py')
    ? source.replace(AWS_KEY, match => `A\\x${match.codePointAt(1)!.toString(16).padStart(2, '0')}${match.slice(2)}`)
    : source
  return sanitizeRuntimeText(withEncodedFontToken)
}

function likelyText(bytes: Buffer, extension: string, name: string): boolean {
  if (TEXT_EXTENSIONS.has(extension) || TEXT_NAMES.has(name.toUpperCase())) return true
  if (bytes.includes(0)) return false
  let printable = 0
  for (const byte of bytes) if (byte === 9 || byte === 10 || byte === 13 || (byte >= 32 && byte <= 126)) printable += 1
  return bytes.length > 0 && printable / bytes.length >= 0.95
}

function visit(directory: string, result: { files: number; retained: Record<string, number> }): void {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) {
      if (entry.name === '__pycache__') {
        rmSync(path, { recursive: true })
        continue
      }
      visit(path, result)
      continue
    }
    if (!entry.isFile()) continue
    if (entry.name.endsWith('.pyc')) {
      rmSync(path)
      continue
    }
    if (entry.name === 'DELVEWHEEL' && directory.endsWith('.dist-info')) {
      rmSync(path)
      continue
    }
    const extension = entry.name.includes('.') ? `.${entry.name.split('.').pop()!}`.toLowerCase() : ''
    const bytes = readFileSync(path)
    const text = likelyText(bytes, extension, entry.name)
    const utf8 = isUtf8(bytes)
    const executable = /\.(?:[cm]?js|ts|py|ps1|cmd|sh)$/.test(entry.name)
    if (!text || (!utf8 && executable)) {
      // Fixed format labels only: never disclose paths or matching values.
      const kind = ['.ttf', '.otf', '.ttc', '.woff', '.woff2'].includes(extension) ? 'font'
        : ['.exe', '.dll', '.node', '.pyd', '.so', '.dylib', '.a', '.lib', '.bin', '.dat', '.pak'].includes(extension) ? 'native'
          : executable ? 'non-utf8-code' : 'other-binary'
      for (let start = 0; start < bytes.length; start += 65536) {
        const window = bytes.subarray(Math.max(0, start - 8192), start + 65536).toString('latin1')
        if (sanitizeRuntimeText(window) !== window) {
          result.retained[kind] = (result.retained[kind] ?? 0) + 1
          break
        }
      }
      continue
    }
    const encoding = utf8 ? 'utf8' : 'latin1'
    const source = bytes.toString(encoding)
    const sanitized = sanitizeTextFile(path, source)
    if (sanitized === source) continue
    writeFileSync(path, sanitized, encoding)
    result.files += 1
  }
}

/** Sanitize a prepared release runtime in place and return a small audit count. */
export function sanitizeRuntimeMetadata(root: string): { files: number; retained: Record<string, number> } {
  if (!statSync(root).isDirectory()) throw new Error(`runtime metadata: expected directory ${root}`)
  const result = { files: 0, retained: {} }
  visit(root, result)
  return result
}
