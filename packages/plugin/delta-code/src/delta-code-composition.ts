import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import type { Settings } from './types.js'
import { mountDeltaCodeFeatures } from './delta-code-kernel/feature.js'
import { dshToolRegistrar } from './delta-code-adapter/providers/dsh/tool-registration.js'
import {
  createDshCodingCoreTools, createDshCodingCommandTools, createDshQualityTools,
  createDshRepositoryRemoteTools, createDshRepositoryCommandTools,
} from './delta-code-adapter/providers/dsh/feature-tools.js'
import { createGitHubTransport } from './delta-code-adapter/providers/github/api.js'
import { registerGatewayRetryCoordinator } from './delta-code-adapter/providers/dsh/gateway-retry-coordination.js'
import { deltaCodeCodingWithoutCommands, deltaCodeCodingCommands } from './delta-code-coding/index.js'
import { deltaCodeStrategy } from './delta-code-strategy/index.js'
import { registerAutomaticCodingRouter } from './delta-code-adapter/providers/dsh/auto-routing.js'
import { registerCodingSkills } from './delta-code-adapter/providers/dsh/skills.js'
import { registerApprovedGitHubSkills } from './delta-code-adapter/providers/dsh/github-skills.js'
import { registerDeltaCodePrompt } from './delta-code-adapter/providers/dsh/prompt.js'
import { deltaCodeQuality } from './delta-code-quality/index.js'
import { registerNativeResume } from './delta-code-quality/resume.js'
import { observeNativeDelegation } from './delta-code-adapter/providers/dsh/native-delegation.js'
import { deltaCodeRepositoryRemote, deltaCodeRepositoryCommands } from './delta-code-repository/index.js'
import { deltaCodeSupport } from './delta-code-support/index.js'
import { ExecutionStats } from './delta-code-support/execution-stats.js'
import { UserTaskContext } from './delta-code-strategy/intent-contract.js'
import { createExecutionNextTool } from './delta-code-adapter/providers/dsh/execution-next.js'
import { registerNativeToolStats } from './delta-code-adapter/providers/dsh/native-tool-stats.js'
import { registerNativeCompactionMarker } from './delta-code-adapter/providers/dsh/compaction-marker.js'

/** Only this composition root knows both Host adapters and feature implementations. */
export function mountDeltaCode(ctx: Context, settings: Settings): () => void {
  if (!settings.enabled) return () => {}
  const resume = registerNativeResume(ctx)
  const compaction = registerNativeCompactionMarker(ctx)
  const delegation = observeNativeDelegation(ctx)
  const stats = new ExecutionStats()
  const taskContext = new UserTaskContext()
  const releases: Array<() => void> = [
    () => resume.dispose(), () => compaction.dispose(), () => delegation.dispose(), () => stats.clear(),
    () => taskContext.clear(),
  ]
  try {
    // The lightweight strategy and diagnostic tools never need subprocess.
    const coreFeatures = [
      deltaCodeStrategy({
        canDelegate: delegation.canDelegate,
        taskContext,
        registerSkills: () => {
          const releaseLocal = registerCodingSkills(ctx)
          try {
            const releaseApproved = registerApprovedGitHubSkills(ctx)
            return () => { releaseApproved(); releaseLocal() }
          } catch (error) {
            releaseLocal()
            throw error
          }
        },
        registerRouter: () => registerAutomaticCodingRouter(ctx, () => settings, undefined, resume, taskContext, stats),
        registerPrompt: () => registerDeltaCodePrompt(ctx, settings),
      }),
      deltaCodeCodingWithoutCommands(createDshCodingCoreTools(ctx, settings)),
      deltaCodeRepositoryRemote(createDshRepositoryRemoteTools(
        ctx, settings, createGitHubTransport(settings.githubAccounts, ctx))),
      deltaCodeQuality([createExecutionNextTool(resume, stats, compaction)]),
      deltaCodeSupport(stats),
    ] satisfies Array<import('./delta-code-kernel/feature.js').DeltaCodeFeature<ToolDefinition>>
    releases.push(mountDeltaCodeFeatures(coreFeatures, dshToolRegistrar(ctx, stats)))
    releases.push(registerNativeToolStats(ctx, stats))

    // Cordis independently mounts/unmounts this group when subprocess becomes
    // available or changes. The child Context owns subprocess access; the
    // parent Context retains the already-injected tool registration service.
    let isCommandActive = true
    const commandDisposers = new Set<() => void>()
    const injected = ctx.inject(['subprocess'], child => {
      if (!isCommandActive) return
      const commandFeatures = [
        deltaCodeCodingCommands(createDshCodingCommandTools(child, settings)),
        deltaCodeQuality(createDshQualityTools(child, settings, resume)),
        deltaCodeRepositoryCommands(createDshRepositoryCommandTools(child, settings)),
      ] satisfies Array<import('./delta-code-kernel/feature.js').DeltaCodeFeature<ToolDefinition>>
      const unmount = mountDeltaCodeFeatures(commandFeatures, dshToolRegistrar(ctx, stats))
      commandDisposers.add(unmount)
      return () => {
        commandDisposers.delete(unmount)
        unmount()
      }
    })
    releases.push(() => {
      isCommandActive = false
      for (const unmount of commandDisposers) unmount()
      commandDisposers.clear()
      const stop = (injected as { dispose?: () => Promise<void> } | null)?.dispose
      if (typeof stop === 'function') void Promise.resolve(stop.call(injected)).catch(() => {
        ctx.logger?.warn?.('delta-code: subprocess injector disposal failed')
      })
    })
    releases.push(registerGatewayRetryCoordinator(ctx, () => settings.enabled))
  } catch (error) {
    for (const release of releases.reverse()) { try { release() } catch {} }
    throw error
  }
  let isActive = true
  return () => {
    if (!isActive) return
    isActive = false
    for (const release of releases.reverse()) { try { release() } catch {} }
  }
}
