"""Protocol definitions matching Rust CapabilityJob/CapabilityResult."""

from __future__ import annotations

from typing import Any, Literal, Optional
from pydantic import BaseModel, Field


class CapabilityInputFile(BaseModel):
    path: str
    sha256: str


class CapabilityGrants(BaseModel):
    network: list[str] = Field(default_factory=list)
    read_roots: list[str] = Field(default_factory=list)
    write_roots: list[str] = Field(default_factory=list)
    secrets: list[str] = Field(default_factory=list)


class CapabilityBoundary(BaseModel):
    network: list[str] = Field(default_factory=list)
    read_roots: list[str] = Field(default_factory=list)
    write_roots: list[str] = Field(default_factory=list)
    secrets: list[str] = Field(default_factory=list)
    provenance: str = ""


class CapabilityJob(BaseModel):
    abi_version: int
    capability_id: str
    job_id: str
    run_id: Optional[str] = None
    session_id: Optional[str] = None
    workspace: Optional[str] = None
    input_files: list[CapabilityInputFile] = Field(default_factory=list)
    arguments: dict[str, Any] = Field(default_factory=dict)
    grants: CapabilityGrants = Field(default_factory=CapabilityGrants)
    boundary: CapabilityBoundary = Field(default_factory=CapabilityBoundary)
    timeout_secs: int = 0
    artifact_staging_dir: Optional[str] = None


class CapabilityProgress(BaseModel):
    type: Literal["progress"] = "progress"
    data: dict[str, Any]


class CapabilityArtifact(BaseModel):
    staging_path: str
    relative_path: str
    kind: str
    sha256: Optional[str] = None
    size: Optional[int] = None
    incomplete: bool = False


class CapabilityDiagnostics(BaseModel):
    stderr_lines: list[str] = Field(default_factory=list)
    error_code: Optional[str] = None
    error_message: Optional[str] = None


class CapabilityResult(BaseModel):
    abi_version: int
    job_id: str
    state: Literal["completed", "failed", "cancelled", "timed_out"]
    result: Optional[dict[str, Any]] = None
    output: Optional[str] = None
    artifacts: list[CapabilityArtifact] = Field(default_factory=list)
    diagnostics: Optional[CapabilityDiagnostics] = None
    finished_at: float