"""Retired-branding consistency gate.

Prevents the codebase from re-introducing retired retired brand names in
production source. The R1.6 source-level rename swept legacy identifiers to
delta across the stack; this script makes sure new code doesn't regress.

Scope:
  - Scans Git-tracked files in apps/ core/ packages/ providers/ services/
    integrations/ packaging/ scripts/ (case-insensitive).
  - Skips docs/ (CHANGELOG / ADR / historical architecture records legitimately
    reference retired names as historical context).
  - Skips LICENSE / NOTICE / UPSTREAM.md (attribution governance).
  - Skips legal/provenance policy guards that necessarily contain retired
    identity strings in order to enforce the ban.
  - Skips files matching legacy_* / *_legacy* / test_*legacy* (legacy-compat
    tests that explicitly exercise old spell forms).
  - Skips lines that mention provenance context (historical lineage notes,
    port attribution records).

Forbidden patterns (case-insensitive):
  - Retired brand name and its lowercase form
  - `@ocw` (legacy Slack bot handle)
  - `ocw.` (legacy localStorage / event prefix)
  - `.openworker/` (legacy per-user state directory)
  - `OPENWORKER_` (legacy env var prefix)
  - `OpenWorkerRuntime` (legacy runtime class name)
  - `CoworkerManager` (legacy manager class name)
  - `ocw_home` (legacy env var)
  - `cowork` (legacy agent id / module name)

Run:

    python scripts/check_retired_branding.py

Exit code 1 on any violation.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

SCAN_DIRS = ("apps", "core", "packages", "providers", "services",
             "integrations", "packaging", "scripts")

SKIP_FILES = {
    # Legacy-compat test fixtures that pin old spell forms on purpose.
    "tests/test_fake_slack.py",
    "tests/test_source_citation.py",
    "tests/test_conversations_repair.py",
    # Legal/provenance guards must name attributed third parties to verify the
    # required notices. They are governance surfaces, not product branding.
    "scripts/check_license_policy.py",
    "scripts/check_provenance_policy.py",
    # Self — the patterns in this script mention the forbidden tokens.
    "scripts/check_retired_branding.py",
}

# Lines that match any of these regexes are NOT checked, even inside scanned
# dirs. This lets us keep provenance / attribution comments and historical
# lineage notes that legitimately reference the OpenWorker / cowork names
# without using them as live product identity.
EXEMPT_LINE_PATTERNS = [
    re.compile(r"andrewyng/openworker", re.IGNORECASE),
    re.compile(r"Semantic port of", re.IGNORECASE),
    re.compile(r"Port of andrewyng/openworker", re.IGNORECASE),
    re.compile(r"imports adapted coworker", re.IGNORECASE),
    # Historical lineage notes: "was historically `cowork` during the
    # OpenWorker lineage" / "OpenWorker-era rebrand aliases" / etc.
    re.compile(r"was historically", re.IGNORECASE),
    re.compile(r"OpenWorker-era|OpenWorker lineage", re.IGNORECASE),
    # OpenWorker platform split / archive provenance.
    re.compile(r"archive[d]? OpenWorker|OpenWorker platform split|OpenWorker rebrand", re.IGNORECASE),
]

FORBIDDEN_PATTERNS = [
    re.compile(r"OpenWorker", re.IGNORECASE),
    re.compile(r"@ocw\b", re.IGNORECASE),
    re.compile(r"\bocw\.", re.IGNORECASE),
    re.compile(r"\.openworker[/\\]"),
    re.compile(r"OPENWORKER_"),
    re.compile(r"OpenWorkerRuntime"),
    re.compile(r"CoworkerManager"),
    re.compile(r"\bocw_home\b"),
    re.compile(r"\bcowork\b", re.IGNORECASE),
    re.compile(r"coworker/", re.IGNORECASE),
    re.compile(r"core\.agents\.cowork\b", re.IGNORECASE),
    re.compile(r"COWORK_CAPABILITIES|COWORK_INSTRUCTIONS|COWORK_TOOLS"),
    re.compile(r"cowork_agent|cowork_tool_factory"),
    # Identifier-aware: catches "Cowork" / "cowork" embedded in larger
    # identifiers that the word-boundary pattern misses (CamelCase /
    # snake_case with cowork as a component, e.g. startCoworkSession,
    # stopCoworkRun, my_cowork_helper). The leading \w ensures standalone
    # `cowork` is still caught by the word-boundary pattern above.
    re.compile(r"\w[Cc]owork"),
]

_SKIP_PARTS = {
    ".git", "node_modules", "target", "__pycache__", ".venv",
    "dist", "releases", ".pytest_cache", "build",
}


def _tracked_files() -> list[Path]:
    out = subprocess.run(
        ["git", "ls-files"],
        cwd=REPO, capture_output=True, text=True, check=True,
    ).stdout
    return [REPO / line for line in out.splitlines() if line]


def _should_scan(path: Path) -> bool:
    rel = path.relative_to(REPO).as_posix()
    if rel in SKIP_FILES:
        return False
    parts = rel.split("/")
    if any(p in _SKIP_PARTS for p in parts):
        return False
    # Only scan source dirs (not docs/, not LICENSE, not UPSTREAM.md).
    if not any(rel.startswith(d + "/") for d in SCAN_DIRS):
        return False
    return True


def _is_exempt_line(line: str) -> bool:
    return any(p.search(line) for p in EXEMPT_LINE_PATTERNS)


def violations_for(text: str, rel_path: str) -> list[str]:
    out: list[str] = []
    for lineno, line in enumerate(text.splitlines(), 1):
        if _is_exempt_line(line):
            continue
        for pat in FORBIDDEN_PATTERNS:
            for m in pat.finditer(line):
                out.append(
                    f"{rel_path}:{lineno}: forbidden retired brand "
                    f"{pat.pattern!r} matched {m.group()!r}"
                )
    return out


def find_violations() -> list[str]:
    violations: list[str] = []
    for path in _tracked_files():
        if not _should_scan(path):
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        violations.extend(violations_for(text, path.relative_to(REPO).as_posix()))
    return violations


def main() -> int:
    violations = find_violations()
    if violations:
        print(
            "retired-branding gate FAILED — retired retired brand "
            "tokens found in production source:"
        )
        for v in violations:
            print(f"  {v}")
        print(
            "\nIf this is a legitimate historical reference, add the line "
            "to EXEMPT_LINE_PATTERNS or the file to SKIP_FILES in "
            "scripts/check_retired_branding.py."
        )
        return 1
    print("retired-branding gate clean: no retired retired brand tokens in production source")
    return 0


if __name__ == "__main__":
    sys.exit(main())
