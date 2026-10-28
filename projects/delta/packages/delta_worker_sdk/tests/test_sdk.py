"""Tests for delta-worker-sdk."""

import json
from io import StringIO
from unittest.mock import patch

import pytest

from delta_worker_sdk import (
    CAPABILITY_ABI_VERSION,
    WorkerContext,
    run_persistent_worker,
    run_worker,
    CapabilityJob,
    CapabilityResult,
    CapabilityGrants,
    CapabilityBoundary,
    CapabilityInputFile,
    CapabilityArtifact,
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


def test_protocol_fields_match_capability_abi():
    input_file = CapabilityInputFile(path="input.txt", sha256=None, size=12)
    assert input_file.sha256 is None
    assert input_file.size == 12

    grants = CapabilityGrants(
        read_roots=["/read"],
        write_roots=["/write"],
        network=["https://api.example.test:443"],
        secrets=["API_KEY"],
        exec=True,
        execution_epoch=100.0,
        expires_at=200.0,
    )
    assert grants.exec is True
    assert grants.execution_epoch == 100.0
    assert grants.expires_at == 200.0

    boundary = CapabilityBoundary(
        read_roots=["/read"],
        write_roots=["/write"],
        network=["https://api.example.test:443"],
        exec=True,
        secrets=["API_KEY"],
        destructive=True,
        provenance="approval:test",
        execution_epoch=100.0,
        expires_at=200.0,
    )
    assert boundary.exec is True
    assert boundary.destructive is True
    assert boundary.provenance == "approval:test"
    assert boundary.execution_epoch == 100.0
    assert boundary.expires_at == 200.0

    artifact = CapabilityArtifact(
        staging_path="/tmp/out",
        relative_path="out.txt",
    )
    assert artifact.kind is None


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


def test_context_result_accepts_any_json_value():
    job = make_job()
    ctx = WorkerContext(
        job=job,
        secret_values={},
        progress_sender=lambda p: None,
        cancel_checker=lambda: False,
    )
    assert ctx.result(result="ok").result == "ok"
    assert ctx.result(result=[1, "two", True]).result == [1, "two", True]
    assert ctx.result(result=42).result == 42


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


def test_run_worker_returns_terminal_result():
    job = make_job()
    stdin_data = job.model_dump_json() + "\n{}\n"

    with patch("sys.stdin", StringIO(stdin_data)):
        with patch("sys.stdout", StringIO()) as mock_stdout:
            terminal = run_worker(
                lambda ctx: ctx.result(result={"ok": True}),
                progress_sender=lambda p: None,
                cancel_checker=lambda: False,
            )
            assert terminal is not None
            assert terminal.state == "completed"
            assert terminal.result == {"ok": True}
            assert CapabilityResult.model_validate_json(mock_stdout.getvalue().strip()).state == "completed"


def test_run_worker_rejects_invalid_job_json():
    with patch("sys.stdin", StringIO("{not-json}\n{}\n")):
        with patch("sys.stdout", StringIO()) as mock_stdout:
            terminal = run_worker(
                lambda ctx: ctx.result(),
                progress_sender=lambda p: None,
                cancel_checker=lambda: False,
            )
            assert terminal is not None
            assert terminal.state == "failed"
            assert terminal.diagnostics is not None
            assert terminal.diagnostics.error_code == "worker_input"
            assert terminal.job_id == ""
            assert CapabilityResult.model_validate_json(mock_stdout.getvalue().strip()).state == "failed"


def test_run_worker_rejects_invalid_secret_payload():
    job = make_job()
    stdin_data = job.model_dump_json() + "\n[]\n"

    with patch("sys.stdin", StringIO(stdin_data)):
        with patch("sys.stdout", StringIO()) as mock_stdout:
            terminal = run_worker(
                lambda ctx: ctx.result(),
                progress_sender=lambda p: None,
                cancel_checker=lambda: False,
            )
            assert terminal is not None
            assert terminal.state == "failed"
            assert terminal.diagnostics is not None
            assert terminal.diagnostics.error_code == "worker_input"
            assert terminal.job_id == "job-1"
            assert CapabilityResult.model_validate_json(mock_stdout.getvalue().strip()).state == "failed"


def test_run_worker_rejects_non_result_handler_value():
    job = make_job()
    stdin_data = job.model_dump_json() + "\n{}\n"

    with patch("sys.stdin", StringIO(stdin_data)):
        with patch("sys.stdout", StringIO()) as mock_stdout:
            terminal = run_worker(
                lambda ctx: {"ok": True},  # type: ignore[return-value]
                progress_sender=lambda p: None,
                cancel_checker=lambda: False,
            )
            assert terminal is not None
            assert terminal.state == "failed"
            assert terminal.diagnostics is not None
            assert terminal.diagnostics.error_code == "worker_protocol"
            assert CapabilityResult.model_validate_json(mock_stdout.getvalue().strip()).state == "failed"



def test_run_persistent_worker_handles_multiple_jobs():
    first = make_job(job_id="job-1", arguments={"value": 1})
    second = make_job(job_id="job-2", arguments={"value": 2})
    stdin_data = (
        first.model_dump_json()
        + "\n{}\n"
        + second.model_dump_json()
        + "\n{}\n"
    )

    with patch("sys.stdin", StringIO(stdin_data)):
        with patch("sys.stdout", StringIO()) as mock_stdout:
            code = run_persistent_worker(
                lambda ctx: ctx.result(result=ctx.arguments),
                progress_sender=lambda p: None,
                cancel_checker=lambda: False,
            )

    assert code == 0
    results = [
        CapabilityResult.model_validate_json(line)
        for line in mock_stdout.getvalue().splitlines()
    ]
    assert [result.job_id for result in results] == ["job-1", "job-2"]
    assert [result.result for result in results] == [{"value": 1}, {"value": 2}]


def test_run_persistent_worker_recovers_after_invalid_job_frame():
    valid = make_job(job_id="job-2", arguments={"ok": True})
    stdin_data = "{not-json}\n{}\n" + valid.model_dump_json() + "\n{}\n"

    with patch("sys.stdin", StringIO(stdin_data)):
        with patch("sys.stdout", StringIO()) as mock_stdout:
            code = run_persistent_worker(
                lambda ctx: ctx.result(result=ctx.arguments),
                progress_sender=lambda p: None,
                cancel_checker=lambda: False,
            )

    assert code == 0
    results = [
        CapabilityResult.model_validate_json(line)
        for line in mock_stdout.getvalue().splitlines()
    ]
    assert len(results) == 2
    assert results[0].state == "failed"
    assert results[0].diagnostics is not None
    assert results[0].diagnostics.error_code == "worker_input"
    assert results[1].state == "completed"
    assert results[1].job_id == "job-2"


def test_run_persistent_worker_recovers_after_invalid_secret_frame():
    first = make_job(job_id="job-1")
    second = make_job(job_id="job-2")
    stdin_data = (
        first.model_dump_json()
        + "\n[]\n"
        + second.model_dump_json()
        + '\n{"API_KEY":"secret"}\n'
    )

    with patch("sys.stdin", StringIO(stdin_data)):
        with patch("sys.stdout", StringIO()) as mock_stdout:
            code = run_persistent_worker(
                lambda ctx: ctx.result(result={"secret": ctx.secret("API_KEY")}),
                progress_sender=lambda p: None,
                cancel_checker=lambda: False,
            )

    assert code == 0
    results = [
        CapabilityResult.model_validate_json(line)
        for line in mock_stdout.getvalue().splitlines()
    ]
    assert results[0].state == "failed"
    assert results[0].diagnostics is not None
    assert results[0].diagnostics.error_code == "worker_input"
    assert results[1].state == "completed"
    assert results[1].result == {"secret": "secret"}


def test_run_persistent_worker_reports_missing_secret_frame():
    job = make_job(job_id="job-1")

    with patch("sys.stdin", StringIO(job.model_dump_json() + "\n")):
        with patch("sys.stdout", StringIO()) as mock_stdout:
            code = run_persistent_worker(
                lambda ctx: ctx.result(),
                progress_sender=lambda p: None,
                cancel_checker=lambda: False,
            )

    assert code == 0
    result = CapabilityResult.model_validate_json(mock_stdout.getvalue().strip())
    assert result.state == "failed"
    assert result.job_id == "job-1"
    assert result.diagnostics is not None
    assert result.diagnostics.error_code == "worker_input"
    assert result.diagnostics.error_message == "missing secret payload"


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