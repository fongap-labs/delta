# Delta Worker SDK

Thin Python protocol adapter for Delta Capability ABI v2 workers.

It provides typed job, grant, boundary, progress, artifact, and result models plus
`WorkerContext` and `run_worker()` for one-job worker processes.

The SDK does not own runtime orchestration, routing, policy, approval, credentials,
state, or lifecycle authority. Those remain in Delta Foundation.

## Worker contract

A worker reads one `CapabilityJob` JSON line and one granted-secret JSON line from
stdin, executes its handler, may emit progress frames, and emits one terminal
`CapabilityResult`.

```python
from delta_worker_sdk import WorkerContext, run_worker


def handle(ctx: WorkerContext):
    return ctx.result(result={"ok": True})


if __name__ == "__main__":
    run_worker(handle)
```
