import tsconfigPaths from 'vite-tsconfig-paths'
import { defineConfig } from 'vitest/config'
import { standardDecoratorPlugin, vitestExecArgv } from './vitest.shared.ts'

/**
 * CI performance gate. Node CPU cases use compiled plain-Node workers under
 * `tests/performance/.dsh-build/`; browser cases drive built Client artifacts through
 * the shared shipped-composition Web scaffold.
 * Two files run at a time so the four-vCPU CI runner keeps both cores busy.
 * Budgets keep their reviewed CI scale and headroom; a pairing that inflates an
 * endpoint surfaces as a failing budget and is never loosened to accept it.
 */
export default defineConfig({
  plugins: [tsconfigPaths({ projects: ['./tsconfig.base.json'] }), standardDecoratorPlugin()],
  test: {
    execArgv: vitestExecArgv,
    setupFiles: ['./scripts/test-proxy-environment.ts'],
    // Clears recorded timings before the run and renders tests/performance/.dsh-report/report.html after it.
    globalSetup: ['./tests/performance/support/benchmark-scaling-report.ts'],
    include: [
      'tests/performance/**/*-bench.ts',
      'tests/performance/**/*-bench-client.ts',
    ],
    fileParallelism: true,
    maxWorkers: 2,
    testTimeout: 600_000,
    hookTimeout: 120_000,
    disableConsoleIntercept: true,
  },
})
