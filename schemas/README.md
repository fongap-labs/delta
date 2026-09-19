# Delta Foundation Schemas

> JSON Schema definitions for the cross-process / cross-repository contract
> data shapes used by Delta Foundation and consumed by Delta Suite.

## Index

| Schema | Purpose |
| --- | --- |
| `execution-grant.schema.json` | Execution Grant data contract |
| `capability-manifest.schema.json` | Worker capability manifest |

Schemas are versioned via the `"$id"` field. Extension code must not define
relaxed alternates of these shapes silently; breaking changes require a new
version.