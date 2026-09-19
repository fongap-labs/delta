"""Tests for delta-worker-sdk."""

import json
from io import StringIO
from unittest.mock import patch

import pytest

from delta_worker_sdk import (
    CAPABILITY_ABI_VERSION,
    WorkerContext,
    run_worker,
    CapabilityJob,
    CapabilityResult,
    CapabilityGrants,
)


def make_job(**overrides) -> CapabilityJob:
    base = {
        "abi_version": CAPABILITY_ABI_VERSION,
        "capability_id": "test.cap",
        "job_id": "job-1",
        "arguments": {"path": "input.txt"},
        "grants": CapabilityGrants(secrets=["API_KEY"]),
        "workspace": "/ws",
    }
    base.update(overrides)
    return CapabilityJob(**base)


def test_context_reads_arguments():
    job = make_job(arguments={"query": "hello"})
    ctx = WorkerContext(
        job=job,
        secret_values={},
        progress_sender=lambda p: None,
        cancel_checker=lambda: False,
    )
    assert ctx.capability_id == "test.cap"
    assert ctx.job_id == "job-1"
    assert ctx.arguments == {"query": "hello"}


def test_context_secret_access_granted():
    job = make_job(grants={"secrets": ["API_KEY", "DB_URL"]})
    ctx = WorkerContext(
        job=job,
        secret_values={"API_KEY": "sk-xxx", "DB_URL": "postgres://..."},
        progress_sender=lambda p: None,
        cancel_checker=lambda: False,
    )
    assert ctx.secret("API_KEY") == "sk-xxx"
    assert ctx.secret("DB_URL") == "postgres://..."
    assert ctx.secret("NOT_GRANTED") is None


def test_context_secret_or_raise():
    job = make_job(grants={"secrets": ["API_KEY"]})
    ctx = WorkerContext(
        job=job,
        secret_values={"API_KEY": "sk-xxx"},
        progress_sender=lambda p: None,
        cancel_checker=lambda: False,
    )
    assert ctx.secret_or_raise("API_KEY") == "sk-xxx"
    with pytest.raises(ValueError):
        ctx.secret_or_raise("MISSING")


def test_context_result_completed():
    job = make_job()
    ctx = WorkerContext(
        job=job,
        secret_values={},
        progress_sender=lambda p: None,
        cancel_checker=lambda: False,
    )
    result = ctx.result(output="done")
    assert result.state == "completed"
    assert result.job_id == "job-1"
    assert result.abi_version == CAPABILITY_ABI_VERSION
    assert result.output == "done"


def test_context_result_error():
    job = make_job()
    ctx = WorkerContext(
        job=job,
        secret_values={},
        progress_sender=lambda p: None,
        cancel_checker=lambda: False,
    )
    result = ctx.error("boom", code="E_RUN")
    assert result.state == "failed"
    assert result.diagnostics is not None
    assert result.diagnostics.error_code == "E_RUN"
    assert result.diagnostics.error_message == "boom"


def test_run_worker_success():
    job = make_job()
    secret_values = {"API_KEY": "sk-xxx"}

    captured = []

    def mock_progress(p):
        captured.append(("progress", p.model_dump_json()))

    def mock_cancel():
        return False

    stdin_data = job.model_dump_json() + "\n" + json.dumps(secret_values) + "\n"

    with patch("sys.stdin", StringIO(stdin_data)):
        with patch("sys.stdout", StringIO()) as mock_stdout:
            run_worker(
                lambda ctx: ctx.result(output="ok"),
                progress_sender=mock_progress,
                cancel_checker=mock_cancel,
            )
            output = mock_stdout.getvalue().strip()
            result = CapabilityResult.model_validate_json(output)
            assert result.state == "completed"
            assert result.output == "ok"


def test_run_worker_abi_mismatch():
    job = make_job(abi_version=99)
    stdin_data = job.model_dump_json() + "\n{}\n"

    with patch("sys.stdin", StringIO(stdin_data)):
        with patch("sys.stdout", StringIO()) as mock_stdout:
            run_worker(
                lambda ctx: ctx.result(),
                progress_sender=lambda p: None,
                cancel_checker=lambda: False,
            )
            output = mock_stdout.getvalue().strip()
            result = CapabilityResult.model_validate_json(output)
            assert result.state == "failed"
            assert result.diagnostics is not None
            assert result.diagnostics.error_code == "abi_mismatch"


def test_run_worker_secrets_delivered():
    job = make_job()

    def handler(ctx: WorkerContext) -> CapabilityResult:
        assert ctx.secret("API_KEY") == "sk-xxx"
        return ctx.result(result={"ok": True})

    stdin_data = job.model_dump_json() + "\n" + json.dumps({"API_KEY": "sk-xxx"}) + "\n"

    with patch("sys.stdin", StringIO(stdin_data)):
        with patch("sys.stdout", StringIO()) as mock_stdout:
            run_worker(
                handler,
                progress_sender=lambda p: None,
                cancel_checker=lambda: False,
            )
            output = mock_stdout.getvalue().strip()
            result = CapabilityResult.model_validate_json(output)
            assert result.state == "completed"


def test_run_worker_progress_emission():
    job = make_job()

    progress_frames = []

    def handler(ctx: WorkerContext) -> CapabilityResult:
        ctx.emit_progress(0.5, stage="processing", message="half done")
        return ctx.result(result={"ok": True})

    stdin_data = job.model_dump_json() + "\n{}\n"

    with patch("sys.stdin", StringIO(stdin_data)):
        with patch("sys.stdout", StringIO()):
            run_worker(
                handler,
                progress_sender=lambda p: progress_frames.append(p),
                cancel_checker=lambda: False,
            )

    assert len(progress_frames) == 1
    frame = progress_frames[0]
    assert frame.type == "progress"
    assert frame.data["fraction"] == 0.5
    assert frame.data["stage"] == "processing"
    assert frame.data["message"] == "half done"