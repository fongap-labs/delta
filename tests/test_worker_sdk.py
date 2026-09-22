"""Tests for the Delta Worker SDK (R8.6)."""
from __future__ import annotations

import io
import json
import os
import tempfile
from unittest.mock import patch

from packages.worker_sdk import (
    CAPABILITY_ABI_VERSION,
    WorkerContext,
    WorkerError,
    run_worker,
)


def _make_job(**ov):
    j = {"abi_version": CAPABILITY_ABI_VERSION, "capability_id": "test.cap",
         "job_id": "job-1", "arguments": {"path": "input.txt"}, "workspace": "/ws",
         "grants": {"secrets": ["API_KEY"]}, "boundary": {}, "timeout_secs": 30,
         "artifact_staging_dir": None}
    j.update(ov)
    return j


class _Cap:
    def __init__(self): self.lines = []
    def write(self, s):
        for line in s.split("\n"):
            if line:
                self.lines.append(line)

    def flush(self):
        pass


def test_ctx_reads_args():
    ctx = WorkerContext(_make_job(arguments={"q": "hi"}), {})
    assert ctx.capability_id == "test.cap"
    assert ctx.job_id == "job-1"
    assert ctx.arguments == {"q": "hi"}


def test_ctx_secret():
    ctx = WorkerContext(_make_job(grants={"secrets": ["K"]}), {"K": "v", "U": "x"})
    assert ctx.secret("K") == "v"
    assert ctx.secret("U") is None


def test_ctx_result_ok():
    ctx = WorkerContext(_make_job(), {})
    r = ctx.result(is_ok=True, output="done")
    assert r["state"] == "completed"
    assert r["job_id"] == "job-1"
    assert r["abi_version"] == CAPABILITY_ABI_VERSION
    assert r["output"] == "done"


def test_ctx_error_result():
    ctx = WorkerContext(_make_job(), {})
    r = ctx.error_result("boom", code="E")
    assert r["state"] == "failed"
    assert r["diagnostics"]["error_code"] == "E"
    assert r["diagnostics"]["error_message"] == "boom"


def test_ctx_artifact():
    ctx = WorkerContext(_make_job(), {})
    a = ctx.artifact("/s/out.csv", "out.csv", kind="csv", sha256="ab", size=5)
    assert a["staging_path"] == "/s/out.csv"
    assert a["kind"] == "csv"
    assert a["sha256"] == "ab"


def test_run_success():
    cap = _Cap()
    job = _make_job()
    with patch("sys.stdin", io.StringIO(json.dumps(job) + "\n{}\n")):
        with patch("sys.stdout", cap):
            run_worker(lambda ctx: ctx.result(is_ok=True, output="ok"))
    r = json.loads(cap.lines[-1])
    assert r["state"] == "completed"
    assert r["output"] == "ok"


def test_run_abi_mismatch():
    cap = _Cap()
    job = _make_job(abi_version=99)
    with patch("sys.stdin", io.StringIO(json.dumps(job) + "\n{}\n")):
        with patch("sys.stdout", cap):
            run_worker(lambda ctx: ctx.result())
    r = json.loads(cap.lines[-1])
    assert r["state"] == "failed"
    assert "mismatch" in r["diagnostics"]["error_message"]


def test_run_handler_error():
    cap = _Cap()
    job = _make_job()

    def h(ctx): raise WorkerError("fail", code="E_C")
    with patch("sys.stdin", io.StringIO(json.dumps(job) + "\n{}\n")):
        with patch("sys.stdout", cap):
            run_worker(h)
    r = json.loads(cap.lines[-1])
    assert r["state"] == "failed"
    assert r["diagnostics"]["error_code"] == "E_C"


def test_run_unhandled():
    cap = _Cap()
    job = _make_job()

    def h(ctx): raise RuntimeError("crash")
    with patch("sys.stdin", io.StringIO(json.dumps(job) + "\n{}\n")):
        with patch("sys.stdout", cap):
            run_worker(h)
    r = json.loads(cap.lines[-1])
    assert r["state"] == "failed"
    assert r["diagnostics"]["error_code"] == "worker_unhandled"


def test_run_progress():
    cap = _Cap()
    job = _make_job()

    def h(ctx):
        ctx.emit_progress(0.5, stage="proc")
        return ctx.result(is_ok=True)
    with patch("sys.stdin", io.StringIO(json.dumps(job) + "\n{}\n")):
        with patch("sys.stdout", cap):
            run_worker(h)
    prog = json.loads(cap.lines[0])
    assert prog["type"] == "progress"
    assert prog["data"]["fraction"] == 0.5
    r = json.loads(cap.lines[-1])
    assert r["state"] == "completed"


def test_run_secrets_delivered():
    cap = _Cap()
    job = _make_job(grants={"secrets": ["API_KEY"]})

    def h(ctx):
        assert ctx.secret("API_KEY") == "sk-xxx"
        return ctx.result(is_ok=True)
    with patch("sys.stdin", io.StringIO(
        json.dumps(job) + "\n" + json.dumps({"API_KEY": "sk-xxx"}) + "\n"
    )):
        with patch("sys.stdout", cap):
            run_worker(h)
    r = json.loads(cap.lines[-1])
    assert r["state"] == "completed"


def test_stage_artifact():
    with tempfile.TemporaryDirectory() as d:
        ctx = WorkerContext(_make_job(artifact_staging_dir=d), {})
        path = ctx.stage_artifact("out.txt", b"hello")
        assert os.path.exists(path)
        with open(path, "rb") as f:
            assert f.read() == b"hello"
