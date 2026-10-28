"""The Office runtime manifest lists exactly the worker's files and fails closed on any change."""

from __future__ import annotations

import json
import shutil
from pathlib import Path

from packages.office_worker import runtime_pack


def _packaged_copy(destination: Path) -> Path:
    """A copy of the runtime's files under `destination`, as a packaging step would place them."""
    for name in runtime_pack.source_files():
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(runtime_pack._REPO_ROOT / name, target)
    return destination


def test_the_manifest_lists_the_worker_and_no_tests() -> None:
    names = runtime_pack.source_files()

    assert "packages/office_worker/worker.py" in names
    assert "packages/office_worker/docx_build.py" in names
    assert "packages/__init__.py" in names
    assert not [name for name in names if "/tests/" in name or "__pycache__" in name]


def test_the_manifest_is_the_same_on_every_build() -> None:
    assert runtime_pack.manifest() == runtime_pack.manifest()


def test_a_packaged_copy_verifies_against_its_manifest(tmp_path: Path) -> None:
    root = _packaged_copy(tmp_path / "installed")
    runtime_pack.write(root)

    assert runtime_pack.verify(root) == []


def test_a_changed_file_is_a_hash_mismatch(tmp_path: Path) -> None:
    root = _packaged_copy(tmp_path / "installed")
    runtime_pack.write(root)
    (root / "packages" / "office_worker" / "worker.py").write_text(
        "import os\n", encoding="utf-8"
    )

    assert runtime_pack.verify(root) == [
        "hash mismatch: packages/office_worker/worker.py"
    ]


def test_a_removed_file_is_missing(tmp_path: Path) -> None:
    root = _packaged_copy(tmp_path / "installed")
    runtime_pack.write(root)
    (root / "packages" / "office_worker" / "spec.py").unlink()

    assert runtime_pack.verify(root) == ["missing file: packages/office_worker/spec.py"]


def test_a_missing_manifest_fails_closed(tmp_path: Path) -> None:
    root = _packaged_copy(tmp_path / "installed")

    assert runtime_pack.verify(root) == ["manifest.json is missing"]


def test_a_path_that_escapes_the_runtime_is_refused(tmp_path: Path) -> None:
    root = tmp_path / "runtime"
    root.mkdir()
    document = {
        "format": runtime_pack.FORMAT,
        "version": 1,
        "files": [{"path": "../outside.py", "sha256": "00", "size": 0}],
    }
    (root / runtime_pack.MANIFEST_NAME).write_text(
        json.dumps(document), encoding="utf-8"
    )

    assert runtime_pack.verify(root) == ["path escapes the pack: ../outside.py"]


def test_the_command_line_writes_the_manifest_into_a_packaged_copy(
    tmp_path: Path,
) -> None:
    root = _packaged_copy(tmp_path / "installed")

    assert runtime_pack.main(["--root", str(root)]) == 0
    assert runtime_pack.verify(root) == []
