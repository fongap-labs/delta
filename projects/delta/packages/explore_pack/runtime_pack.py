"""Build and verify the Explore Runtime Pack (ADR-0080, distribution section).

The pack is the Explore Pack source (standard library only, so there are no third-party packages to
lock) plus a manifest that lists every file with its SHA-256. The archive keeps the repository layout
(`packages/__init__.py`, `packages/explore_pack/*.py`), so an extracted pack can be used as
`DELTA_EXPLORE_ROOT` directly. The archive is deterministic: the same source always yields the same
bytes, so its digest can be pinned in a published tool descriptor.

The Python interpreter is not part of the pack. The host checks that a suitable interpreter exists.
"""

from __future__ import annotations

import gzip
import hashlib
import io
import json
import tarfile
from collections.abc import Sequence
from pathlib import Path
from typing import Any

from packages.explore_pack.result import PACK_VERSION

MANIFEST_NAME = "manifest.json"
ARCHIVE_NAME = "explore-runtime.tar.gz"
PYTHON_REQUIRES = ">=3.11"
_REPO_ROOT = Path(__file__).resolve().parents[2]
_EXCLUDED_DIRS = {"tests", "__pycache__"}


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def source_files(repo_root: Path = _REPO_ROOT) -> list[str]:
    """Paths relative to the repository root: the package init and the pack source, sorted."""
    files = ["packages/__init__.py"]
    pack = repo_root / "packages" / "explore_pack"
    for path in sorted(pack.rglob("*.py")):
        relative = path.relative_to(repo_root)
        if any(part in _EXCLUDED_DIRS for part in relative.parts):
            continue
        files.append(relative.as_posix())
    return sorted(files)


def _tar_bytes(repo_root: Path, names: Sequence[str]) -> bytes:
    """A reproducible gzip'ed tar: fixed mtimes, owners and modes; entries in the given order."""
    raw = io.BytesIO()
    with gzip.GzipFile(fileobj=raw, mode="wb", mtime=0) as gz:
        with tarfile.open(fileobj=gz, mode="w", format=tarfile.USTAR_FORMAT) as archive:
            for name in names:
                data = (repo_root / name).read_bytes()
                info = tarfile.TarInfo(name)
                info.size = len(data)
                info.mtime = 0
                info.mode = 0o644
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                archive.addfile(info, io.BytesIO(data))
    return raw.getvalue()


def build(
    out_dir: Path, version: str = PACK_VERSION, repo_root: Path = _REPO_ROOT
) -> dict[str, Any]:
    """Write the archive and its manifest into `out_dir`; return the manifest.

    The version is checked against the pack's own `PACK_VERSION`, which is what the results it
    writes report. The asset names carry no version: the release manifest names its assets once,
    and the version comes from the release start.
    """
    if version != PACK_VERSION:
        raise ValueError(
            f"version {version} does not match the pack's PACK_VERSION {PACK_VERSION}; "
            "update packages/explore_pack/result.py first"
        )
    names = source_files(repo_root)
    files = []
    for name in names:
        data = (repo_root / name).read_bytes()
        files.append({"path": name, "sha256": _sha256(data), "size": len(data)})
    archive = _tar_bytes(repo_root, names)
    archive_name = ARCHIVE_NAME
    manifest: dict[str, Any] = {
        "format": "delta.explore-runtime",
        "version": 1,
        "pack": "explore",
        "pack_version": version,
        "python_requires": PYTHON_REQUIRES,
        "license": "Apache-2.0",
        "archive": {
            "name": archive_name,
            "sha256": _sha256(archive),
            "size": len(archive),
        },
        "files": files,
    }
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / archive_name).write_bytes(archive)
    (out_dir / MANIFEST_NAME).write_text(
        json.dumps(manifest, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    return manifest


def verify(root: Path) -> list[str]:
    """Check an extracted pack directory against its manifest; return the problems found.

    An empty list means `manifest.json` is present and every file it lists exists with the
    recorded SHA-256. Paths that escape the pack root are refused.
    """
    manifest_path = root / MANIFEST_NAME
    if not manifest_path.is_file():
        return [f"{MANIFEST_NAME} is missing"]
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        return [f"{MANIFEST_NAME} is unreadable: {exc}"]
    if (
        manifest.get("format") != "delta.explore-runtime"
        or manifest.get("version") != 1
    ):
        return ["manifest format or version is not supported"]

    base = root.resolve()
    problems: list[str] = []
    for entry in manifest.get("files", []):
        relative = str(entry.get("path", ""))
        path = (base / relative).resolve()
        if base not in path.parents:
            problems.append(f"path escapes the pack: {relative}")
            continue
        if not path.is_file():
            problems.append(f"missing file: {relative}")
            continue
        if _sha256(path.read_bytes()) != entry.get("sha256"):
            problems.append(f"hash mismatch: {relative}")
    return problems
