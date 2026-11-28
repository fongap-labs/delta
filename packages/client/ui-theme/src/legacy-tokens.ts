/**
 * Compatibility contract for token names that predate the `--delta-` prefix.
 * Host styles only declare and read `--delta-*`. Third-party plugins and
 * themes written against the earlier `--dsw-`, `--dsh-`, `--dsl-`, and `--ds-`
 * names keep working through two bridges built from this module: the
 * generated `styles/legacy-plugin-aliases.css` (legacy name reads resolve to
 * the current token) and {@link normalizeLegacyTokens} (legacy keys passed to
 * `ctx.theme.overrideTokens` or `ThemeDefinition.tokens` reach host styles).
 *
 * The frozen name list lives in `legacy-tokens.json`; it may only shrink.
 */

/** Prefixes that preceded `--delta-`. */
export const LEGACY_TOKEN_PREFIX = /^--(?:dsw|dsh|dsl|ds)-/

/** Current token prefix. */
export const TOKEN_PREFIX = '--delta-'

/**
 * Legacy names whose current token is not a plain prefix swap. The user
 * font preferences moved under `-user-` so they no longer share a name with
 * the derived `--delta-font-family-code` stack.
 */
export const LEGACY_TOKEN_RENAMES: Readonly<Record<string, string>> = Object.freeze({
  '--dsh-font-family-text': '--delta-font-family-user-text',
  '--dsh-font-family-code': '--delta-font-family-user-code',
  '--dsh-font-family-terminal': '--delta-font-family-user-terminal',
})

/**
 * Map a legacy token name to its current `--delta-` name.
 * @param name - token name as a plugin wrote it.
 * @returns the current name, or `name` unchanged when it carries no legacy prefix.
 */
export function normalizeLegacyTokenName(name: string): string {
  const renamed = LEGACY_TOKEN_RENAMES[name]
  if (renamed !== undefined) return renamed
  return LEGACY_TOKEN_PREFIX.test(name) ? name.replace(LEGACY_TOKEN_PREFIX, TOKEN_PREFIX) : name
}

/**
 * Rewrite a token dictionary's legacy keys to current names. When a legacy
 * key and its current name are both present, the current name wins
 * regardless of key order, so migrating plugins can ship both safely.
 * @param tokens - token-name dictionary.
 * @returns a new dictionary keyed by current names.
 */
export function normalizeLegacyTokens<T>(tokens: Readonly<Record<string, T>>): Record<string, T> {
  const normalized: Record<string, T> = {}
  const explicit = new Set<string>()
  for (const [name, value] of Object.entries(tokens)) {
    const current = normalizeLegacyTokenName(name)
    if (current === name) {
      normalized[current] = value
      explicit.add(current)
    } else if (!explicit.has(current)) {
      normalized[current] = value
    }
  }
  return normalized
}
