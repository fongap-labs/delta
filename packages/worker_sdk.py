"""Delta Worker SDK -- thin protocol adapter for controlled workers (R8.6).

This module is intentionally small and dumb. It is a protocol adapter
layer: it reads the Capability ABI job from stdin, checks the version,
provides access to arguments and granted secrets, emits progress frames,
observes cancellation, and writes a typed result to stdout.

It is NOT a runtime, agent, scheduler, state manager, or permission manager.

Worker protocol (matches ``WorkerProcessRunner`` in delta-core):

  stdin  line 1: CapabilityJob JSON (no secret values)
  stdin  line 2: secrets JSON (``{"key": "value", ...}`` or ``{}``)
  stdout       : JSON frames (progress or result), one per line
  stderr       : free-text diagnostic lines

Usage::

    from packages.worker_sdk import run_worker

    def my_handler(ctx):
        path = ctx.arguments["path"]
        text = open(path).read()
        ctx.emit_progress(1.0, "done")
        return ctx.result(is_ok=True, output=text)

    run_worker(my_handler)
"""

from __future__ import annotations

import json
import os
import sys
from typing import Any, Callable

CAPABILITY_ABI_VERSION = 2


class WorkerError(Exception):
    """Typed worker error with a stable code."""

    def __init__(self, message: str, code: str = "worker_error") -> None:
        super().__init__(message)
        self.message = message
        self.code = code


class WorkerContext:
    """Read-only context for a single capability job."""

    def __init__(
        self,
        job: dict[str, Any],
        secrets: dict[str, str],
    ) -> None:
        self.job = job
        self.secrets = secrets
        self._is_cancelled = False

    @property
    def capability_id(self) -> str:
        return self.job.get("capability_id", "")

    @property
    def job_id(self) -> str:
        return self.job.get("job_id", "")

    @property
    def arguments(self) -> dict[str, Any]:
        return self.job.get("arguments", {})

    @property
    def workspace(self) -> str | None:
        return self.job.get("workspace")

    @property
    def artifact_staging_dir(self) -> str | None:
        return self.job.get("artifact_staging_dir")

    @property
    def grants(self) -> dict[str, Any]:
        return self.job.get("grants", {})

    @property
    def boundary(self) -> dict[str, Any]:
        return self.job.get("boundary", {})

    @property
    def timeout_secs(self) -> int:
        return self.job.get("timeout_secs", 0)

    @property
    def is_cancelled(self) -> bool:
        return self._is_cancelled

    def secret(self, key: str) -> str | None:
        """Get a granted secret value by key, or None if not granted."""
        granted = set(self.grants.get("secrets", []))
        if key not in granted:
            return None
        return self.secrets.get(key)

    def emit_progress(
        self,
        fraction: float,
        stage: str | None = None,
        message: str | None = None,
    ) -> None:
        """Emit a progress frame to stdout."""
        frame = {
            "type": "progress",
            "data": {
                "job_id": self.job_id,
                "fraction": fraction,
                "stage": stage,
                "message": message,
            },
        }
        sys.stdout.write(json.dumps(frame) + "\n")
        sys.stdout.flush()

    def stage_artifact(
        self,
        relative_path: str,
        content: bytes,
        kind: str = "file",
    ) -> str:
        """Write content to the staging directory and return the staging path.

        The Runtime formalizes (hash, validate, register) the artifact later.
        Workers never register formal artifacts directly.
        """
        staging_dir = self.artifact_staging_dir
        if not staging_dir:
            raise WorkerError("artifact staging directory is unavailable", "staging")
        os.makedirs(staging_dir, exist_ok=True)
        staging_path = os.path.join(staging_dir, "candidate_" + relative_path.replace("/", "_"))
        with open(staging_path, "wb") as f:
            f.write(content)
        return staging_path

    def result(
        self,
        *,
        is_ok: bool = True,
        output: str | None = None,
        artifacts: list[dict[str, Any]] | None = None,
    ) -> dict[str, Any]:
        """Build a typed CapabilityResult for a completed job."""
        return {
            "abi_version": CAPABILITY_ABI_VERSION,
            "job_id": self.job_id,
            "state": "completed" if is_ok else "failed",
            "result": {"ok": is_ok, **({"output": output} if output is not None else {})},
            "output": output,
            "artifacts": artifacts or [],
            "diagnostics": None,
            "finished_at": _now(),
        }

    def error_result(
        self,
        message: str,
        code: str = "worker_error",
        stderr_lines: list[str] | None = None,
    ) -> dict[str, Any]:
        """Build a typed CapabilityResult for a failed job."""
        return {
            "abi_version": CAPABILITY_ABI_VERSION,
            "job_id": self.job_id,
            "state": "failed",
            "result": None,
            "output": None,
            "artifacts": [],
            "diagnostics": {
                "stderr_lines": stderr_lines or [],
                "error_code": code,
                "error_message": message,
            },
            "finished_at": _now(),
        }

    def artifact(
        self,
        staging_path: str,
        relative_path: str,
        kind: str = "file",
        sha256: str | None = None,
        size: int | None = None,
        is_incomplete: bool = False,
    ) -> dict[str, Any]:
        """Build an artifact declaration for the result."""
        return {
            "staging_path": staging_path,
            "relative_path": relative_path,
            "kind": kind,
            "sha256": sha256,
            "size": size,
            "incomplete": is_incomplete,
        }


def _now() -> float:
    import time
    return time.time()


def _read_stdin_json() -> dict[str, Any]:
    """Read one JSON line from stdin."""
    line = sys.stdin.readline()
    if not line:
        return {}
    return json.loads(line)


def run_worker(
    handler: Callable[[WorkerContext], dict[str, Any]],
) -> None:
    """Read the job from stdin, call handler, write the result to stdout.

    The handler receives a WorkerContext and must return a CapabilityResult
    dict. If the handler raises WorkerError, a typed error result is emitted.
    """
    try:
        job = _read_stdin_json()
    except Exception as exc:
        _emit_fatal(f"failed to read job: {exc}")
        return

    abi = job.get("abi_version", 0)
    if abi != CAPABILITY_ABI_VERSION:
        _emit_fatal(
            f"ABI version mismatch: worker={CAPABILITY_ABI_VERSION}, job={abi}",
            code="abi_mismatch",
        )
        return

    try:
        secrets = _read_stdin_json()
    except Exception:
        secrets = {}

    ctx = WorkerContext(job, secrets)

    try:
        result = handler(ctx)
    except WorkerError as exc:
        result = ctx.error_result(exc.message, exc.code)
    except Exception as exc:
        result = ctx.error_result(str(exc), "worker_unhandled")

    if not isinstance(result, dict):
        result = ctx.error_result("handler did not return a dict", "worker_protocol")

    result.setdefault("abi_version", CAPABILITY_ABI_VERSION)
    result.setdefault("job_id", ctx.job_id)
    result.setdefault("finished_at", _now())

    sys.stdout.write(json.dumps(result) + "\n")
    sys.stdout.flush()


def _emit_fatal(message: str, code: str = "worker_fatal") -> None:
    """Emit a minimal failure result to stdout and exit."""
    result = {
        "abi_version": CAPABILITY_ABI_VERSION,
        "job_id": "",
        "state": "failed",
        "result": None,
        "output": None,
        "artifacts": [],
        "diagnostics": {
            "stderr_lines": [],
            "error_code": code,
            "error_message": message,
        },
        "finished_at": _now(),
    }
    sys.stdout.write(json.dumps(result) + "\n")
    sys.stdout.flush()


__all__ = [
    "CAPABILITY_ABI_VERSION",
    "WorkerContext",
    "WorkerError",
    "run_worker",
]
