"""Release layout gate.

The release build is a set of PowerShell scripts that run on a Windows runner and name files by path.
Nothing in CI runs them, so a move of the product tree can leave a path behind and the break only shows
when a release is cut. This gate reads those scripts and the release manifest and checks, on every
change, that the files they name exist where the scripts will look for them.

It checks:
  - every build script, the version files and the root named by release.manifest.json;
  - every path the release and packaging scripts join to the product root (Join-Path $RepoRoot "x"),
    or name relative to it after Set-Location $repoRoot (& ".\\x.ps1", Push-Location "x", python scripts/x.py);
  - every file stamp_release_version.py writes the release version into.

Files the build creates (releases, node_modules, target, ...) are not required to exist.

Run from the product root:

    python scripts/check_release_layout.py
    python scripts/check_release_layout.py --self-test

Exits with code 1 when a named path is missing.
"""

from __future__ import annotations

import json
import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY_ROOT = ROOT.parent.parent

# The scripts that build, package and sign a release.
RELEASE_SCRIPT_PATTERNS = (
    "scripts/release-build*.ps1",
    "scripts/build-*-runtime.ps1",
    "packaging/portable/*.ps1",
)
# What a build creates or installs; these do not exist in a clean checkout.
GENERATED = {"releases", "node_modules", "target", "dist", "build", ".venv", "stage"}

ROOT_JOIN = re.compile(r'Join-Path\s+\$(?:RepoRoot|repoRoot)\s+"([^"$`]+)"')
SET_LOCATION = re.compile(r"Set-Location\s+\$(?:RepoRoot|repoRoot)\b")
RELATIVE_INVOKE = re.compile(r'^\s*&\s+"(\.[\\/][^"$`]+)"', re.MULTILINE)
RELATIVE_PUSH = re.compile(r'Push-Location\s+"([^"$`]+)"')
RELATIVE_PYTHON = re.compile(r"^\s*python3?\s+(scripts/[\w./-]+\.py)", re.MULTILINE)
STAMP_TARGET = re.compile(r'stamp_\w+\(\s*"([^"]+)"')


def code_of(text: str) -> str:
    """The script without its comment lines."""
    return "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("#"))


def is_generated(relative: str) -> bool:
    first = re.split(r"[\\/]", relative.lstrip(".\\/"), maxsplit=1)[0]
    return first in GENERATED


def exists(product_root: Path, relative: str) -> bool:
    return (product_root / relative.replace("\\", "/")).exists()


def check_script(product_root: Path, script: Path) -> list[str]:
    code = code_of(script.read_text(encoding="utf-8-sig"))
    name = script.relative_to(product_root).as_posix()
    named: list[str] = list(ROOT_JOIN.findall(code))
    if SET_LOCATION.search(code):
        for pattern in (RELATIVE_INVOKE, RELATIVE_PUSH, RELATIVE_PYTHON):
            named.extend(pattern.findall(code))
    return [
        f"{name}: names '{relative}', which is not in the product tree ({product_root.as_posix()})"
        for relative in named
        if not is_generated(relative) and not exists(product_root, relative)
    ]


def check_manifest(product_root: Path, repository_root: Path) -> list[str]:
    path = product_root / "release.manifest.json"
    if not path.is_file():
        return [f"release.manifest.json: missing from {product_root.as_posix()}"]
    manifest = json.loads(path.read_text(encoding="utf-8-sig"))
    problems: list[str] = []
    root = manifest.get("root", "")
    if root and (repository_root / root).resolve() != product_root.resolve():
        problems.append(f"release.manifest.json: root '{root}' is not this product tree")
    for build in manifest.get("builds", []):
        script = build.get("script", "")
        if not exists(product_root, script):
            problems.append(f"release.manifest.json: build '{build.get('id')}' script '{script}' is missing")
    for key in ("node_version_file", "python_version_file"):
        value = manifest.get(key, "")
        if value and not exists(product_root, value):
            problems.append(f"release.manifest.json: {key} '{value}' is missing")
    return problems


def check_stamp_targets(product_root: Path) -> list[str]:
    path = product_root / "scripts" / "stamp_release_version.py"
    if not path.is_file():
        return []
    return [
        f"scripts/stamp_release_version.py: writes the version into '{target}', which is missing"
        for target in STAMP_TARGET.findall(path.read_text(encoding="utf-8"))
        if not exists(product_root, target)
    ]


def find_problems(product_root: Path, repository_root: Path) -> list[str]:
    problems = check_manifest(product_root, repository_root)
    problems.extend(check_stamp_targets(product_root))
    seen: set[Path] = set()
    for pattern in RELEASE_SCRIPT_PATTERNS:
        for script in sorted(product_root.glob(pattern)):
            if script not in seen:
                seen.add(script)
                problems.extend(check_script(product_root, script))
    return problems


def self_test() -> int:
    """The gate must catch the break it exists for, and pass the layout once it is repaired."""
    with tempfile.TemporaryDirectory() as tmp:
        repository = Path(tmp)
        product = repository / "projects" / "demo"
        (product / "scripts").mkdir(parents=True)
        (product / "packaging" / "portable").mkdir(parents=True)
        (repository / "LICENSE").write_text("license", encoding="utf-8")
        (product / ".nvmrc").write_text("22", encoding="utf-8")
        (product / "scripts" / "release-build.ps1").write_text("# build\n", encoding="utf-8")
        (product / "release.manifest.json").write_text(
            json.dumps(
                {
                    "root": "projects/demo",
                    "node_version_file": ".nvmrc",
                    "python_version_file": ".python-version",
                    "builds": [{"id": "a", "script": "scripts/release-build.ps1"}],
                }
            ),
            encoding="utf-8",
        )
        portable = product / "packaging" / "portable" / "build.ps1"
        # The break this gate exists for: the license is at the repository root, not the product root.
        portable.write_text(
            '$RepoRoot = "x"\n'
            'Copy-Item (Join-Path $RepoRoot "LICENSE") -Destination out\n'
            '$Out = Join-Path $RepoRoot "releases"\n'
            "# Join-Path $RepoRoot \"commented-out\"\n",
            encoding="utf-8",
        )
        problems = find_problems(product, repository)
        expected = {"build.ps1: names 'LICENSE'", "python_version_file '.python-version' is missing"}
        for fragment in expected:
            if not any(fragment in problem for problem in problems):
                print(f"self-test: expected a problem containing {fragment!r}, got {problems}")
                return 1
        if len(problems) != len(expected):
            print(f"self-test: expected exactly {len(expected)} problems, got {problems}")
            return 1

        # Repaired: the license is looked up from above, and the version file exists.
        portable.write_text(
            '$RepoRoot = "x"\n'
            'for ($Dir = $RepoRoot; $Dir; $Dir = Split-Path -Parent $Dir) { Join-Path $Dir "LICENSE" }\n',
            encoding="utf-8",
        )
        (product / ".python-version").write_text("3.12", encoding="utf-8")
        leftover = find_problems(product, repository)
        if leftover:
            print(f"self-test: the repaired layout still reports {leftover}")
            return 1

        # Paths named after Set-Location are relative to the product root.
        (product / "scripts" / "release-build.ps1").write_text(
            'Set-Location $repoRoot\n& ".\\packaging\\portable\\missing.ps1"\npython scripts/missing.py\n',
            encoding="utf-8",
        )
        relative = find_problems(product, repository)
        if len(relative) != 2:
            print(f"self-test: expected the two relative names to be reported, got {relative}")
            return 1
    print("check_release_layout self-test: ok")
    return 0


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()
    problems = find_problems(ROOT, REPOSITORY_ROOT)
    if problems:
        print("release layout: the release build names files that are not there:", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        return 1
    print("release layout: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
