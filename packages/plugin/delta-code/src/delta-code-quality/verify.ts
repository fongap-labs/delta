/** Compatibility facade for Quality tool consumers; DSH execution is adapter-owned. */
export { createVerifyTool, prepareVerify, runPreparedVerify, redactKnownTokens } from '../delta-code-adapter/providers/dsh/verify.js'
export type { PreparedVerification } from '../delta-code-adapter/providers/dsh/verify.js'
