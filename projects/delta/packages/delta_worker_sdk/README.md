# Delta Worker SDK

Delta Worker SDK 是面向 Delta Capability ABI v2 Worker 的轻量协议适配器（thin protocol adapter）。

它提供类型化的 job、授予（grant）、边界（boundary）、进度（progress）、产物（artifact）与结果（result）模型，以及 `WorkerContext`、用于单任务进程的 `run_worker()`，和用于长生命周期进程（可复用运行时状态或会话）的 `run_persistent_worker()`。

SDK 不拥有运行时编排、路由、策略、审批、凭据、状态或生命周期方面的权威；这些权威仍保留在 Delta Foundation 中。

## Worker 合同

每个 Worker 帧由一行 `CapabilityJob` JSON，以及随后一行被授予的密钥（granted-secret）JSON 组成。SDK 校验该帧、执行处理函数，可以发出进度帧，并输出一个终态 `CapabilityResult`。

进程只处理一个任务时使用 `run_worker()`；同一进程需要处理多个任务，并保留 Worker 本地的运行时状态（例如浏览器会话）时，使用 `run_persistent_worker()`。

```python
from delta_worker_sdk import WorkerContext, run_persistent_worker, run_worker


def handle(ctx: WorkerContext):
    return ctx.result(result={"ok": True})


if __name__ == "__main__":
    run_worker(handle)
```

长生命周期 Worker：

```python
from delta_worker_sdk import WorkerContext, run_persistent_worker


def handle(ctx: WorkerContext):
    return ctx.result(result={"job_id": ctx.job_id})


if __name__ == "__main__":
    raise SystemExit(run_persistent_worker(handle))
```
