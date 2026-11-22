# 现有性能基准测试维护规范

本目录保留旧版 Harness 已存在的性能基准测试，覆盖跨包的实际用户操作路径。它属于待收敛的存量工程资产，不代表 Delta 已获得新增本地测试或执行器的中央例外。合规迁移必须按 [Action Worker 测试归属](https://github.com/fongap-labs/action-worker/blob/main/docs/05_INTEGRATION_GUIDE.md)执行：先记录测试基线，中央测试包真实运行并确认覆盖不减少，再考虑清理原件。不得将目录直接迁至新的本地 `tests/`。

## 存量用例约束

- 按被测用户路径划分场景目录，不照搬包目录结构。Host 用例沿用 `*.bench.ts`，Client 用例沿用 `*.bench.client.ts`；Worker、Fixture 和支持模块不带性能用例后缀。
- 当前私有 `@deepseek-ai/dsh-benchmarks` 工作区拥有现有测试依赖。`test:bench` 先构建工作区及 `benchmarks/.dsh-build/` Worker，再由 Vitest 编排；CPU 计时操作在纯 Node Worker 中运行，包依赖必须指向已构建的 `lib/` 入口。
- 浏览器场景通过发布形态的 Web Scaffold 驱动已构建的 Client Bundle。测试 Host 的源码构建结果与正式 Host 结果分别报告；两个动画帧仅证明存在渲染机会，不能代替硬件显示验证。每个样本使用独立浏览器与环境。
- 工作负载由审核过的固定常量合成，不能使用真实会话、用户资料、环境中的仓库或外部网络服务。
- 进程计时与保留内存在独立子进程及私有 `mkdtemp` 目录测量；同步折叠每次重新构造对象图，不污染进程全局状态。必须设置子进程上限、等待退出、异常时清理临时目录。
- 所有受预算约束的端点先通过 `support/scaling-report.ts` 中的 `recordTimings` 记录，再执行断言；`BenchmarkCase` 应说明被测路径及用户场景。能够测量时记录同区间 CPU 时间，其他情况使用有注释的存储等待占比；不要将固定等待或强制节拍计入计时端点。整个进程峰值 RSS 使用 `recordPeakMemory`。
- 校准机型位于 `support/machine-profiles.ts`；运行生成 `.dsh-report/report.html`，现有 CI 发布 `benchmark-report` 报告制品。基准机预期与 CI 时间缩放、波动余量分别记录，时间缩放不能作用于内存或无量纲比率。
- 报告应展示原始样本、聚合值、判定依据，说明使用中位数、最小值、绝对阈值还是比率；性能预算来源于审查过的常量，不允许通过环境变量覆盖。
- 场景专用工具保留在对应场景；至少有两个场景需要时才可提取至 `benchmarks/support/`。优先调用真实产品入口，禁止复制产品算法、只为测量增加生产导出或制造重复的功能断言。
- 编译 Worker 在缺少公开 Node 入口时可打包私有集成适配器，但产品包导入保持外置，使其解析到发布的构建产物。
- 所属 Agent 决策记录应保存负载、计时边界、内存端点、参考机型、备选方案及已知未覆盖范围。

## 内存压力验证

现有 `pnpm run test:bench:memory 4g` 或 `8g` 依赖 Linux swapless cgroup，在 `scripts/run-memory-pressure-bench.ts` 列明的用例上施加 4 或 8 GiB 限额，保持同一预算。调整这些存量用例需覆盖两个限额，参考[内存受限测试记录](../.agents/notes/implemented/testing/2026-10-07-memory-limited-benchmarks.md)。

上述存量规则只能用于维护既有测试，不能授权新增受中央禁止的本地测试文件。任何迁移以中央 `tests/packs/delta/` 的等价验证证据为准。
