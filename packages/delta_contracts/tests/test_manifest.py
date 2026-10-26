from __future__ import annotations

import json
import sys

import pytest
from pydantic import ValidationError

from delta_contracts import (
    CAPABILITY_ABI_VERSION,
    WORKER_MANIFEST_VERSION,
    WorkerManifest,
)


def _manifest() -> dict:
    return {
        "version": 1,
        "source": "example-extension",
        "workers": [
            {
                "capability_id": "example-extension.cmc.dissolution.f2",
                "tool_name": "suite_f2",
                "description": "Compute f2 with R.",
                "parameters": {"type": "object", "properties": {}},
                "metadata": {"source": "example-extension"},
                "grants": {"read_roots": [], "write_roots": [], "network": []},
                "workspace_write": True,
                "program": sys.executable,
                "arguments": ["-m", "advanced.cmc.worker", "--tool", "suite_f2"],
                "environment": {"DELTA_RSCRIPT": "C:\\R\\Rscript.exe"},
                "enabled": True,
            }
        ],
    }


def test_a_manifest_round_trips_with_the_installed_shape() -> None:
    manifest = WorkerManifest.model_validate(_manifest())
    assert CAPABILITY_ABI_VERSION == 2
    assert manifest.version == WORKER_MANIFEST_VERSION
    assert manifest.source == "example-extension"
    assert manifest.workers[0].capability_id == "example-extension.cmc.dissolution.f2"
    assert manifest.workers[0].workspace_write is True
    assert json.loads(manifest.model_dump_json())["workers"][0]["tool_name"] == "suite_f2"


def test_a_relative_program_is_refused() -> None:
    data = _manifest()
    data["workers"][0]["program"] = "python"
    with pytest.raises(ValidationError, match="absolute path"):
        WorkerManifest.model_validate(data)


def test_an_unknown_manifest_version_is_refused() -> None:
    data = _manifest()
    data["version"] = 2
    with pytest.raises(ValidationError, match="unsupported worker manifest version"):
        WorkerManifest.model_validate(data)


def test_the_source_must_match_the_install_directory() -> None:
    manifest = WorkerManifest.model_validate(_manifest())
    with pytest.raises(ValueError, match="does not match"):
        manifest.for_install("another-source")


def test_unknown_fields_are_refused() -> None:
    data = _manifest()
    data["workers"][0]["new_authority"] = True
    with pytest.raises(ValidationError):
        WorkerManifest.model_validate(data)
