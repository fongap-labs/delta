"""Retired-path consistency gate.

Blocks the repository from re-referencing migrated or retired file paths.

Current code, workflows, configuration, tests, and valid docs must use the
current canonical paths. Historical paths are allowed only in CHANGELOG.md;
other historical context is traced via Git and Pull Requests.

Scope:
  - Scans all Git tracked files (git ls-files).
  - Ignores build output, dependency directories, and other non-source trees.
  - Uses precise file-level rules for retired paths.
  - Invalid top-level directories remain checked by the CI layout-check.

Run:

    python scripts/check_retired_paths.py

Exits with code 1 when a retired path is found.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent


# Retired path -> current canonical path.
#
# [\\/] matches both Windows and POSIX path separators.
# Rules must stay precise; avoid broad patterns such as a bare directory
# name, which would hit currently valid paths like apps/desktop/src.
FORBIDDEN: dict[str, str] = {
    # --- retired Python server packaging ---
    r"packaging[/\\]delta-server-version\.txt": (
        "packaging/server/delta-server-version.txt"
    ),
    r"packaging[/\\]delta-server\.spec": "packaging/server/delta-server.spec",
    r"packaging[/\\]server_entry\.py": "packaging/server/server_entry.py",
    r"packaging[/\\]build_portable\.ps1": (
        "packaging/portable/build_portable.ps1"
    ),
    r"packaging[/\\]scan_portable_paths\.ps1": (
        "packaging/portable/scan_portable_paths.ps1"
    ),
    # --- retired dual-authority files ---
    # These were hard-cut to a single private authority (model-authority.json).
    # A reference here means a retired path crept back as a production target.
    r"crates[/\\]delta-core[/\\]src[/\\]prefs\.json": "model-authority.json",
    r"crates[/\\]delta-core[/\\]src[/\\]secrets\.json": "model-authority.json",
    # --- retired connector authority files ---
    r"crates[/\\]delta-core[/\\]src[/\\]application-state\.json": "application.json",
    r"crates[/\\]delta-core[/\\]src[/\\]connector-secrets\.json": "application.json",
}


# CHANGELOG.md is the only current file allowed to keep historical paths.
#
# governance, architecture, operations, UPSTREAM, etc. are all current
# valid documents and must not reference retired paths again.
HISTORY_EXEMPT = ("CHANGELOG.md",)


# Build output and dependency directories that do not need scanning.
_SKIP_PARTS = {
    ".git",
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
    "dist",
    "releases",
}


def _tracked_files() -> list[Path]:
    """Returns all Git tracked files in the repository."""
    out = subprocess.run(
        ["git", "ls-files"],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=True,
    ).stdout

    return [REPO / line for line in out.splitlines() if line]


def _is_exempt(path: Path) -> bool:
    """Returns whether the file is exempt from the retired-path history rule."""
    rel = path.relative_to(REPO).as_posix()
    return rel in HISTORY_EXEMPT


def violations_for(text: str, rel_path: str) -> list[str]:
    """Returns retired path references found in a single file."""
    return [
        (
            f"{rel_path}:{text.count(chr(10), 0, match.start()) + 1}: "
            f"deprecated path -> use {canonical}"
        )
        for pattern, canonical in FORBIDDEN.items()
        for match in re.finditer(pattern, text)
    ]


def find_violations() -> list[str]:
    """Scans the repository and returns all retired path references."""
    violations: list[str] = []

    for path in _tracked_files():
        if _is_exempt(path) or _SKIP_PARTS & set(path.parts):
            continue

        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue

        violations.extend(
            violations_for(
                text,
                path.relative_to(REPO).as_posix(),
            )
        )

    return violations


def main() -> int:
    violations = find_violations()

    if violations:
        print(
            "retired-path gate FAILED — "
            "deprecated paths referenced outside CHANGELOG.md:"
        )

        for violation in violations:
            print(f"  {violation}")

        return 1

    print("retired-path gate clean: no deprecated path references")
    return 0


if __name__ == "__main__":
    sys.exit(main())