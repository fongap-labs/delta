# 性能基准测试

从仓库根目录运行 `pnpm run test:bench`，构建并执行完整性能测试。只运行已构建测试可执行 `pnpm run test:bench:built`。

按场景选择用例：`pnpm exec vitest run --config vitest.bench.config.ts tests/performance/session-open/session-open-bench.ts`。

执行完毕后查看 `tests/performance/.dsh-report/report.html`。在支持 Linux cgroup v2 与 systemd 用户管理器的环境下，可用 `pnpm run test:bench:memory 4g` 或 `8g` 验证受限内存场景。

Python SDK 另见[入门指南](../../docs/user/guide/python-sdk.md)。不同测试任务使用独立工作区及会话 ID。
