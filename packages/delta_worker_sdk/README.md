# Delta Worker SDK

Thin Python protocol adapter for Delta Capability ABI v2 workers.

It provides typed job, grant, boundary, progress, artifact, and result models plus
`WorkerContext`, `run_worker()` for one-job processes, and `run_persistent_worker()` for long-lived processes that reuse runtime state or sessions.

The SDK does not own runtime orchestration, routing, policy, approval, credentials,
state, or lifecycle authority. Those remain in Delta Foundation.

## Worker contract

Each worker frame is one `CapabilityJob` JSON line followed by one granted-secret JSON line. The SDK validates the frame, executes the handler, may emit progress frames, and emits one terminal `CapabilityResult`.

Use `run_worker()` when the process handles one job. Use `run_persistent_worker()` when the same process must handle multiple jobs while preserving worker-local runtime state such as a browser session.

```python
from delta_worker_sdk import WorkerContext, run_persistent_worker, run_worker


def handle(ctx: WorkerContext):
    return ctx.result(result={"ok": True})


if __name__ == "__main__":
    run_worker(handle)
```

Persistent worker:

```python
from delta_worker_sdk import WorkerContext, run_persistent_worker


def handle(ctx: WorkerContext):
    return ctx.result(result={"job_id": ctx.job_id})


if __name__ == "__main__":
    raise SystemExit(run_persistent_worker(handle))
```
