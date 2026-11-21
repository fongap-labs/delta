/** Stable Quality entrypoint; Host session projection belongs to the DSH adapter. */
export { foldNativeResume } from './resume-policy.js'
export type { NativeResumeReader } from './resume-policy.js'
export { registerNativeResume, createNativeResumeTool } from '../delta-code-adapter/providers/dsh/resume.js'
