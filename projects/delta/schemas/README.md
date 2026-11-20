# Delta Foundation Schemas

> 本目录是 Delta Foundation 与 Delta Suite 共同使用的跨进程、跨仓库合同数据结构的 JSON Schema 定义。

## 索引

| Schema | 用途 |
| --- | --- |
| `execution-grant.schema.json` | Execution Grant 数据合同 |
| `capability-manifest.schema.json` | Worker 能力清单（capability manifest） |
| `study.schema.json` | 多轮研究的研究记录（ADR-0056） |

Schema 通过 `"$id"` 字段标识版本。扩展代码不得悄悄定义这些结构的宽松替代版本；破坏性变更必须发布新版本。
