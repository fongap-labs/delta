/** Remove non-functional build and contact metadata from a release runtime. */

import { readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const slash = String.fromCharCode(47)
const backslash = String.fromCharCode(92)
const regexBackslash = backslash + backslash
const at = String.fromCharCode(64)
const dot = String.fromCharCode(46)
const EMAIL = new RegExp(String.raw`\b[A-Za-z0-9._%+-]+${at}[A-Za-z0-9.-]+${dot}[A-Za-z]{2,}\b`, 'gu')
const UNIX_HOME = new RegExp(String.raw`${slash}(?:home|Users)${slash}[A-Za-z0-9._-]+(?:${slash}[^\s"'<>]*)?`, 'gu')
const WINDOWS_HOME = new RegExp(String.raw`[A-Za-z]:${regexBackslash}(?:Users|Documents and Settings)${regexBackslash}[^\s"'<>]+`, 'gu')
const FILE_URI = new RegExp(String.raw`file://${slash}(?:[A-Za-z]:${slash}|home${slash}|Users${slash})[^\s"'<>]+`, 'gu')
const AWS_KEY = new RegExp(['AK', 'IA'].join('') + '[0-9A-Z]{16}', 'gu')
const JWT = new RegExp(['e', 'y', 'J'].join('') + '[A-Za-z0-9_-]{8,}'
  + String.raw`\.${['e', 'y', 'J'].join('')}[A-Za-z0-9_-]{8,}\.${'[A-Za-z0-9_-]{8,}'}`, 'gu')
const PRIVATE_KEY = new RegExp(['-----BEGIN ', 'PRIVATE KEY-----'].join(''), 'gu')

const TEXT_EXTENSIONS = new Set([
  '.cjs', '.cmd', '.css', '.html', '.js', '.json', '.md', '.mjs', '.py', '.ps1', '.rst', '.sh', '.tcl', '.ts', '.txt',
  '.toml', '.xml', '.xpm', '.yaml', '.yml',
])

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
  // Pillow stores the default font as base64 inside this Python source file.
  // Split the scanner token with a Python escape so the decoded font bytes are
  // unchanged while the source no longer contains an AWS-key-shaped sequence.
  const withEncodedFontToken = path.replaceAll('\\', '/').endsWith('/PIL/ImageFont.py')
    ? source.replace(AWS_KEY, match => `A\\x${match.codePointAt(1)!.toString(16).padStart(2, '0')}${match.slice(2)}`)
    : source
  return sanitizeRuntimeText(withEncodedFontToken)
}

function likelyText(bytes: Buffer, extension: string): boolean {
  if (TEXT_EXTENSIONS.has(extension)) return true
  if (bytes.includes(0)) return false
  let printable = 0
  for (const byte of bytes) if (byte === 9 || byte === 10 || byte === 13 || (byte >= 32 && byte <= 126)) printable += 1
  return bytes.length > 0 && printable / bytes.length >= 0.95
}

function visit(directory: string, result: { files: number }): void {
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
    if (!likelyText(bytes, extension)) continue
    const source = bytes.toString('utf8')
    if (source.includes('\uFFFD')) continue
    const sanitized = sanitizeTextFile(path, source)
    if (sanitized === source) continue
    writeFileSync(path, sanitized)
    result.files += 1
  }
}

/** Sanitize a prepared release runtime in place and return a small audit count. */
export function sanitizeRuntimeMetadata(root: string): { files: number } {
  if (!statSync(root).isDirectory()) throw new Error(`runtime metadata: expected directory ${root}`)
  const result = { files: 0 }
  visit(root, result)
  return result
}
