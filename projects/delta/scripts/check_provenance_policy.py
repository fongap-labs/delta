from __future__ import annotations

import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]


def read_text(path: pathlib.Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return ""


def main() -> int:
    errors: list[str] = []

    license_text = read_text(ROOT / "LICENSE")
    notice_text = read_text(ROOT / "NOTICE")
    provenance_text = read_text(ROOT / "PROVENANCE.md")

    if "Apache License" not in license_text or "Version 2.0" not in license_text:
        errors.append("LICENSE must be the Delta Foundation Apache-2.0 license")
    if "Copyright 2026 Fongap Labs" not in notice_text:
        errors.append("NOTICE must contain the current Delta copyright attribution")
    if "THIRD_PARTY_NOTICES.md" not in provenance_text or "LICENSES/" not in provenance_text:
        errors.append("PROVENANCE.md must identify the attribution register and full-license directory")

    if errors:
        print("Delta provenance policy violations:")
        for error in sorted(set(errors)):
            print(f"  - {error}")
        return 1

    print("Delta provenance policy: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
