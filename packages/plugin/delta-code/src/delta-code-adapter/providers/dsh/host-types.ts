/** DSH-only Context augmentation; public Delta configuration types stay Host-independent. */
import type { HostFsService } from './host-fs.js'
import type {} from '@deepseek-ai/dsh-system-prompt'

interface DshWorkingDirectoryService {
  ensure(agent: unknown, signal?: AbortSignal): Promise<string | undefined | null>
}

declare module '@deepseek-ai/cordis' {
  interface Context {
    /** Stable subset of the Host service lookup used by this plugin. */
    get(name: 'workingDirectory'): DshWorkingDirectoryService | undefined
    get(name: 'fs'): HostFsService | undefined
    /** Defer optional prompt registration until the service exists. */
    inject(dependencies: string[], callback: (child: Context) => void): unknown
    /** Optional Host workspace provider; paths fall back to configured root. */
    workingDirectory?: DshWorkingDirectoryService
    /** Optional Host filesystem service; permission failures never enable local fallback. */
    fs?: HostFsService
  }
}
