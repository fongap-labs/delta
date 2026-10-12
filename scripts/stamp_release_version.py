"""Write the release version into the version fields of the release build checkout.

The version is the one entered when the release is started. It is the only source of
the released version: this script reads no version from these files and compares
nothing. It runs on the sandbox checkout of the release build and its result is never
committed.

    python scripts/stamp_release_version.py <X.Y.Z>
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

_STABLE_RE = re.compile(r"^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$")


def read_text(path: str) -> str:
    return (REPO / path).read_bytes().decode("utf-8")


def write_text(path: str, text: str) -> None:
    (REPO / path).write_bytes(text.encode("utf-8"))


def stamp_toml_table(path: str, table: str, version: str) -> None:
    """Set ``version`` inside the ``[table]`` section of a TOML file."""
    text = read_text(path)
    header = re.search(rf"^\[{re.escape(table)}\][ \t]*\r?$", text, re.MULTILINE)
    if header is None:
        raise SystemExit(f"{path}: no [{table}] section")
    rest = re.search(r"^\[", text[header.end():], re.MULTILINE)
    end = header.end() + rest.start() if rest else len(text)
    section, count = re.subn(
        r'^(version\s*=\s*")[^"]*(")',
        rf"\g<1>{version}\g<2>",
        text[header.end():end],
        count=1,
        flags=re.MULTILINE,
    )
    if count != 1:
        raise SystemExit(f"{path}: no version in [{table}]")
    write_text(path, text[: header.end()] + section + text[end:])


def stamp_lock_package(path: str, package: str, version: str) -> None:
    """Set the version of one ``[[package]]`` entry of a Cargo.lock file."""
    text = read_text(path)
    updated, count = re.subn(
        rf'(\[\[package\]\]\r?\nname = "{re.escape(package)}"\r?\nversion = ")[^"]*(")',
        rf"\g<1>{version}\g<2>",
        text,
    )
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one {package!r} package, found {count}")
    write_text(path, updated)


def stamp_json(path: str, version: str, is_package_lock: bool = False) -> None:
    """Set the top-level ``version`` of a JSON file (and the root entry of a lock file).

    Only the version lines are replaced; the result must equal the original document
    with just those values changed.
    """
    text = read_text(path)
    expected = json.loads(text)
    expected["version"] = version
    if is_package_lock:
        expected["packages"][""]["version"] = version

    updated = re.sub(
        r'^([ \t]*"version"[ \t]*:[ \t]*")[^"]*(")',
        rf"\g<1>{version}\g<2>",
        text,
        count=1,
        flags=re.MULTILINE,
    )
    if is_package_lock:
        updated = re.sub(
            r'("packages"\s*:\s*\{\s*""\s*:\s*\{[^{}]*?"version"\s*:\s*")[^"]*(")',
            rf"\g<1>{version}\g<2>",
            updated,
            count=1,
            flags=re.DOTALL,
        )
    if json.loads(updated) != expected:
        raise SystemExit(f"{path}: version fields could not be written in place")
    write_text(path, updated)


def main() -> None:
    if len(sys.argv) != 2 or not _STABLE_RE.fullmatch(sys.argv[1]):
        raise SystemExit("usage: stamp_release_version.py <X.Y.Z>")
    version = sys.argv[1]

    stamp_toml_table("pyproject.toml", "project", version)
    stamp_json("apps/desktop/package.json", version)
    stamp_json("apps/desktop/package-lock.json", version, is_package_lock=True)
    stamp_json("apps/desktop/src-tauri/tauri.conf.json", version)
    stamp_toml_table("apps/desktop/src-tauri/Cargo.toml", "package", version)
    stamp_lock_package("apps/desktop/src-tauri/Cargo.lock", "delta-desktop", version)
    stamp_toml_table("packaging/portable/launcher/Cargo.toml", "package", version)
    stamp_lock_package(
        "packaging/portable/launcher/Cargo.lock", "delta-portable-launcher", version
    )
    print(f"release version written: {version}")


if __name__ == "__main__":
    main()
