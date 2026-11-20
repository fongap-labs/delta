"""The shared Python runtime pack is reproducible, lists its files exactly, and fails closed."""

from __future__ import annotations

import io
import json
import tarfile
from pathlib import Path

import pytest

from packages.python_runtime import runtime_pack

LINUX = "x86_64-unknown-linux-gnu"
WINDOWS = "x86_64-pc-windows-msvc"
REQUIREMENTS = "python-docx==1.2.0\nopenpyxl==3.1.5\n"


def _interpreter_archive(path: Path) -> tuple[Path, str]:
    """A small stand-in for a CPython install_only archive: an interpreter and a link to it."""
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
        directory = tarfile.TarInfo("python")
        directory.type = tarfile.DIRTYPE
        archive.addfile(directory)
        interpreter = tarfile.TarInfo("python/bin/python3")
        body = b"#!/bin/sh\n"
        interpreter.size = len(body)
        interpreter.mode = 0o755
        archive.addfile(interpreter, io.BytesIO(body))
        link = tarfile.TarInfo("python/bin/python")
        link.type = tarfile.SYMTYPE
        link.linkname = "python3"
        archive.addfile(link)
        stdlib = b"# stdlib\n"
        module = tarfile.TarInfo("python/lib/python3.12/os.py")
        module.size = len(stdlib)
        archive.addfile(module, io.BytesIO(stdlib))
    data = buffer.getvalue()
    path.write_bytes(data)
    return path, runtime_pack.sha256_of(data)


def _install(requirements: str, site_packages: Path, platform: str) -> None:
    """Stands in for `uv pip install`: records what was asked for, without touching the network."""
    site_packages.mkdir(parents=True, exist_ok=True)
    (site_packages / "installed.txt").write_text(
        f"{platform}\n{requirements}", encoding="utf-8"
    )


def _build(out: Path, tmp_path: Path, **overrides: object) -> dict:
    tmp_path.mkdir(parents=True, exist_ok=True)
    archive, digest = _interpreter_archive(tmp_path / "python.tar.gz")
    options: dict[str, object] = {
        "interpreter_archive": archive,
        "interpreter_sha256": digest,
        "platform": LINUX,
        "requirements": REQUIREMENTS,
        "install": _install,
    }
    options.update(overrides)
    return runtime_pack.build(out, **options)  # type: ignore[arg-type]


def _extract(archive: Path, destination: Path) -> Path:
    destination.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive, "r:gz") as tar:
        tar.extractall(destination, filter="data")
    return destination


@pytest.fixture
def built(tmp_path: Path) -> tuple[Path, dict]:
    out = tmp_path / "out"
    return out, _build(out, tmp_path)


def test_the_extracted_pack_verifies_against_the_manifest_inside_it(
    built: tuple[Path, dict], tmp_path: Path
) -> None:
    out, _ = built
    root = _extract(out / runtime_pack.ARCHIVE_NAME, tmp_path / "root")

    assert runtime_pack.verify(root) == []


def test_the_interpreter_keeps_its_mode_and_its_links(
    built: tuple[Path, dict], tmp_path: Path
) -> None:
    out, _ = built
    root = _extract(out / runtime_pack.ARCHIVE_NAME, tmp_path / "root")

    assert (root / "python/bin/python3").stat().st_mode & 0o111
    assert (root / "python/bin/python").is_symlink()
    assert (root / "python/bin/python").resolve() == (
        root / "python/bin/python3"
    ).resolve()


def test_the_same_inputs_give_the_same_bytes(tmp_path: Path) -> None:
    first = _build(tmp_path / "first", tmp_path / "a")
    second = _build(tmp_path / "second", tmp_path / "b")

    assert (tmp_path / "first" / runtime_pack.ARCHIVE_NAME).read_bytes() == (
        tmp_path / "second" / runtime_pack.ARCHIVE_NAME
    ).read_bytes()
    assert first == second


def test_the_release_manifest_pins_the_archive(built: tuple[Path, dict]) -> None:
    out, release = built
    archive = (out / runtime_pack.ARCHIVE_NAME).read_bytes()

    assert release["archive"] == {
        "name": runtime_pack.ARCHIVE_NAME,
        "sha256": runtime_pack.sha256_of(archive),
        "size": len(archive),
    }
    assert (
        json.loads((out / runtime_pack.MANIFEST_NAME).read_text(encoding="utf-8"))
        == release
    )


def test_the_workers_and_the_libraries_are_in_the_pack_without_tests(
    built: tuple[Path, dict], tmp_path: Path
) -> None:
    out, release = built
    root = _extract(out / runtime_pack.ARCHIVE_NAME, tmp_path / "root")
    paths = {entry["path"] for entry in release["files"]}

    assert (root / "packages/office_worker/worker.py").is_file()
    assert (root / "packages/explore_pack/worker.py").is_file()
    assert (root / "python/lib/python3.12/site-packages/installed.txt").is_file()
    assert not any("/tests/" in path or path.startswith("tests/") for path in paths)


def test_an_interpreter_that_does_not_match_its_digest_is_refused(
    tmp_path: Path,
) -> None:
    archive, _ = _interpreter_archive(tmp_path / "python.tar.gz")

    with pytest.raises(ValueError, match="pinned SHA-256"):
        runtime_pack.build(
            tmp_path / "out",
            interpreter_archive=archive,
            interpreter_sha256="0" * 64,
            platform=LINUX,
            requirements=REQUIREMENTS,
            install=_install,
        )
    assert not (tmp_path / "out").exists()


def test_a_version_other_than_the_pack_version_is_refused(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="PACK_VERSION"):
        _build(tmp_path / "out", tmp_path, version="9.9.9")


def test_a_platform_without_a_known_layout_is_refused(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="unsupported platform"):
        _build(tmp_path / "out", tmp_path, platform="aarch64-apple-darwin")


def test_a_windows_pack_places_the_interpreter_and_the_libraries_in_their_layout(
    tmp_path: Path,
) -> None:
    archive = tmp_path / "python.tar.gz"
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as tar:
        body = b"MZ"
        entry = tarfile.TarInfo("python/python.exe")
        entry.size = len(body)
        tar.addfile(entry, io.BytesIO(body))
    archive.write_bytes(buffer.getvalue())
    digest = runtime_pack.sha256_of(buffer.getvalue())

    release = runtime_pack.build(
        tmp_path / "out",
        interpreter_archive=archive,
        interpreter_sha256=digest,
        platform=WINDOWS,
        requirements=REQUIREMENTS,
        install=_install,
    )
    root = _extract(tmp_path / "out" / runtime_pack.ARCHIVE_NAME, tmp_path / "root")

    assert release["python"] == {"version": "3.12", "interpreter": "python/python.exe"}
    assert (root / "python/Lib/site-packages/installed.txt").is_file()
    assert runtime_pack.verify(root) == []


def test_the_office_closure_leaves_out_the_projects_other_dependencies(
    tmp_path: Path,
) -> None:
    lock = tmp_path / "uv.lock"
    lock.write_text(
        """
version = 1

[[package]]
name = "delta"
version = "0.1.0"
dependencies = [
    { name = "ddgs" },
    { name = "python-docx" },
]

[[package]]
name = "ddgs"
version = "9.16.0"
dependencies = [
    { name = "primp" },
]

[[package]]
name = "primp"
version = "2.0.0"

[[package]]
name = "python-docx"
version = "1.2.0"
dependencies = [
    { name = "lxml" },
]

[[package]]
name = "lxml"
version = "6.1.2"
""",
        encoding="utf-8",
    )

    closure = runtime_pack._lock_closure(lock, ["python-docx"])

    assert closure == {"python-docx", "lxml"}
    assert runtime_pack._normal_name("Python_Docx") == "python-docx"


def test_a_changed_file_fails_verification(
    built: tuple[Path, dict], tmp_path: Path
) -> None:
    out, _ = built
    root = _extract(out / runtime_pack.ARCHIVE_NAME, tmp_path / "root")
    (root / "packages/office_worker/worker.py").write_text(
        "tampered\n", encoding="utf-8"
    )

    assert runtime_pack.verify(root) == [
        "hash mismatch: packages/office_worker/worker.py"
    ]
