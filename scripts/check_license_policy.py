from __future__ import annotations

import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    target = ROOT / path
    if not target.is_file():
        raise FileNotFoundError(path)
    return target.read_text(encoding="utf-8")


def main() -> int:
    errors: list[str] = []

    try:
        license_text = read("LICENSE")
        notice_text = read("NOTICE")
        notices_text = read("THIRD_PARTY_NOTICES.md")
        provenance_text = read("PROVENANCE.md")
    except FileNotFoundError as exc:
        print(f"Delta license policy violation: missing {exc.args[0]}")
        return 1

    if "Apache License" not in license_text or "Version 2.0" not in license_text:
        errors.append("root LICENSE must be Apache License 2.0")
    if "THIRD-PARTY COMPONENTS" in license_text:
        errors.append("root LICENSE must contain only the standard Apache-2.0 license text")
    if "Copyright (c) 2024 Andrew Ng" in license_text:
        errors.append("retired third-party copyright must not be presented as Delta root-license ownership")

    if "Copyright 2026 Fongap Labs" not in notice_text:
        errors.append("NOTICE must contain the Delta copyright attribution")
    if "THIRD_PARTY_NOTICES.md" not in notice_text or "LICENSES/" not in notice_text:
        errors.append("NOTICE must direct distributions to third-party notices and license texts")

    required_notices = {
        "Copyright (c) 2023 LobeHub": "Lobe Icons attribution",
        "Copyright 2018 The Manrope Project Authors": "Manrope attribution",
        "LICENSES/LobeIcons-MIT.txt": "Lobe Icons license link",
        "LICENSES/Manrope-OFL-1.1.txt": "Manrope license link",
    }
    for needle, label in required_notices.items():
        if needle not in notices_text:
            errors.append(f"THIRD_PARTY_NOTICES.md missing {label}")

    third_party_licenses = {
        "LICENSES/LobeIcons-MIT.txt": ("MIT License", "Copyright (c) 2023 LobeHub"),
        "LICENSES/Manrope-OFL-1.1.txt": ("SIL OPEN FONT LICENSE Version 1.1", "Copyright 2018 The Manrope Project Authors"),
    }
    for path, needles in third_party_licenses.items():
        try:
            text = read(path)
        except FileNotFoundError:
            errors.append(f"missing third-party license text: {path}")
            continue
        for needle in needles:
            if needle not in text:
                errors.append(f"{path} missing required upstream notice: {needle}")

    manifest_expectations = {
        "pyproject.toml": 'license = "Apache-2.0"',
        "crates/delta-core/Cargo.toml": 'license = "Apache-2.0"',
        "apps/desktop/src-tauri/Cargo.toml": 'license = "Apache-2.0"',
        "crates/delta-stt/Cargo.toml": 'license = "Apache-2.0"',
        "packaging/portable/launcher/Cargo.toml": 'license = "Apache-2.0"',
    }
    for path, expected in manifest_expectations.items():
        try:
            text = read(path)
        except FileNotFoundError:
            errors.append(f"missing Delta manifest: {path}")
            continue
        if expected not in text:
            errors.append(f"{path} must declare Apache-2.0")

    try:
        package = json.loads(read("apps/desktop/package.json"))
    except (FileNotFoundError, json.JSONDecodeError):
        errors.append("apps/desktop/package.json must be readable JSON")
    else:
        if package.get("license") != "Apache-2.0":
            errors.append("apps/desktop/package.json must declare Apache-2.0")

    deny_text = read("deny.toml")
    if '"Apache-2.0"' not in deny_text or '"MIT"' not in deny_text:
        errors.append("deny.toml must continue allowing Apache-2.0 and MIT dependency licenses")

    if "THIRD_PARTY_NOTICES.md" not in provenance_text or "LICENSES/" not in provenance_text:
        errors.append("PROVENANCE.md must define the attribution and full-license locations")

    if errors:
        print("Delta license policy violations:")
        for error in sorted(set(errors)):
            print(f"  - {error}")
        return 1

    print("Delta Foundation license policy: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
