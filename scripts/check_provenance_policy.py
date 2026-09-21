from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]

EXTERNAL_IDENTITY = re.compile(
    r"openworker|opencoworker|api\.openworker\.com|andrewyng/openworker",
    re.IGNORECASE,
)

SOURCE_ROOTS = (
    "apps",
    "core",
    "integrations",
    "packages",
    "packaging",
    "resources",
    "services",
)

ACTIVE_ROOT_DOCS = (
    "README.md",
    "CONTRIBUTING.md",
    "SECURITY.md",
)

LEGAL_AND_HISTORY = {
    pathlib.Path("LICENSE"),
    pathlib.Path("NOTICE"),
    pathlib.Path("PROVENANCE.md"),
    pathlib.Path("THIRD_PARTY_NOTICES.md"),
    pathlib.Path("CHANGELOG.md"),
    pathlib.Path("docs/audits/phase-1-provenance-license-audit.md"),
    pathlib.Path("docs/audits/phase-2.5-foundation-cleanup-audit.md"),
}

TEXT_SUFFIXES = {
    ".md",
    ".rs",
    ".py",
    ".ts",
    ".tsx",
    ".js",
    ".mjs",
    ".json",
    ".toml",
    ".yaml",
    ".yml",
    ".sh",
    ".ps1",
}


def read_text(path: pathlib.Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return ""


def active_files() -> list[pathlib.Path]:
    files: list[pathlib.Path] = []
    for name in ACTIVE_ROOT_DOCS:
        path = ROOT / name
        if path.exists():
            files.append(path)

    for root_name in SOURCE_ROOTS:
        root = ROOT / root_name
        if not root.exists():
            continue
        for path in root.rglob("*"):
            if not path.is_file():
                continue
            if any(part in {"node_modules", "target", ".venv", "dist", "build"} for part in path.parts):
                continue
            if path.suffix.lower() in TEXT_SUFFIXES or path.name in {"Cargo.toml", "package.json", "pyproject.toml"}:
                files.append(path)
    return files


def main() -> int:
    errors: list[str] = []

    license_text = read_text(ROOT / "LICENSE")
    notice_text = read_text(ROOT / "NOTICE")
    notices_text = read_text(ROOT / "THIRD_PARTY_NOTICES.md")
    provenance_text = read_text(ROOT / "PROVENANCE.md")

    if "Apache License" not in license_text or "Version 2.0" not in license_text:
        errors.append("LICENSE must be the Delta Foundation Apache-2.0 license")
    if "Copyright 2026 Fongap Labs" not in notice_text:
        errors.append("NOTICE must contain the current Delta copyright attribution")
    if "THIRD_PARTY_NOTICES.md" not in provenance_text or "LICENSES/" not in provenance_text:
        errors.append("PROVENANCE.md must identify the attribution register and full-license directory")

    # Phase 2.6: retired third-party attribution must not reappear.
    retired_license = ROOT / "LICENSES" / "OpenWorker-MIT.txt"
    if retired_license.exists():
        errors.append("LICENSES/OpenWorker-MIT.txt must not be reintroduced (A/B = 0)")
    if "Copyright (c) 2024 Andrew Ng" in notices_text:
        errors.append("THIRD_PARTY_NOTICES.md must not retain retired third-party copyright notice")

    for path in active_files():
        rel = path.relative_to(ROOT)
        if rel in LEGAL_AND_HISTORY or path.resolve() == pathlib.Path(__file__).resolve():
            continue
        if EXTERNAL_IDENTITY.search(rel.as_posix()):
            errors.append(f"{rel}: external source-project identity in active path")
            continue
        text = read_text(path)
        if EXTERNAL_IDENTITY.search(text):
            errors.append(f"{rel}: external source-project identity in active product/source content")

    if errors:
        print("Delta provenance policy violations:")
        for error in sorted(set(errors)):
            print(f"  - {error}")
        return 1

    print("Delta provenance policy: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
