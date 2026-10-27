"""Capability worker entry point for the Explore Pack (ADR-0078 amendment, ADR-0080).

The Rust host writes one `CapabilityJob` as a JSON line to stdin, then one line of granted secret
values, and reads one `CapabilityResult` as JSON from stdout. The host has already resolved the input file, hashed it and checked it against the
read grants (`input_files`); this worker checks the hash again before it computes anything, so a
file that changed between the host check and the run is refused.

Run with `python -I worker.py`. The worker adds its own package root to `sys.path`.
"""

from __future__ import annotations

import hashlib
import json
import sys
import time
from pathlib import Path
from typing import Any

_PACK_ROOT = Path(__file__).resolve().parents[2]
if str(_PACK_ROOT) not in sys.path:
    sys.path.insert(0, str(_PACK_ROOT))

from packages.explore_pack import engine  # noqa: E402

ABI_VERSION = 2

METHODS = {
    "statistics.explore.describe": "describe",
    "statistics.explore.compare": "compare",
    "statistics.explore.correlation": "correlation",
    "statistics.explore.regression": "regression",
}


def _failed(job_id: str, message: str, code: str) -> dict[str, Any]:
    return {
        "abi_version": ABI_VERSION,
        "job_id": job_id,
        "state": "failed",
        "result": None,
        "output": None,
        "artifacts": [],
        "diagnostics": {
            "stderr_lines": [],
            "error_code": code,
            "error_message": message,
        },
        "finished_at": time.time(),
    }


def _completed(job_id: str, document: dict[str, Any]) -> dict[str, Any]:
    return {
        "abi_version": ABI_VERSION,
        "job_id": job_id,
        "state": "completed",
        "result": document,
        "output": None,
        "artifacts": [],
        "diagnostics": None,
        "finished_at": time.time(),
    }


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _run_method(method: str, path: Path, arguments: dict[str, Any]) -> dict[str, Any]:
    if method == "describe":
        return engine.describe_column(path, str(arguments["column"]))
    if method == "compare":
        return engine.compare_groups(
            path,
            group_column=str(arguments["group_column"]),
            value_column=str(arguments["value_column"]),
            group_a=str(arguments["group_a"]),
            group_b=str(arguments["group_b"]),
        )
    if method == "correlation":
        return engine.correlation(
            path,
            x_column=str(arguments["x_column"]),
            y_column=str(arguments["y_column"]),
        )
    return engine.regression(
        path, x_column=str(arguments["x_column"]), y_column=str(arguments["y_column"])
    )


def run_job(job: dict[str, Any]) -> dict[str, Any]:
    """Run one job and return the `CapabilityResult` document as a dictionary."""
    job_id = str(job.get("job_id", ""))
    if job.get("abi_version") != ABI_VERSION:
        return _failed(job_id, "worker ABI version mismatch", "abi_mismatch")
    method = METHODS.get(str(job.get("capability_id", "")))
    if method is None:
        return _failed(
            job_id, "unknown capability for the explore worker", "unknown_capability"
        )
    inputs = job.get("input_files") or []
    if len(inputs) != 1:
        return _failed(job_id, "exactly one input file is required", "arguments")
    entry = inputs[0]
    path = Path(str(entry["path"]))
    try:
        digest = _sha256(path)
    except OSError as exc:
        return _failed(job_id, f"input file is unavailable: {exc}", "read_failed")
    if entry.get("sha256") and entry["sha256"] != digest:
        return _failed(job_id, "input hash changed before execution", "hash_changed")
    arguments = job.get("arguments") or {}
    try:
        document = _run_method(method, path, arguments)
    except (KeyError, OSError, TypeError, ValueError) as exc:
        return _failed(job_id, str(exc), "invalid_input")
    return _completed(job_id, document)


def main() -> None:
    # The host writes the job as the first line and the granted secret values as the second. This
    # worker is granted no secrets, so the second line is read and discarded.
    raw = sys.stdin.readline()
    sys.stdin.read()
    try:
        job = json.loads(raw)
    except json.JSONDecodeError as exc:
        result = _failed("", f"job is not valid JSON: {exc}", "invalid_job")
    else:
        result = run_job(job)
    sys.stdout.write(json.dumps(result, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
