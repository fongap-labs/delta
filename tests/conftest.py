"""Shared Foundation pytest fixtures."""

from __future__ import annotations

import pytest


@pytest.fixture(autouse=True)
def _isolated_state_dir(tmp_path, monkeypatch):
    """Give every test an isolated SecretStore/state directory."""
    monkeypatch.setenv("DELTA_STATE_DIR", str(tmp_path / "delta-state"))
    monkeypatch.delenv("DELTA_API_TOKEN", raising=False)


@pytest.fixture(autouse=True)
def _close_delta_core_client():
    """Close the process-wide DeltaCoreClient after each test."""
    yield
    try:
        from packages.delta_core_client import close_default_client

        close_default_client()
    except Exception:
        pass
