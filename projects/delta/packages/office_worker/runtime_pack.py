"""The manifest of a packaged Office runtime (ADR-0085).

A packaged runtime lists every file it ships with its SHA-256 in `manifest.json`. The Rust host
refuses an Office runtime whose files do not match that manifest, and refuses one that has no
manifest unless the development switch is set. The manifest is written by this module once the
runtime's files are in place:

    python -m packages.office_worker.runtime_pack --root <packaged runtime directory>
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

FORMAT = "delta.office-runtime"
PACK_VERSION = "0.1.0"
MANIFEST_NAME = "manifest.json"
_REPO_ROOT = Path(__file__).resolve().parents[2]
_EXCLUDED_DIRS = {"tests", "__pycache__"}


def source_files(root: Path = _REPO_ROOT) -> list[str]:
    """The files the Office worker runs from, relative to `root`, sorted and without tests."""
    names = ["packages/__init__.py"]
    for path in sorted((root / "packages" / "office_worker").rglob("*.py")):
        relative = path.relative_to(root)
        if _EXCLUDED_DIRS.intersection(relative.parts):
            continue
        names.append(relative.as_posix())
    return sorted(set(names))


def manifest(root: Path = _REPO_ROOT) -> dict[str, Any]:
    """The manifest for the files under `root`. Two calls on the same files give the same result."""
    files = []
    for name in source_files(root):
        data = (root / name).read_bytes()
        files.append(
            {
                "path": name,
                "sha256": hashlib.sha256(data).hexdigest(),
                "size": len(data),
            }
        )
    return {
        "format": FORMAT,
        "version": 1,
        "pack": "office",
        "pack_version": PACK_VERSION,
        "files": files,
    }


def write(root: Path) -> dict[str, Any]:
    """Write `manifest.json` into `root` and return it."""
    document = manifest(root)
    (root / MANIFEST_NAME).write_text(
        json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return document


def verify(root: Path) -> list[str]:
    """The problems that stop the runtime under `root` from being used. An empty list means it matches."""
    manifest_path = root / MANIFEST_NAME
    if not manifest_path.exists():
        return [f"{MANIFEST_NAME} is missing"]
    try:
        document = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return [f"{MANIFEST_NAME} is unreadable"]
    if document.get("format") != FORMAT or document.get("version") != 1:
        return ["manifest format or version is not supported"]
    problems = []
    for entry in document.get("files", []):
        relative = entry["path"]
        if Path(relative).is_absolute() or ".." in Path(relative).parts:
            problems.append(f"path escapes the pack: {relative}")
            continue
        target = root / relative
        if not target.is_file():
            problems.append(f"missing file: {relative}")
        elif hashlib.sha256(target.read_bytes()).hexdigest() != entry["sha256"]:
            problems.append(f"hash mismatch: {relative}")
    return problems


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--root", type=Path, required=True, help="the packaged runtime directory"
    )
    arguments = parser.parse_args(argv)
    document = write(arguments.root)
    print(f"wrote {MANIFEST_NAME}: {len(document['files'])} files")
    return 0


if __name__ == "__main__":
    sys.exit(main())
