# Delta 扩展分发治理（Extension Distribution Governance）

**版本：** v1.1
**状态：** 治理基线
**范围：** 可选 Delta 扩展的发现、安装、校验、更新与移除。

本文是可选扩展分发的基线，规定扩展的发现、安装、校验、更新与移除的要求，以及本地安装目录结构。

## 1. 原则

Delta Foundation 不依赖扩展即可完整运行。安装扩展绝不会改变由哪个组件掌握 Runtime、Trust、State、Approval 或 Recovery 的权威。

每个扩展都必须：

- 具有明确可识别的来源 / ID；
- 与其他已安装的扩展相互隔离；
- 由带版本的公开清单描述；
- 仅被授予其声明的、且用户批准的权限；
- 可以移除，且移除不损害 Foundation 状态。

## 2. 安装布局

规范的本地布局如下：

```text
<delta-state>/
└── extensions/
    ├── <extension-a>/
    │   ├── capability-workers.json
    │   └── connector-catalog.json
    └── <extension-b>/
        ├── capability-workers.json
        └── connector-catalog.json
```

单个根级清单不构成扩展边界，因为相互独立的包可能彼此覆盖。

## 3. 发现

Foundation 以确定的顺序枚举扩展目录，并独立校验每个清单。

若扩展目录不存在，则 Foundation 以“仅 Foundation”模式运行。

格式错误的扩展会被跳过并报告。它不得阻止 Foundation 或无关的有效扩展启动。

能力 ID、工具名或连接器 ID 重复时，在公开合同边界处被拒绝。

## 4. 安装

安装必须：

1. 仅创建或更新该扩展自己的目录；
2. 以原子方式写入清单；
3. 绝不将凭据写入清单；
4. 绝不直接修改 Foundation 的权威数据库；
5. 不改动其他扩展的目录。

卸载仅移除该扩展已安装的产物。由 Foundation 持有的持久化用户数据，仍须通过 Foundation API 管理。

## 5. 权限模型

权限是按任务（job）划分的 Runtime 授权。扩展元数据可以请求文件系统、网络、密钥、进程或硬件访问，但只有 Foundation 可以批准并执行最终的有效授权。

任何扩展都不会仅因安装而获得全局访问权限。

## 6. 兼容性

扩展依赖已发布的公开合同。生产环境 CI 必须针对明确兼容的 Foundation 版本或引用进行测试，而不得使用未固定版本的默认分支。

开发阶段可额外测试最新的 Foundation main 分支，作为非阻塞的兼容性信号。

## 7. 安全与来源证明

分发系统可以增加包签名、哈希、来源证明（provenance）、SBOM 与吊销元数据。这些机制属于传输与供应链层面的事项，并不授予 Runtime 权威。

## 8. 硬性规则

- Foundation 绝不下载或导入具有特权的扩展实现。
- 扩展代码绝不得绕过 CapabilityHost / Trust / Approval。
- 一个扩展不得覆盖另一个扩展的清单。
- 凭据绝不存在于扩展清单中。
- 损坏的可选扩展不得导致本地 Foundation 无法使用。
