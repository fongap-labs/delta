"""Build `delta.result` v2 documents for Explore Pack methods (ADR-0079, ADR-0080).

Every document written here is exploratory. The environment digest binds the result to the Python
runtime, the platform and the pack's own source files, so a result can be traced to the code that
produced it.
"""

from __future__ import annotations

from typing import Any

import hashlib
import json
import math
import platform
import sys
from collections.abc import Mapping, Sequence
from pathlib import Path

PACK_VERSION = "0.1.0"
METHOD_VERSION = "0.1.0"
_PACK_DIR = Path(__file__).resolve().parent


def _sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def environment_sha256() -> str:
    """Digest of the runtime and the pack source files that compute exploratory results."""
    sources = {
        name: _sha256_bytes((_PACK_DIR / name).read_bytes())
        for name in ("stats.py", "result.py", "engine.py")
    }
    description = {
        "pack": "explore",
        "pack_version": PACK_VERSION,
        "python": platform.python_version(),
        "implementation": platform.python_implementation(),
        "platform": sys.platform,
        "sources": sources,
    }
    encoded = json.dumps(description, sort_keys=True, separators=(",", ":"))
    return _sha256_bytes(encoded.encode("utf-8"))


def split_publishable(
    values: Mapping[str, float], warnings: list[str]
) -> dict[str, float]:
    """Keep finite values. A non-finite value is dropped and recorded as a warning."""
    kept: dict[str, float] = {}
    for key, value in values.items():
        if math.isfinite(value):
            kept[key] = float(value)
        else:
            warnings.append(f"{key} is not finite for this input and was not written")
    return kept


def build_result(
    *,
    method_id: str,
    values: Mapping[str, float],
    parameters: Mapping[str, object],
    inputs: Sequence[tuple[str, str]],
    warnings: Sequence[str] = (),
    limitations: Sequence[str] = (),
) -> dict[str, Any]:
    """Return a `delta.result` v2 document with `assurance: exploratory`."""
    return {
        "format": "delta.result",
        "version": 2,
        "assurance": "exploratory",
        "engine": {
            "name": "python",
            "version": platform.python_version(),
            "pack": "explore",
            "environment_sha256": environment_sha256(),
        },
        "method": {"id": method_id, "version": METHOD_VERSION},
        "inputs": [{"path": path, "sha256": digest} for path, digest in inputs],
        "parameters": dict(parameters),
        "values": {key: {"value": value} for key, value in values.items()},
        "warnings": list(warnings),
        "limitations": [
            "exploratory: this result cannot enter a formal report",
            *limitations,
        ],
    }


def write_result(document: Mapping[str, object], path: Path) -> None:
    """Write the document as UTF-8 JSON with sorted keys."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(document, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
