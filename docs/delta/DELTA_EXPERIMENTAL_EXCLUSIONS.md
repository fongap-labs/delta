# 上游实验实现的排除边界

本文件此前错误地把排除 Harness 实验实现解释为排除同类产品能力，现已纠正。Delta 保留 CUA、浏览器操作、Forge、Worktree、LiveShare；不直接吸收 Harness 处于实验状态的功能实现。应评估非实验实现或 Delta 自有实现，同时核查许可证、依赖和真实可用性，不能通过重命名上游实验代码规避要求。详见 [五项产品能力与实现来源](DELTA_FEATURE_SOURCES.md)。

`node scripts/delta-verify.mjs --ci` 是源码门禁：若 `packages/experimental` 目录重新出现，或工作区包清单重新引用 `@deepseek-ai/dsh-experimental-*`，检查会失败。测试夹具和盘点文档可保留这些名称来验证过滤行为或记录被排除的来源，不代表它们进入产品运行闭包。
