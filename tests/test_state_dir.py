from __future__ import annotations

from pathlib import Path

from packages.state_dir import default_state_dir


def test_state_dir_prefers_explicit_override(monkeypatch, tmp_path: Path) -> None:
    target = tmp_path / "delta-state"
    monkeypatch.setenv("DELTA_STATE_DIR", str(target))
    assert default_state_dir() == target


def test_state_dir_uses_home_on_non_windows(monkeypatch, tmp_path: Path) -> None:
    monkeypatch.delenv("DELTA_STATE_DIR", raising=False)
    monkeypatch.setenv("HOME", str(tmp_path))
    if __import__("os").name != "nt":
        assert default_state_dir() == tmp_path / ".config" / "delta"
