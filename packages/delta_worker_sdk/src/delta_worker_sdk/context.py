"""WorkerContext - runtime context for a capability worker.

Provides typed access to job arguments, grants, secrets, and helpers
for progress reporting, artifact staging, and result emission.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any, Callable, Optional

from .protocol import (
    CapabilityArtifact,
    CapabilityBoundary,
    CapabilityGrants,
    CapabilityJob,
    CapabilityProgress,
    CapabilityResult,
    CapabilityDiagnostics,
)


class WorkerContext:
    """Immutable context for a single capability execution.

    Workers receive this context and use it to:
    - Access job arguments and metadata
    - Access granted secrets (never in env vars)
    - Report progress
    - Stage artifacts
    - Emit final result
    """

    def __init__(
        self,
        job: CapabilityJob,
        secret_values: dict[str, str],
        progress_sender: Callable[[CapabilityProgress], None],
        cancel_checker: Callable[[], bool],
    ):
        self._job = job
        self._secret_values = secret_values
        self._progress = progress_sender
        self._cancel = cancel_checker

    @property
    def job(self) -> CapabilityJob:
        return self._job

    @property
    def capability_id(self) -> str:
        return self._job.capability_id

    @property
    def job_id(self) -> str:
        return self._job.job_id

    @property
    def arguments(self) -> dict[str, Any]:
        return self._job.arguments

    @property
    def grants(self) -> CapabilityGrants:
        return self._job.grants

    @property
    def boundary(self) -> CapabilityBoundary:
        return self._job.boundary

    @property
    def workspace(self) -> Optional[str]:
        return self._job.workspace

    @property
    def artifact_staging_dir(self) -> Optional[Path]:
        if self._job.artifact_staging_dir:
            return Path(self._job.artifact_staging_dir)
        return None

    def secret(self, key: str) -> Optional[str]:
        """Get a granted secret value.

        Only returns secrets listed in grants.secrets.
        Returns None if key not granted or not provided.
        """
        if key not in self._job.grants.secrets:
            return None
        return self._secret_values.get(key)

    def secret_or_raise(self, key: str) -> str:
        """Get a granted secret, raise if not available."""
        val = self.secret(key)
        if val is None:
            raise ValueError(f"Secret '{key}' not granted or not provided")
        return val

    def emit_progress(
        self,
        fraction: float,
        stage: Optional[str] = None,
        message: Optional[str] = None,
    ) -> None:
        """Emit a progress frame to stdout."""
        data = {"job_id": self._job.job_id, "fraction": fraction}
        if stage:
            data["stage"] = stage
        if message:
            data["message"] = message
        self._progress(CapabilityProgress(data=data))

    def is_cancelled(self) -> bool:
        """Check if the job has been cancelled."""
        return self._cancel()

    def check_cancelled(self) -> None:
        """Raise if the job has been cancelled."""
        if self._cancel():
            raise KeyboardInterrupt("Job cancelled")

    def stage_artifact(
        self,
        relative_path: str,
        content: bytes,
        kind: str = "file",
        sha256: Optional[str] = None,
    ) -> CapabilityArtifact:
        """Write content to staging dir and return artifact declaration.

        The Runtime will validate, hash, and promote this to a formal artifact.
        """
        staging_dir = self.artifact_staging_dir
        if not staging_dir:
            raise RuntimeError("No artifact staging directory configured")

        staging_dir.mkdir(parents=True, exist_ok=True)
        staging_path = staging_dir / relative_path
        staging_path.parent.mkdir(parents=True, exist_ok=True)
        staging_path.write_bytes(content)

        if sha256 is None:
            import hashlib
            sha256 = hashlib.sha256(content).hexdigest()

        return CapabilityArtifact(
            staging_path=str(staging_path),
            relative_path=relative_path,
            kind=kind,
            sha256=sha256,
            size=len(content),
        )

    def result(
        self,
        output: Optional[str] = None,
        result: Optional[Any] = None,
        artifacts: Optional[list[CapabilityArtifact]] = None,
    ) -> CapabilityResult:
        """Build a completed result."""
        return CapabilityResult(
            abi_version=self._job.abi_version,
            job_id=self._job.job_id,
            state="completed",
            result=result,
            output=output,
            artifacts=artifacts or [],
            diagnostics=None,
            finished_at=__import__("time").time(),
        )

    def error(
        self,
        message: str,
        code: Optional[str] = None,
        stderr_lines: Optional[list[str]] = None,
    ) -> CapabilityResult:
        """Build a failed result."""
        return CapabilityResult(
            abi_version=self._job.abi_version,
            job_id=self._job.job_id,
            state="failed",
            result=None,
            output=None,
            artifacts=[],
            diagnostics=CapabilityDiagnostics(
                stderr_lines=stderr_lines or [],
                error_code=code,
                error_message=message,
            ),
            finished_at=__import__("time").time(),
        )


def _default_cancel_checker() -> bool:
    return False


def _default_progress_sender(progress: CapabilityProgress) -> None:
    """Default progress sender writes JSON line to stdout."""
    sys.stdout.write(progress.model_dump_json() + "\n")
    sys.stdout.flush()


def _emit_result(result: CapabilityResult) -> CapabilityResult:
    sys.stdout.write(result.model_dump_json() + "\n")
    sys.stdout.flush()
    return result


def _input_error(job_id: str, message: str) -> CapabilityResult:
    from . import CAPABILITY_ABI_VERSION

    return CapabilityResult(
        abi_version=CAPABILITY_ABI_VERSION,
        job_id=job_id,
        state="failed",
        result=None,
        output=None,
        artifacts=[],
        diagnostics=CapabilityDiagnostics(
            stderr_lines=[],
            error_code="worker_input",
            error_message=message,
        ),
        finished_at=__import__("time").time(),
    )


def run_worker(
    handler: Callable[[WorkerContext], CapabilityResult],
    *,
    progress_sender: Callable[[CapabilityProgress], None] = _default_progress_sender,
    cancel_checker: Callable[[], bool] = _default_cancel_checker,
) -> Optional[CapabilityResult]:
    """Run one Capability ABI job and emit exactly one terminal result.

    Reads from stdin:
    - Line 1: CapabilityJob JSON
    - Line 2: Secret values JSON (object mapping secret key -> string value)

    Returns the same terminal CapabilityResult written to stdout. No input line
    returns None.
    """
    job_line = sys.stdin.readline()
    if not job_line:
        return None

    try:
        job = CapabilityJob.model_validate_json(job_line)
    except Exception:
        return _emit_result(_input_error("", "invalid capability job"))

    secrets_line = sys.stdin.readline()
    secret_values: dict[str, str] = {}
    if secrets_line:
        try:
            parsed_secrets = json.loads(secrets_line)
        except json.JSONDecodeError:
            return _emit_result(_input_error(job.job_id, "invalid secret payload"))
        if not isinstance(parsed_secrets, dict) or not all(
            isinstance(key, str) and isinstance(value, str)
            for key, value in parsed_secrets.items()
        ):
            return _emit_result(_input_error(job.job_id, "invalid secret payload"))
        secret_values = parsed_secrets

    from . import CAPABILITY_ABI_VERSION
    if job.abi_version != CAPABILITY_ABI_VERSION:
        result = CapabilityResult(
            abi_version=CAPABILITY_ABI_VERSION,
            job_id=job.job_id,
            state="failed",
            result=None,
            output=None,
            artifacts=[],
            diagnostics=CapabilityDiagnostics(
                stderr_lines=[],
                error_code="abi_mismatch",
                error_message=(
                    f"ABI version mismatch: worker={CAPABILITY_ABI_VERSION}, "
                    f"job={job.abi_version}"
                ),
            ),
            finished_at=__import__("time").time(),
        )
        return _emit_result(result)

    ctx = WorkerContext(
        job=job,
        secret_values=secret_values,
        progress_sender=progress_sender,
        cancel_checker=cancel_checker,
    )

    try:
        result = handler(ctx)
    except KeyboardInterrupt:
        result = ctx.error("Job cancelled", code="cancelled")
    except Exception as exc:
        result = ctx.error(
            f"Worker error: {exc}",
            code="worker_error",
            stderr_lines=[str(exc)],
        )

    if not isinstance(result, CapabilityResult):
        result = ctx.error(
            "Worker handler did not return CapabilityResult",
            code="worker_protocol",
        )

    return _emit_result(result)

__all__ = [
    "WorkerContext",
    "run_worker",
    "Grants",
    "Boundary",
    "CapabilityArtifact",
    "CapabilityProgress",
    "CapabilityResult",
    "CapabilityDiagnostics",
]

# Re-export for convenience
Grants = CapabilityGrants
Boundary = CapabilityBoundary