"""The Explore Runtime Pack is reproducible, lists its files exactly, and fails closed."""

from __future__ import annotations

import hashlib
import json
import shutil
import tarfile
from pathlib import Path

import pytest

from packages.explore_pack import runtime_pack


@pytest.fixture
def built(tmp_path: Path) -> Path:
    out = tmp_path / "out"
    runtime_pack.build(out)
    return out


def _extract(built: Path, destination: Path) -> Path:
    manifest = json.loads(
        (built / runtime_pack.MANIFEST_NAME).read_text(encoding="utf-8")
    )
    archive = built / manifest["archive"]["name"]
    destination.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive) as tar:
        tar.extractall(destination, filter="data")
    shutil.copy(
        built / runtime_pack.MANIFEST_NAME, destination / runtime_pack.MANIFEST_NAME
    )
    return destination


def test_two_builds_are_byte_identical(tmp_path: Path) -> None:
    first = tmp_path / "first"
    second = tmp_path / "second"
    runtime_pack.build(first)
    runtime_pack.build(second)

    assert (first / runtime_pack.MANIFEST_NAME).read_bytes() == (
        second / runtime_pack.MANIFEST_NAME
    ).read_bytes()
    manifest = json.loads(
        (first / runtime_pack.MANIFEST_NAME).read_text(encoding="utf-8")
    )
    name = manifest["archive"]["name"]
    assert (first / name).read_bytes() == (second / name).read_bytes()


def test_the_archive_digest_in_the_manifest_is_the_real_digest(built: Path) -> None:
    manifest = json.loads(
        (built / runtime_pack.MANIFEST_NAME).read_text(encoding="utf-8")
    )
    archive = built / manifest["archive"]["name"]

    assert (
        hashlib.sha256(archive.read_bytes()).hexdigest()
        == manifest["archive"]["sha256"]
    )
    assert archive.stat().st_size == manifest["archive"]["size"]


def test_the_archive_has_the_worker_and_no_tests(built: Path) -> None:
    manifest = json.loads(
        (built / runtime_pack.MANIFEST_NAME).read_text(encoding="utf-8")
    )
    with tarfile.open(built / manifest["archive"]["name"]) as tar:
        names = tar.getnames()

    assert "packages/__init__.py" in names
    assert "packages/explore_pack/worker.py" in names
    assert "packages/explore_pack/engine.py" in names
    assert not [
        name for name in names if "/tests/" in name or name.endswith("test_worker.py")
    ]


def test_an_extracted_pack_verifies_against_its_manifest(
    built: Path, tmp_path: Path
) -> None:
    root = _extract(built, tmp_path / "installed")

    assert runtime_pack.verify(root) == []


def test_a_changed_file_is_reported(built: Path, tmp_path: Path) -> None:
    root = _extract(built, tmp_path / "installed")
    (root / "packages" / "explore_pack" / "engine.py").write_text(
        "# changed\n", encoding="utf-8"
    )

    problems = runtime_pack.verify(root)

    assert problems == ["hash mismatch: packages/explore_pack/engine.py"]


def test_a_missing_file_is_reported(built: Path, tmp_path: Path) -> None:
    root = _extract(built, tmp_path / "installed")
    (root / "packages" / "explore_pack" / "stats.py").unlink()

    assert runtime_pack.verify(root) == ["missing file: packages/explore_pack/stats.py"]


def test_a_missing_manifest_fails_closed(tmp_path: Path) -> None:
    assert runtime_pack.verify(tmp_path) == ["manifest.json is missing"]


def test_the_output_is_exactly_the_archive_and_the_manifest(tmp_path: Path) -> None:
    out = tmp_path / "out"
    runtime_pack.build(out, version=runtime_pack.PACK_VERSION)

    assert sorted(path.name for path in out.iterdir()) == [
        runtime_pack.ARCHIVE_NAME,
        runtime_pack.MANIFEST_NAME,
    ]


def test_a_version_that_differs_from_the_pack_is_refused(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="does not match the pack's PACK_VERSION"):
        runtime_pack.build(tmp_path / "out", version="9.9.9")
    assert not (tmp_path / "out").exists()


def test_a_path_that_escapes_the_pack_is_refused(tmp_path: Path) -> None:
    root = tmp_path / "pack"
    root.mkdir()
    (tmp_path / "outside.py").write_text("x = 1\n", encoding="utf-8")
    digest = hashlib.sha256(b"x = 1\n").hexdigest()
    manifest = {
        "format": "delta.explore-runtime",
        "version": 1,
        "files": [{"path": "../outside.py", "sha256": digest, "size": 6}],
    }
    (root / runtime_pack.MANIFEST_NAME).write_text(
        json.dumps(manifest), encoding="utf-8"
    )

    assert runtime_pack.verify(root) == ["path escapes the pack: ../outside.py"]
