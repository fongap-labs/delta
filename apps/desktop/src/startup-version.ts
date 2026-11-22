/** Keep a packaged version query separate from the interactive Desktop lifecycle. */

/** Only an exact version-only invocation may bypass profile and single-instance setup. */
export function isPackagedDesktopVersionQuery(argv: readonly string[], packaged: boolean): boolean {
  return packaged && argv.length === 2 && argv[1] === '--version'
}
