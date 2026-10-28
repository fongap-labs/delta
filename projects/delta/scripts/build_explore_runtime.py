"""Build the Explore Runtime Pack and its manifest (ADR-0080, distribution section).

Usage: python scripts/build_explore_runtime.py --version <semver> --out-dir <dir>

The version must equal the pack's PACK_VERSION. The output is deterministic, and it is exactly two
files: the archive and manifest.json. The script prints the archive name and SHA-256, which is what a
published tool descriptor pins.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT))

from packages.explore_pack import runtime_pack  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    args = parser.parse_args()
    try:
        manifest = runtime_pack.build(args.out_dir, version=args.version)
    except ValueError as exc:
        print(f"build_explore_runtime: {exc}", file=sys.stderr)
        return 2
    archive = manifest["archive"]
    print(f"archive: {archive['name']}")
    print(f"sha256: {archive['sha256']}")
    print(f"files: {len(manifest['files'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
