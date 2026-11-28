/** Keep Typert's programmatic compiler API separate from the repository build CLI. */
import ts from 'typescript'

const REQUIRED_FUNCTIONS = [
  'createProgram',
  'createCompilerHost',
  'createSourceFile',
  'createModuleResolutionCache',
  'createPrinter',
  'readConfigFile',
  'parseJsonConfigFileContent',
  'resolveModuleName',
  'preProcessFile',
  'transpileModule',
  'flattenDiagnosticMessageText',
] as const

/**
 * Reject an incompatible compiler API before analysis or decorator lowering.
 * @param api - compiler module resolved from this package's own dependency.
 * @throws If the supported JavaScript compiler API is unavailable.
 */
export function assertTypertCompilerApi(api: unknown): void {
  if (typeof api !== 'object' || api === null) {
    throw new Error('typert-generator: TypeScript 6 compiler API is required')
  }
  const version = Reflect.get(api, 'versionMajorMinor')
  if (typeof version !== 'string' || !version.startsWith('6.')) {
    throw new Error('typert-generator: TypeScript 6 compiler API is required; the build CLI is not an analyzer')
  }
  const missing = REQUIRED_FUNCTIONS.filter(name => typeof Reflect.get(api, name) !== 'function')
  if (missing.length > 0) {
    throw new Error(`typert-generator: incomplete compiler API: ${missing.join(', ')}`)
  }
  for (const name of ['sys', 'factory', 'SyntaxKind'] as const) {
    const value = Reflect.get(api, name)
    if (typeof value !== 'object' || value === null) {
      throw new Error(`typert-generator: incomplete compiler API: ${name}`)
    }
  }
}

assertTypertCompilerApi(ts)

// Preserve the compiler namespace types as well as the runtime value.
export { default } from 'typescript'
