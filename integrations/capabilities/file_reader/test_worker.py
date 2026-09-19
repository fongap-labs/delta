"""Tests for file reader capability worker."""

import json
import tempfile
from io import StringIO
from pathlib import Path
from unittest.mock import patch

from delta_worker_sdk import CAPABILITY_ABI_VERSION, CapabilityJob, CapabilityResult, run_worker
from integrations.capabilities.file_reader.worker import handle_file_read


def make_job(workspace: str, path: str, artifact_staging_dir: str | None = None) -> CapabilityJob:
    return CapabilityJob(
        abi_version=CAPABILITY_ABI_VERSION,
        capability_id="file.read",
        job_id="job-1",
        arguments={"path": path},
        grants={"secrets": []},
        workspace=workspace,
        artifact_staging_dir=artifact_staging_dir,
    )


def run_worker_and_get_result(job: CapabilityJob, secret_values: dict):
    """Run worker and return the final CapabilityResult (last JSON line)."""
    stdin_data = job.model_dump_json() + "\n" + json.dumps(secret_values) + "\n"

    with patch("sys.stdin", StringIO(stdin_data)):
        with patch("sys.stdout", StringIO()) as mock_stdout:
            run_worker(handle_file_read)
            output = mock_stdout.getvalue().strip()
            # Last line is the result
            last_line = output.split("\n")[-1]
            return CapabilityResult.model_validate_json(last_line)


def test_read_small_file():
    """Test reading a small file returns content inline."""
    with tempfile.TemporaryDirectory() as tmpdir:
        test_file = Path(tmpdir) / "test.txt"
        test_file.write_text("Hello, World!")

        job = make_job(tmpdir, "test.txt")
        result = run_worker_and_get_result(job, {})

        assert result.state == "completed"
        assert result.result is not None
        assert result.result["content"] == "Hello, World!"
        assert result.result["size"] == 13
        assert "sha256" in result.result


def test_read_large_file_staged():
    """Test reading a large file stages as artifact."""
    with tempfile.TemporaryDirectory() as tmpdir:
        test_file = Path(tmpdir) / "large.bin"
        test_file.write_bytes(b"x" * (2 * 1024 * 1024))

        job = make_job(tmpdir, "large.bin", artifact_staging_dir=tmpdir)
        result = run_worker_and_get_result(job, {})

        assert result.state == "completed"
        assert result.artifacts is not None
        assert len(result.artifacts) == 1
        artifact = result.artifacts[0]
        assert artifact.kind == "file"
        assert artifact.sha256 is not None
        assert artifact.size == 2 * 1024 * 1024


def test_read_nonexistent_file():
    """Test reading non-existent file returns error."""
    with tempfile.TemporaryDirectory() as tmpdir:
        job = make_job(tmpdir, "nonexistent.txt")
        result = run_worker_and_get_result(job, {})

        assert result.state == "failed"
        assert result.diagnostics is not None
        assert result.diagnostics.error_code == "not_found"


def test_read_outside_workspace():
    """Test reading file outside workspace is denied."""
    with tempfile.TemporaryDirectory() as tmpdir:
        outside_file = Path(tmpdir).parent / "outside.txt"
        outside_file.write_text("secret")

        job = make_job(tmpdir, f"../{outside_file.name}")
        result = run_worker_and_get_result(job, {})

        assert result.state == "failed"
        assert result.diagnostics is not None
        assert result.diagnostics.error_code == "permission_denied"


def test_missing_path_argument():
    """Test missing path argument returns error."""
    with tempfile.TemporaryDirectory() as tmpdir:
        job = CapabilityJob(
            abi_version=CAPABILITY_ABI_VERSION,
            capability_id="file.read",
            job_id="job-1",
            arguments={},
            grants={"secrets": []},
            workspace=tmpdir,
        )
        result = run_worker_and_get_result(job, {})

        assert result.state == "failed"
        assert result.diagnostics is not None
        assert result.diagnostics.error_code == "invalid_args"