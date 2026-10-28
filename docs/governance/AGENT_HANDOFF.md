# 开发交接：Delta / Delta Suite 剩余工作

本文是给其他开发 Agent 的交接板。它只记录**当前事实、任务边界、验收条件和停止条件**；产品决定仍以 ADR 与仓库治理文档为准。

阅读顺序：

1. 本仓库 `CLAUDE.md` 与 `AGENTS.md`
2. Action Worker 的 `docs/01_SHARED_GOVERNANCE.md`、`03_NAMING_CONVENTIONS.md`、`04_CHANGELOG_CONVENTIONS.md`
3. 本文件
4. 任务涉及的 ADR（见每项任务）

`delta` 与 `delta-suite` 是两个独立仓库。不要把 Suite 代码复制进 Foundation，也不要在 Suite 里导入 Foundation 内部模块。

---

## 1. 当前状态（2026-10-27）

### 已合并

| 变更 | 仓库 | 合并提交 |
|---|---|---|
| 分析保证等级、结果合同 v2、报告门禁、移除 Foundation R 统计 | `fongap-labs/delta` | `124606f` |
| Suite Validate Pack、干净切割、只依赖 `delta-worker-sdk` | `fongap-labs/delta-suite` | `6c0dddb` |
| Delta Pro edition 标记 + 关于界面 | `delta` / `delta-suite` | `a7660ff` / `251fb4e` |
| Suite 跟随 Foundation 到 `a7660ff` | `delta-suite` | `c12e387` |

### 进行中

| 任务 | 仓库 | PR | 状态 |
|---|---|---|---|
| 测试包允许顶层 `validation/` | `fongap-labs/action-worker` | #1079 | 已开，等待 CI/合并 |
| 方法验证登记表 + CI 棘轮 | `fongap-labs/delta-suite` | #70 | 已开；被 #1079 阻塞 |

#1079 合并后，重跑 Suite #70 的中央 CI。两项都合并后，P0 完成。

---

## 2. 任务板

任务优先级从高到低。一个 Agent 一次只认领一项，并在 PR 描述中写明任务编号。

### P0：解除验证门禁的阻塞

**P0-1 合并 action-worker #1079**

- 目标：让 Suite 仓库的顶层 `validation/` 目录通过中央测试包边界检查。
- 改动：`tests/packs/delta-suite/test_suite_contract.py` 的顶层允许集合加入 `validation`。
- 验收：
  - `node tests/run-pack.mjs delta-suite <suite-checkout> --filter=test_suite_contract` 通过；
  - `... --filter=test_suite_hygiene` 通过；
  - 中央 CI 与 PR 治理通过；
  - 不删除或放宽任何 authority/import 检查。
- 不要做：不要为了通过检查把 `validation/` 改到 `docs/` 或 `advanced/` 下；目录位置是 Suite 的验证登记约定。

**P0-2 合并 Suite #70**

- 目标：验证登记表与 CI 棘轮生效。
- 依赖：P0-1 合并。
- 验收：
  - 中央 CI 通过；
  - `python scripts/check_validation_claims.py` 输出 3 个方法均为 `exploratory`；
  - `validated` / `attested` 没有报告和证据哈希时检查失败；
  - 三个 CMC 方法的结果仍为 `exploratory`，formal 报告仍拒绝它们。

### P1：建立 R 4.6.0 验证环境

**P1-1 让参考测试真正运行，而不是 skip**

- 现状：本机与常规 Suite CI 没有固定 R 4.6.0，`tests/cmc/test_r_engine.py` 与 `tests/cmc/test_shelf_life_reference.py` 会 skip。skip 不是验证证据。
- 目标：提供一个可重复的验证运行方式，记录 R 版本、方法包版本、提交 SHA、输入哈希与测试输出。
- 建议做法（任选其一，需在 PR 中说明）：
  - 专用验证脚本 `projects/delta/scripts/run_validation.py`，启动时强制校验 `Rscript --version` 为 4.6.0，缺失即失败；
  - 或在中央 CI 增加独立的验证分片，安装 R 4.6.0 后运行 `tests/cmc`。
- 验收：
  - R 版本不是 4.6.0 时明确失败，不退化为 skip；
  - 记录文件包含 R 版本、`deltasuite.cmc` 版本、提交 SHA、测试命令与结果；
  - 至少 f2 与 shelf-life 的参考测试真实通过；
  - 验证日志作为证据归档，路径写入 `validation/README.md` 或报告。
- 不要做：不要因为 CI 没装 R 就把测试删掉或永久 skip；不要在 Foundation 重新加入 R。

### P2：验证三个 CMC 方法（可拆成 3 个子任务）

顺序建议：`f2` → `shelf_life` → `poolability`。每次只提升一个方法。

**P2-1 `cmc.dissolution.f2`**

- 目标：用独立参考值与参考数据验证 f2 实现，形成报告，然后把 `assurance` 提升为 `validated`。
- 必须有的证据：
  - 参考数据与来源/许可说明；
  - 独立期望值（不得由被测 R 代码自己生成）；
  - 在 R 4.6.0 上运行的结果；
  - 输入文件、期望值与环境摘要的 SHA-256；
  - 报告编号、范围、签发日期、审查人。
- 验收：
  - `validation/cmc.dissolution.f2/` 下有方案、数据、期望值与报告；
  - registry 写入完整 `validation` 块，证据哈希匹配；
  - 结果文件写出 `validation.report_id` / `scope` / `issued_at` / `evidence`；
  - Foundation `report_check` formal 模式接受一个绑定该结果的测试报告；
  - `projects/delta/scripts/check_validation_claims.py` 通过。
- 不要做：不要在报告完成前改 `RESULT_ASSURANCE`；不要用单元测试或 mock 结果作为验证证据。

**P2-2 `cmc.stability.shelf_life`**

- 目标与证据要求同 P2-1。
- 额外要求：覆盖 `pooled` 与 `separate` 两种模型、单侧置信界限、外推标记与“时间 0 已超限”分支。
- 现有解析式参考测试（完美线性数据，交点 7.5）只能证明交点逻辑，不能单独构成完整验证。

**P2-3 `cmc.stability.poolability`**

- 目标与证据要求同 P2-1。
- 额外要求：覆盖 `slopes` 与 `intercepts`、自由度、RSS、F 值与 p 值；参考值来源必须独立于被测 R 运行。
- 现有测试中的 statsmodels 参考值需要在报告中写明版本与计算方式。

**P2 的共同约束**

- 代码中的保证等级目前是一个常量 `RESULT_ASSURANCE`。提升任一方法前，先把它拆成按方法区分的常量或参数，不要用一次全局修改把三个方法一起标成 validated。
- `delta.result` v2 的 `validation` 字段必须与 registry 一致。
- 验证报告不得写“符合 Part 11”或“获监管机构接受”。

### P3：Foundation Python Explore Pack

**P3-1 设计并实现 Explore Pack MVP**

- 背景：Foundation 已移除 R 统计，当前没有本地分析引擎；统计页只显示说明。ADR-0080 要求 Explore Pack 用固定 Python 环境，只产出 `exploratory` 结果。
- MVP 范围建议：
  - 描述统计、分组比较、相关、线性回归；
  - 受控 CSV 读取与结果 v2 写出；
  - 结果标注 `assurance: exploratory`，明确不能进入 formal 报告；
  - 统计页在没有 Pack 时继续显示说明。
- 验收：
  - Python 运行时版本与依赖锁定，启动时校验；
  - 结果通过 Foundation `analysis-result.schema.json` 校验；
  - formal 报告拒绝 exploratory 结果；
  - 不依赖 R，不导入 Suite；
  - 有 ADR-0080 的实现记录与用户文档。
- 不要做：不要把 Python 解释器或第三方包静默塞进安装包；不要复用 Suite 的 R 方法实现。

### P4：发布

**P4-1 发布 Delta 与 Suite**

- 前置：P0 完成；P1/P2 是否包含在本次发布由所有者决定。
- 版本建议（需所有者确认）：Foundation `0.2.0-beta.1`（含破坏性变更）；Suite 对应版本。
- 流程：使用 Action Worker 的 `手动 · 发布应用`，填写 `source_repository` 与 `requested_version`；等待发布 PR、门禁、构建、签名与 Release。
- 验收：
  - `CHANGELOG.md` 版本小节正确，`changelog.d/` 已汇总条目被删除；
  - 发布 tag 与 Release 产物存在；
  - 安装冒烟测试通过，包括 `product-edition.json`；
  - 发布后重新下载校验产物。
- 不要做：不要在 CI 红、changelog 不一致或所有者未确认版本时启动发布。

### P5：后续与清理（非阻塞）

- **P5-1 ADR 历史标记**：ADR-0054/0068/0070/0071/0073/0074/0078 的正文仍描述 R 统计。保留历史，但在标题下加“已被 ADR-0081 取代”的说明，避免 Agent 把旧叙述当现状。
- **P5-2 Suite 报告模板 + Foundation 集成测试**：验证 `report_render` / `report_check` formal 模式对 Suite 结果文件的端到端行为。
- **P5-3 DOE 方法包与设计空间**：按 Suite 路线图推进；不要回到 Foundation。
- **P5-4 文档清理**：`docs/governance/OPEN_ITEMS.md`、README、扩展加载说明中与当前实现不一致的条目。

---

## 3. 跨仓库治理规则

### 3.1 边界

- Foundation：`fongap-labs/delta`。拥有 Runtime、Approval、Ledger、Artifact、Report Gate、Workspace 与状态权威。
- Suite：`fongap-labs/delta-suite`。只通过公开 `delta-worker-sdk`、JSON Manifest 与能力合同接入。
- 禁止：
  - Suite 导入 `integrations.*`、`packages.*`、`core.*`、`delta_extension_api`；
  - Suite 依赖完整 `delta` 发行包；
  - Foundation 源码出现 `delta-suite` / `delta_suite` 实现标识；
  - Foundation 重新加入 R 统计、`Rscript`、`chart_render` 或 `data-analysis` 技能。
- 边界检查：
  - Foundation：`python scripts/check_architecture_boundary.py`、`python scripts/check_repository_boundary.py`、`python scripts/check_naming.py`
  - Suite：`uv run python scripts/check_boundaries.py`、`uv run python scripts/check_naming.py`、`uv run python scripts/check_validation_claims.py`

### 3.2 PR 与 changelog

- 所有改动走 PR，不直接 push `main`。
- PR 标题使用 `type(scope): summary`；破坏性变更在类型后加 `!`，例如 `feat(analysis)!: ...`。
- 不编辑 `CHANGELOG.md` 的 `[Unreleased]`。新增 `changelog.d/<name>.md`，每条使用中文分类：
  - `- 新增: ...`
  - `- 变更: ...`
  - `- 修复: ...`
  - `- 安全: ...`
- 破坏性 PR 的每条 changelog 条目都要带 `[breaking]`。
- 发布 PR 只能修改 `CHANGELOG.md` 并删除已汇总的 fragment；不要手动改版本或 tag。

### 3.3 验证与保证等级

- `exploratory`：可探索、可草稿，不能进 formal 报告。
- `validated`：固定 R 方法包 + 固定环境 + 验证报告 + 证据；才可进 formal 报告。
- `attested`：外部工具结果 + 外部证据；后期适配器。
- 提升等级必须同时满足：
  - registry 有完整 `validation` 块；
  - 报告文件存在；
  - 证据文件 SHA-256 匹配；
  - 结果文件写出对应字段；
  - 所有者审查。
- 只改代码常量、没有报告与证据的提升，CI 必须失败。

### 3.4 测试与 CI

- Suite 中央 CI 由 Action Worker 测试包执行；修改测试包需要单独在 `action-worker` 开 PR 并先合并。
- 本地没有 R 4.6.0 时，R 测试允许 skip；**skip 不能作为验证证据**。
- 修复 CI 时先看失败分片与行号：
  - `Central CI / Linux (desktop)`：TypeScript / Tauri 类型检查；
  - `Central CI / Linux (rust-app-lint)`：Clippy / 未使用导入；
  - `Central CI / Linux (rust-crates)`：Rust 测试；
  - `Central CI / Linux (python)`：Python 检查；
  - `run-pack.mjs` 退出码对应具体测试包，见 `.github/scripts/central-ci.sh`。
- 不要在 PR 里放宽 lint 规则来消除失败；修复代码或规则本身。

### 3.5 发布

- 版本、tag、构建与签名由 Action Worker 统一发布流程处理。
- 发布前必须：CI 绿、PR 治理绿、changelog 一致、所有者确认版本。
- 发布后必须：重新下载产物并校验哈希/签名。

---

## 4. 交接协议

每个 Agent 开始任务时：

1. 读取本文件与对应仓库的 `CLAUDE.md` / `AGENTS.md`；
2. 检查任务板中的 PR 是否已合并，不要重复实现；
3. 在 PR 描述中写任务编号（例如 `P1-1`）、范围、验收与未验证事项；
4. 只改与本任务有关的文件；发现相邻问题时记录到任务板，不要顺手重构。

每个 Agent 结束时，向所有者报告：

```text
任务：P?-?
状态：完成 / 被阻塞 / 需要决定
PR：<链接>（head <sha>）
验证：<命令与结果>
未验证：<明确列出>
下一步：<一个具体动作>
```

停止条件（必须停下来问所有者）：

- 需要提升 `validated` / `attested`，但没有独立证据或审查人；
- 需要修改发布版本、tag 或签名配置；
- 需要跨仓库修改测试包，但当前 PR 无法合并；
- 发现需要改变 Foundation/Suite 边界的设计；
- 需要删除历史数据、用户文件或发布产物。

不要用“等 CI”作为不汇报的理由；CI 运行时先做下一项可并行的只读检查或文档工作，并在结束时给出准确状态。

---

## 5. 常用命令

Foundation：

```bash
cargo test --lib
python scripts/check_architecture_boundary.py
python scripts/check_repository_boundary.py
python scripts/check_naming.py
```

Suite：

```bash
uv run --with pytest pytest tests -q
uv run --with ruff==0.16.8 ruff check advanced scripts tests --select E9,F63,F7,F82,B,UP
uv run python scripts/check_boundaries.py
uv run python scripts/check_naming.py
uv run python scripts/check_validation_claims.py
uv lock --check
```

Action Worker 测试包（本地）：

```bash
node tests/run-pack.mjs delta-suite <suite-checkout> --filter=test_suite_contract
node tests/run-pack.mjs delta-suite <suite-checkout> --filter=test_suite_hygiene
node tests/run-pack.mjs delta-suite <suite-checkout>
```

Windows 本机没有系统 Node 时，可把 `C:\Users\Fong\AppData\Local\Temp\opencode\node\node-v24.22.0-win-x64` 加到 `PATH`。

---

## 6. 需要所有者决定的事项

以下事项不要由 Agent 自行决定：

1. 发布版本号与发布时机（`0.2.0-beta.1` 只是建议）。
2. 是否把 P1/P2 纳入本次发布。
3. 验证报告的审查人与签发人。
4. Python Explore Pack 的分发方式（内嵌解释器、独立 Pack 或按需下载）。
5. 是否为 Suite 增加独立的验证 CI 预算与运行环境。
6. ADR-0054 等历史文档是加“已取代”标记还是重写正文（建议只加标记，保留历史）。

---

## 7. 相关文件索引

- `docs/architecture/adr/0079-analysis-assurance-and-report-gate.md`
- `docs/architecture/adr/0080-analysis-runtime-packs.md`
- `docs/architecture/adr/0081-remove-foundation-r-statistics.md`
- `docs/architecture/adr/0082-foundation-suite-clean-cut.md`
- `docs/architecture/adr/0083-product-edition-marker.md`
- `delta-suite/validation/README.md`
- `delta-suite/validation/registry.json`
- `delta-suite/scripts/check_validation_claims.py`
- `delta-suite/docs/domains/cmc-calculation-flow.md`
- `delta-suite/advanced/cmc/r_engine.py`
- `action-worker/tests/packs/delta-suite/test_suite_contract.py`
