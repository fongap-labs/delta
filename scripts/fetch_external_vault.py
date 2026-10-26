"""Fetch pinned dependency archives from the external-vault GitHub Releases and check their SHA-256.

R and the R packages Delta statistics use are distributed as release assets of
fongap-labs/external-vault. The release tag is `<key>-v<version>` and the asset name is pinned next
to its SHA-256 in .github/external-vault-assets.json.

    python3 scripts/fetch_external_vault.py --list
    python3 scripts/fetch_external_vault.py r-base qcc
    python3 scripts/fetch_external_vault.py --dest ./vault

With no tool names every pinned archive is fetched. Each verified archive is kept in the destination
folder, which is printed on standard output. An archive already there with the pinned hash is not
downloaded again, so an archive saved by hand from the release page works offline. A mismatching file
is refused. Moving to a new release needs a new pin.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PINS = ROOT / ".github" / "external-vault-assets.json"
CHUNK = 1024 * 1024


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


def release_url(prefix: str, key: str, entry: dict[str, str]) -> str:
    return f"{prefix}{key}-v{entry['version']}/{entry['asset']}"


def fetch(prefix: str, key: str, entry: dict[str, str], dest: Path) -> None:
    name = entry["asset"]
    expected = entry["sha256"]
    if len(expected) != 64:
        raise SystemExit(f"{key} has no pinned SHA-256 in {PINS.name}")

    target = dest / name
    if target.is_file() and sha256_of(target) != expected:
        target.unlink()
    if not target.is_file():
        print(f"fetching {name}", file=sys.stderr)
        download(release_url(prefix, key, entry), target)
    actual = sha256_of(target)
    if actual != expected:
        target.unlink(missing_ok=True)
        raise SystemExit(f"{name} does not match its pinned SHA-256 (got {actual})")


def main() -> int:
    parser = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    parser.add_argument("tools", nargs="*", help="tool keys to fetch; default is every pinned tool")
    parser.add_argument("--list", action="store_true", help="print the pinned tools and their release URLs")
    parser.add_argument(
        "--dest",
        type=Path,
        default=Path(tempfile.gettempdir()) / "delta-external-vault",
        help="folder to keep the verified archives in",
    )
    args = parser.parse_args()

    pins = json.loads(PINS.read_text(encoding="utf-8"))
    prefix = pins["url_prefix"]
    tools = pins["tools"]

    if args.list:
        for key, entry in tools.items():
            print(f"{key}\t{entry['version']}\t{release_url(prefix, key, entry)}")
        return 0

    unknown = [key for key in args.tools if key not in tools]
    if unknown:
        raise SystemExit(f"no pin for {', '.join(unknown)}; pinned tools: {', '.join(tools)}")

    for key in args.tools or tools:
        fetch(prefix, key, tools[key], args.dest)
    print(args.dest)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
