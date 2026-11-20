"""Build the shared Python runtime pack and its release manifest (ADR-0085, distribution section).

Usage:
  python scripts/build_python_runtime.py --version <semver> --platform <target> \
      --interpreter-archive <path> --interpreter-sha256 <hex> --out-dir <dir>

The interpreter archive is the CPython 3.12 install_only archive for the platform, fetched by the
release workflow from its pinned address. The script checks it against the digest and never
downloads it. The output is exactly two files: the archive and manifest.json. The script prints the
archive name and SHA-256, which is what the published pack description pins.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT))

from packages.python_runtime import runtime_pack  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument(
        "--platform", required=True, choices=sorted(runtime_pack.PLATFORMS)
    )
    parser.add_argument("--interpreter-archive", type=Path, required=True)
    parser.add_argument("--interpreter-sha256", required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    args = parser.parse_args()
    try:
        release = runtime_pack.build(
            args.out_dir,
            interpreter_archive=args.interpreter_archive,
            interpreter_sha256=args.interpreter_sha256,
            platform=args.platform,
            version=args.version,
        )
    except ValueError as exc:
        print(f"build_python_runtime: {exc}", file=sys.stderr)
        return 2
    archive = release["archive"]
    print(f"archive: {archive['name']}")
    print(f"sha256: {archive['sha256']}")
    print(f"files: {len(release['files'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
