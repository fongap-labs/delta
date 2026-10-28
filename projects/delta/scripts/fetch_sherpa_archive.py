"""Fetch the sherpa-onnx native archive for this platform and check it against a pinned SHA-256.

The sherpa-onnx-sys build script downloads a prebuilt native library at build time and links it into
the Delta binary, without checking it. This script does the download first, refuses an archive whose
hash is not the pinned one, and prints the folder to give the build script as SHERPA_ONNX_ARCHIVE_DIR.

    export SHERPA_ONNX_ARCHIVE_DIR="$(python3 scripts/fetch_sherpa_archive.py)"

The pins live in .github/sherpa-archives.json. Moving sherpa-onnx to a new version needs new pins; an
archive without a pin is refused, so the build stops instead of downloading something unchecked.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import sys
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PINS = ROOT / ".github" / "sherpa-archives.json"
CHUNK = 1024 * 1024


def archive_for(system: str, machine: str, version: str) -> str:
    """Name of the static archive sherpa-onnx-sys asks for on this platform."""
    machine = machine.lower()
    if system == "Linux" and machine in {"x86_64", "amd64"}:
        return f"sherpa-onnx-v{version}-linux-x64-static-lib.tar.bz2"
    if system == "Windows" and machine in {"amd64", "x86_64"}:
        return f"sherpa-onnx-v{version}-win-x64-static-MT-Release-lib.tar.bz2"
    raise SystemExit(f"no pinned sherpa-onnx archive for {system} {machine}")


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(CHUNK):
            digest.update(chunk)
    return digest.hexdigest()


def download(url: str, target: Path) -> None:
    """Write `url` to `target` through a partial file, so a cut transfer never looks complete."""
    target.parent.mkdir(parents=True, exist_ok=True)
    handle, partial_name = tempfile.mkstemp(dir=target.parent, suffix=".partial")
    partial = Path(partial_name)
    try:
        with os.fdopen(handle, "wb") as out, urllib.request.urlopen(url, timeout=120) as response:
            while chunk := response.read(CHUNK):
                out.write(chunk)
        os.replace(partial, target)
    finally:
        partial.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    parser.add_argument(
        "--dest",
        type=Path,
        default=Path(tempfile.gettempdir()) / "delta-sherpa-archives",
        help="folder to keep the verified archive in",
    )
    args = parser.parse_args()

    pins = json.loads(PINS.read_text(encoding="utf-8"))
    name = archive_for(platform.system(), platform.machine(), pins["version"])
    expected = pins["archives"].get(name)
    if not expected:
        raise SystemExit(f"{name} has no pinned SHA-256 in {PINS.name}")

    target = args.dest / name
    if target.is_file() and sha256_of(target) != expected:
        target.unlink()
    if not target.is_file():
        print(f"fetching {name}", file=sys.stderr)
        download(pins["url_prefix"] + name, target)
    actual = sha256_of(target)
    if actual != expected:
        target.unlink(missing_ok=True)
        raise SystemExit(f"{name} does not match its pinned SHA-256 (got {actual})")
    print(args.dest)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
