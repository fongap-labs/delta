# 来源证明与第三方治理

本文规范 Delta 的来源证明（provenance）、第三方代码采纳与许可证义务。Delta 是一个独立项目，不与任何外部项目维持强制性的上游、镜像分支或自动同步关系。

本文不重复产品范围、目标架构或 Runtime 设计。

## 原则

- 外部项目是参考，而非产品权威。
- 外部代码、行为或架构思路在进入 `main` 之前必须经过独立评估。
- Delta 不会仅为了跟随其他项目，而恢复已退役的目录、协议、面向 Coding 的产品形态、兼容层或多 Agent 结构。
- 现行文档描述的是当前 Delta 架构。历史细节应保存在 Git、ADR、变更日志和审计记录中；在确有实质价值时，也可记入法律声明。
- 第三方版权与许可证义务跟随仍保留在仓库中的代码和资产，与仓库历史无关。
- 第三方名称不得用作 Delta 当前的产品、架构、模块、Runtime 或发布身份。

## 评估过滤条件

任何被考虑引入 Delta 的外部实现，必须回答以下问题：

1. 它能否改善日常办公、研究分析或内容创作中的真实工作流？
2. 它由哪项职责负责：Experience、Runtime、Trust、Work、Capability、Automation 还是 Learning？
3. 谁拥有其状态与 Authority？
4. 能否先作为 Capability 或 Skill 实现，而不是先扩大 Runtime 或 Agent 范围？
5. 它是否符合 Rust + TypeScript 核心边界与受控 Worker 模型？
6. 它是否引入新的安全、许可证或供应链风险？
7. 是否具备测试、验证与回滚路径？

## 外部参考不等于上游身份

Delta 可以学习外部项目在 Runtime、UI、Trust、Office、Research、Content、Skill、Memory、Automation 及相关领域中的优秀思路。参考项目不会成为 Delta 的产品身份或发布 Authority。

采纳路径如下：

```text
problem semantics
→ verifiable behavior
→ Delta-owned contract
→ independent implementation or compliant adoption
```

Delta 不默认继承外部品牌、目录结构或历史包袱。

## 命名与来源

外部模块名不会自动进入 Delta 的命名体系。

Delta 命名规则：

> **真实的独立边界使用 `delta-*`；内部职责使用语义化名称。**

只有存在真正的独立可执行程序、API、协议、SDK、版本边界或发布边界时，才适用带品牌的 `delta-*` 名称。

仅在以下情形中，外部项目名称才可在必要范围内出现：

- 版权与许可证署名；
- 第三方声明；
- 历史 ADR 或审计证据；
- 漏洞与供应链记录；
- 来源证明决策的可复现性。

外部项目名称不得出现在当前 Runtime 配置、端点默认值、面向用户的产品身份、包命名、现行架构 Authority 或发布命名中。

## 许可证边界

Delta 自有的 Foundation 源码采用 Apache License, Version 2.0 许可。根目录的 `LICENSE` 适用于 Delta 自有作品；项目署名载于 `NOTICE`。

第三方代码与资产不会因根目录的 Apache-2.0 许可证而被重新授权。其署名登记在 THIRD_PARTY_NOTICES.md 中，完整的上游许可证文本存放于 `LICENSES/` 目录下。

引入第三方代码或资产之前，请确认：

- 原始许可证；
- 必需的版权声明；
- NOTICE 或署名义务；
- 修改与再分发的权利；
- 与 Delta 仓库许可证的兼容性；
- 生成代码、模型权重、媒体或其他资产的附加条款。

如果义务不明确，不得合并相关材料。

仓库重建、历史改写、重命名或更改 Delta 自有作品的许可证，都不会消除仍然源自第三方的材料所承担的第三方义务。

## 当前来源状态

当前 Delta 源码由仓库现有的许可证、声明与第三方署名记录治理。历史实现审计与已被取代的谱系决策，在需要用于法律或安全审查时可通过 Git 历史获取；它们不属于现行架构合同。

## 归属记录

当前产品身份由当前 Delta 的合同与治理定义，而非由历史项目谱系定义。来源记录仅在许可、安全、架构决策或可复现性确有必要时保留。
