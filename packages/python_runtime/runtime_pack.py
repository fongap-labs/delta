"""Build and verify the shared Python runtime pack (ADR-0085, distribution section).

The pack is a relocatable CPython tree, the locked Office Worker libraries installed into its
site-packages for one target platform, the Office and Explore worker packages, and a manifest that
lists every file with its SHA-256. The interpreter archive is supplied by the caller and checked
against its digest; this module never downloads it.

The archive is deterministic: the same inputs always yield the same bytes, so its digest can be
pinned in the application's published pack description (runtime/packs.json). The application checks
the extracted files against the manifest inside the archive (`runtime_pack::pack_problems`).
"""

from __future__ import annotations

import gzip
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import Any

MANIFEST_NAME = "manifest.json"
ARCHIVE_NAME = "delta-python-runtime.tar.gz"
PACK_NAME = "delta-python"
FORMAT = "delta.python-runtime"
PACK_VERSION = "0.1.0"
PYTHON_VERSION = "3.12"

# Per target platform: the interpreter's path inside the pack and the directory its libraries go to.
PLATFORMS: dict[str, tuple[str, str]] = {
    "x86_64-pc-windows-msvc": ("python/python.exe", "python/Lib/site-packages"),
    "x86_64-unknown-linux-gnu": (
        "python/bin/python3",
        "python/lib/python3.12/site-packages",
    ),
}

_REPO_ROOT = Path(__file__).resolve().parents[2]
_WORKER_PACKAGES = ("explore_pack", "office_worker")
_EXCLUDED_DIRS = {"tests", "__pycache__"}

Installer = Callable[[str, Path, str], None]


def sha256_of(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _file_sha256(path: Path) -> str:
    return sha256_of(path.read_bytes())


_OFFICE_ROOTS = ("python-docx", "python-pptx", "openpyxl")


def _normal_name(name: str) -> str:
    return re.sub(r"[-_.]+", "-", name).lower()


def _lock_closure(lock_path: Path, roots: Sequence[str]) -> set[str]:
    """The normalized names of `roots` and everything they depend on, as `uv.lock` records it."""
    packages = tomllib.loads(lock_path.read_text(encoding="utf-8"))["package"]
    dependencies: dict[str, list[str]] = {}
    for package in packages:
        name = _normal_name(package["name"])
        dependencies.setdefault(name, []).extend(
            _normal_name(dependency["name"])
            for dependency in package.get("dependencies", [])
        )
    closure: set[str] = set()
    pending = [_normal_name(root) for root in roots]
    while pending:
        name = pending.pop()
        if name not in closure:
            closure.add(name)
            pending.extend(dependencies.get(name, []))
    return closure


def office_requirements(repo_root: Path = _REPO_ROOT) -> str:
    """The locked requirements of the Office libraries and what they need, as `uv export` writes them.

    The export also lists the project's own dependencies, which the workers do not use, so only the
    lines for the closure of the Office libraries are kept. Environment markers are left to uv, which
    evaluates them for the target platform when it installs.
    """
    result = subprocess.run(
        [
            "uv",
            "export",
            "--frozen",
            "--no-dev",
            "--extra",
            "office",
            "--no-emit-project",
            "--no-hashes",
            "--no-annotate",
        ],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=True,
    )
    closure = _lock_closure(repo_root / "uv.lock", _OFFICE_ROOTS)
    kept = []
    for line in result.stdout.splitlines():
        match = re.match(r"^([A-Za-z0-9][A-Za-z0-9._-]*)==", line)
        if match and _normal_name(match.group(1)) in closure:
            kept.append(line)
    return "\n".join(kept) + "\n"


def install_with_uv(requirements: str, site_packages: Path, platform: str) -> None:
    """Install the requirements into `site_packages` as wheels for `platform`, without resolving."""
    with tempfile.TemporaryDirectory() as directory:
        listing = Path(directory) / "requirements.txt"
        listing.write_text(requirements, encoding="utf-8")
        subprocess.run(
            [
                "uv",
                "pip",
                "install",
                "--no-deps",
                "--python-platform",
                platform,
                "--python-version",
                PYTHON_VERSION,
                "--target",
                str(site_packages),
                "--requirement",
                str(listing),
            ],
            check=True,
        )


def _entries(root: Path) -> list[str]:
    """Every entry under `root` (files, symbolic links and directories), relative and sorted."""
    entries: list[str] = []
    for directory, subdirectories, filenames in os.walk(root, followlinks=False):
        for name in subdirectories + filenames:
            entries.append(Path(directory, name).relative_to(root).as_posix())
    return sorted(entries)


def _is_directory(root: Path, relative: str) -> bool:
    path = root / relative
    return path.is_dir() and not path.is_symlink()


def _describe(root: Path) -> list[dict[str, Any]]:
    """The manifest's file list: every regular file and link target, with its digest and size."""
    files: list[dict[str, Any]] = []
    for relative in _entries(root):
        if relative == MANIFEST_NAME or _is_directory(root, relative):
            continue
        path = root / relative
        resolved = path.resolve()
        if not resolved.is_relative_to(root.resolve()):
            raise ValueError(f"a link in the pack points outside it: {relative}")
        data = resolved.read_bytes()
        files.append({"path": relative, "sha256": sha256_of(data), "size": len(data)})
    return files


def _tar_bytes(root: Path, entries: Sequence[str]) -> bytes:
    """A reproducible gzip'ed tar: fixed mtimes, owners and modes; entries in the given order."""
    raw = io.BytesIO()
    with gzip.GzipFile(fileobj=raw, mode="wb", mtime=0) as gz:
        with tarfile.open(fileobj=gz, mode="w", format=tarfile.GNU_FORMAT) as archive:
            for relative in entries:
                path = root / relative
                info = tarfile.TarInfo(relative)
                info.mtime = 0
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                if path.is_symlink():
                    info.type = tarfile.SYMTYPE
                    info.linkname = os.readlink(path)
                    info.mode = 0o777
                    archive.addfile(info)
                elif path.is_dir():
                    info.type = tarfile.DIRTYPE
                    info.mode = 0o755
                    archive.addfile(info)
                else:
                    data = path.read_bytes()
                    info.size = len(data)
                    info.mode = 0o755 if os.stat(path).st_mode & 0o111 else 0o644
                    archive.addfile(info, io.BytesIO(data))
    return raw.getvalue()


def _copy_workers(repo_root: Path, staging: Path) -> None:
    (staging / "packages").mkdir(parents=True)
    shutil.copyfile(
        repo_root / "packages" / "__init__.py", staging / "packages" / "__init__.py"
    )
    for worker in _WORKER_PACKAGES:
        for source in sorted((repo_root / "packages" / worker).rglob("*.py")):
            relative = source.relative_to(repo_root)
            if any(part in _EXCLUDED_DIRS for part in relative.parts):
                continue
            target = staging / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)


def build(
    out_dir: Path,
    interpreter_archive: Path,
    interpreter_sha256: str,
    platform: str,
    version: str = PACK_VERSION,
    repo_root: Path = _REPO_ROOT,
    requirements: str | None = None,
    install: Installer = install_with_uv,
) -> dict[str, Any]:
    """Write the archive and its release manifest into `out_dir`; return the release manifest.

    The interpreter archive must match `interpreter_sha256`, and the version must equal the pack's
    `PACK_VERSION`. `requirements` defaults to the locked Office extra; `install` puts them into the
    pack and is replaceable in tests.
    """
    if version != PACK_VERSION:
        raise ValueError(
            f"version {version} does not match the pack's PACK_VERSION {PACK_VERSION}; "
            "update packages/python_runtime/runtime_pack.py first"
        )
    if platform not in PLATFORMS:
        raise ValueError(f"unsupported platform: {platform}")
    interpreter_path, site_packages = PLATFORMS[platform]
    if _file_sha256(interpreter_archive) != interpreter_sha256:
        raise ValueError("the interpreter archive does not match its pinned SHA-256")
    libraries = office_requirements(repo_root) if requirements is None else requirements

    with tempfile.TemporaryDirectory() as work:
        staging = Path(work) / "pack"
        staging.mkdir()
        with tarfile.open(interpreter_archive, "r:gz") as archive:
            archive.extractall(staging, filter="data")
        if not (staging / interpreter_path).exists():
            raise ValueError(f"the interpreter archive has no {interpreter_path}")
        install(libraries, staging / site_packages, platform)
        _copy_workers(repo_root, staging)

        internal = {
            "format": FORMAT,
            "version": 1,
            "pack": PACK_NAME,
            "pack_version": version,
            "python": {"version": PYTHON_VERSION, "interpreter": interpreter_path},
            "files": _describe(staging),
        }
        (staging / MANIFEST_NAME).write_text(
            json.dumps(internal, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
        archive_bytes = _tar_bytes(staging, _entries(staging))

    release = dict(internal)
    release["archive"] = {
        "name": ARCHIVE_NAME,
        "sha256": sha256_of(archive_bytes),
        "size": len(archive_bytes),
    }
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / ARCHIVE_NAME).write_bytes(archive_bytes)
    (out_dir / MANIFEST_NAME).write_text(
        json.dumps(release, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    return release


def verify(root: Path) -> list[str]:
    """Check an extracted pack against the manifest inside it; return the problems found.

    This mirrors `runtime_pack::pack_problems`: the format and version must match, every listed
    path must stay inside the pack, and every listed file must exist with its recorded SHA-256.
    """
    manifest_path = root / MANIFEST_NAME
    if not manifest_path.is_file():
        return [f"{MANIFEST_NAME} is missing"]
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        return [f"{MANIFEST_NAME} is unreadable: {exc}"]
    if manifest.get("format") != FORMAT or manifest.get("version") != 1:
        return ["manifest format or version is not supported"]

    problems: list[str] = []
    for entry in manifest.get("files", []):
        relative = entry.get("path", "")
        if Path(relative).is_absolute() or ".." in Path(relative).parts:
            problems.append(f"path escapes the pack: {relative}")
            continue
        path = root / relative
        if not path.is_file():
            problems.append(f"missing file: {relative}")
        elif _file_sha256(path) != entry.get("sha256"):
            problems.append(f"hash mismatch: {relative}")
    return problems
