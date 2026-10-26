# Delta Contracts

The stable, language-neutral contract surface between Delta Foundation and Delta Suite.

This first package covers the installed Worker Manifest (`capability-workers.json`). The Worker ABI
models remain in `delta-worker-sdk` for now; they will move here when the migration in ADR-0082
reaches the SDK, without changing the public import path for existing workers.

## Rules

- No Foundation internal imports (`integrations.*`, `packages.*`, `core.*`, Runtime, Trust, Ledger).
- Models are versioned. A breaking change raises the model version and the manifest version.
- Suite depends on this package; Foundation validates installed manifests with the same models.
- The Worker Manifest version is `1`. The Capability ABI version remains `2`.

## Test

```bash
uv run --with pytest --with pydantic pytest packages/delta_contracts/tests
```

## Build and publish

```bash
uv build
# publish delta_contracts-<version>-py3-none-any.whl and .tar.gz to the internal index
```

The Worker SDK is built separately from `packages/delta_worker_sdk`. Suite installs both wheels in
its isolated CMC test environment; it does not install the full `delta` distribution for CMC.
