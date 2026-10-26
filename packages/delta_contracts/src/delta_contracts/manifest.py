"""Installed Worker Manifest contracts (capability-workers.json).

The shape mirrors the Rust loader in ``crates/delta-core/src/extension_manifest.rs``. This package
is the Python-side contract; Foundation remains the authority that validates and loads manifests.
"""

from __future__ import annotations

from pathlib import PurePath
from typing import Any

from pydantic import BaseModel, ConfigDict, Field, field_validator

CAPABILITY_ABI_VERSION = 2
WORKER_MANIFEST_VERSION = 1


class CapabilityGrants(BaseModel):
    """The grant shape a Worker Manifest may request."""

    model_config = ConfigDict(extra="forbid")

    network: list[str] = Field(default_factory=list)
    read_roots: list[str] = Field(default_factory=list)
    write_roots: list[str] = Field(default_factory=list)
    secrets: list[str] = Field(default_factory=list)
    exec: bool = False
    execution_epoch: float | None = None
    expires_at: float | None = None


class WorkerEntry(BaseModel):
    """One capability and the program that runs it."""

    model_config = ConfigDict(extra="forbid")

    capability_id: str
    tool_name: str
    description: str
    parameters: dict[str, Any] = Field(default_factory=dict)
    metadata: dict[str, Any] = Field(default_factory=dict)
    grants: CapabilityGrants = Field(default_factory=CapabilityGrants)
    workspace_write: bool = False
    program: str
    arguments: list[str] = Field(default_factory=list)
    environment: dict[str, str] = Field(default_factory=dict)
    enabled: bool = True
    persistent: bool = False
    process_key: str | None = None

    @field_validator("capability_id", "tool_name", "description", "program")
    @classmethod
    def required_text(cls, value: str) -> str:
        if not value.strip():
            raise ValueError("must not be empty")
        return value

    @field_validator("program")
    @classmethod
    def absolute_program(cls, value: str) -> str:
        if not PurePath(value).is_absolute():
            raise ValueError("program must be an absolute path")
        return value

    @field_validator("environment")
    @classmethod
    def environment_keys_present(cls, value: dict[str, str]) -> dict[str, str]:
        if any(not key.strip() for key in value):
            raise ValueError("environment keys must not be empty")
        return value


class WorkerManifest(BaseModel):
    """The installed manifest under ``<state>/extensions/<source>/capability-workers.json``."""

    model_config = ConfigDict(extra="forbid")

    version: int = WORKER_MANIFEST_VERSION
    source: str
    workers: list[WorkerEntry] = Field(default_factory=list)

    @field_validator("version")
    @classmethod
    def supported_version(cls, value: int) -> int:
        if value != WORKER_MANIFEST_VERSION:
            raise ValueError(f"unsupported worker manifest version: {value}")
        return value

    @field_validator("source")
    @classmethod
    def source_matches_directory(cls, value: str) -> str:
        if not value.strip():
            raise ValueError("source must not be empty")
        if "/" in value or "\\" in value:
            raise ValueError("source must be one directory name")
        return value

    def for_install(self, directory_name: str) -> "WorkerManifest":
        """Check that the source matches the directory Foundation will load it from."""
        if self.source != directory_name:
            raise ValueError(
                f"manifest source {self.source!r} does not match extension directory "
                f"{directory_name!r}"
            )
        return self


__all__ = [
    "CAPABILITY_ABI_VERSION",
    "WORKER_MANIFEST_VERSION",
    "CapabilityGrants",
    "WorkerEntry",
    "WorkerManifest",
]
