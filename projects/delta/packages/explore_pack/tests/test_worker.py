"""The explore worker speaks the capability ABI and refuses anything it cannot verify."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from pathlib import Path

import pytest

from packages.explore_pack import worker

WORKER = Path(worker.__file__).resolve()


@pytest.fixture
def data_csv(tmp_path: Path) -> Path:
    path = tmp_path / "data.csv"
    path.write_text("x,y\n1,2.1\n2,3.9\n3,6.2\n4,8.1\n", encoding="utf-8")
    return path


def _job(
    path: Path, capability: str, arguments: dict[str, object], **overrides: object
) -> dict:
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    job: dict[str, object] = {
        "abi_version": 2,
        "capability_id": capability,
        "job_id": "job-1",
        "input_files": [
            {"path": str(path), "sha256": digest, "size": path.stat().st_size}
        ],
        "arguments": arguments,
    }
    job.update(overrides)
    return job


def test_a_regression_job_completes_with_an_exploratory_result(data_csv: Path) -> None:
    result = worker.run_job(
        _job(
            data_csv,
            "statistics.explore.regression",
            {"x_column": "x", "y_column": "y"},
        )
    )

    assert result["state"] == "completed"
    assert result["abi_version"] == 2
    assert result["job_id"] == "job-1"
    assert result["result"]["assurance"] == "exploratory"
    assert result["result"]["method"]["id"] == "explore.regression"
    assert result["diagnostics"] is None


def test_a_changed_input_is_refused(data_csv: Path) -> None:
    job = _job(data_csv, "statistics.explore.describe", {"column": "y"})
    data_csv.write_text("x,y\n1,9.9\n", encoding="utf-8")

    result = worker.run_job(job)

    assert result["state"] == "failed"
    assert result["diagnostics"]["error_code"] == "hash_changed"
    assert result["result"] is None


def test_an_unknown_capability_is_refused(data_csv: Path) -> None:
    result = worker.run_job(_job(data_csv, "statistics.r.f2", {}))

    assert result["state"] == "failed"
    assert result["diagnostics"]["error_code"] == "unknown_capability"


def test_an_abi_mismatch_is_refused(data_csv: Path) -> None:
    result = worker.run_job(
        _job(data_csv, "statistics.explore.describe", {"column": "y"}, abi_version=1)
    )

    assert result["diagnostics"]["error_code"] == "abi_mismatch"


def test_a_missing_argument_is_an_invalid_input(data_csv: Path) -> None:
    result = worker.run_job(_job(data_csv, "statistics.explore.describe", {}))

    assert result["state"] == "failed"
    assert result["diagnostics"]["error_code"] == "invalid_input"


def test_exactly_one_input_file_is_required(data_csv: Path) -> None:
    job = _job(data_csv, "statistics.explore.describe", {"column": "y"}, input_files=[])

    assert worker.run_job(job)["diagnostics"]["error_code"] == "arguments"


def test_the_worker_runs_in_isolated_mode_over_stdin_and_stdout(data_csv: Path) -> None:
    job = _job(data_csv, "statistics.explore.describe", {"column": "y"})

    completed = subprocess.run(
        [sys.executable, "-I", str(WORKER)],
        input=json.dumps(job),
        capture_output=True,
        text=True,
        check=True,
        timeout=60,
    )

    result = json.loads(completed.stdout)
    assert result["state"] == "completed"
    assert result["result"]["values"]["n"] == {"value": 4.0}
    assert completed.stderr == ""


def test_the_job_line_is_read_and_the_secret_line_is_ignored(data_csv: Path) -> None:
    job = _job(data_csv, "statistics.explore.describe", {"column": "y"})

    completed = subprocess.run(
        [sys.executable, "-I", str(WORKER)],
        input=json.dumps(job) + "\n{}\n",
        capture_output=True,
        text=True,
        check=True,
        timeout=60,
    )

    result = json.loads(completed.stdout)
    assert result["state"] == "completed"
    assert result["job_id"] == "job-1"


def test_invalid_json_on_stdin_yields_a_failed_result() -> None:
    completed = subprocess.run(
        [sys.executable, "-I", str(WORKER)],
        input="{not json",
        capture_output=True,
        text=True,
        check=True,
        timeout=60,
    )

    result = json.loads(completed.stdout)
    assert result["state"] == "failed"
    assert result["diagnostics"]["error_code"] == "invalid_job"
