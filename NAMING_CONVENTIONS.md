# 命名约定

本文说明本仓库的命名约定入口。跨仓库的规范命名规则维护在 Action Worker 中：

- https://github.com/fongap-labs/action-worker/blob/main/docs/03_NAMING_CONVENTIONS.md

本仓库不复制中央命名规范。

仓库内的静态校验可保留在 `scripts/check_naming.py` 中，用于校验中央校验器尚未覆盖的源码语言或仓库特有细节。本地校验可以比中央规则更严格，但不得重新定义中央命名语义，也不得与之矛盾。
