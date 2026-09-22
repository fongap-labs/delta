"""Enforce the Delta production architecture.

The product path is React -> Tauri IPC -> Rust Runtime -> controlled
capabilities. Python remains valid only inside capability/worker code. This
guard scans every production root and reports every violation in one run.

Historical names needed by a negative assertion may be exempted only through
``EXEMPT_FILES`` below. Every exemption has a narrow rule set and a reason;
there is no pattern-level or directory-wide escape hatch.
"""

from __future__ import annotations

from dataclasses import dataclass
import re
import sys
from pathlib import Path
from typing import Callable, Iterable


REPO = Path(__file__).resolve().parent.parent
SCAN_TARGETS = (
    "apps",
    "contracts",
    "core",
    "crates",
    "extensions",
    "packages",
    "providers",
    "schemas",
    "services",
    "integrations",
    "packaging",
    "scripts",
    ".github",
    "pyproject.toml",
)
SKIP_PARTS = frozenset(
    {".git", ".venv", "__pycache__", "build", "dist", "node_modules", "releases", "target"}
)
TEXT_SUFFIXES = frozenset(
    {
        ".cjs",
        ".css",
        ".html",
        ".js",
        ".json",
        ".md",
        ".mjs",
        ".ps1",
        ".py",
        ".rs",
        ".sh",
        ".toml",
        ".ts",
        ".tsx",
        ".yaml",
        ".yml",
    }
)


@dataclass(frozen=True)
class Rule:
    id: str
    description: str
    pattern: re.Pattern[str]
    applies: Callable[[str], bool]


@dataclass(frozen=True)
class Exemption:
    rules: frozenset[str]
    reason: str


def _under(rel: str, *roots: str) -> bool:
    return any(rel == root or rel.startswith(f"{root}/") for root in roots)


def _python(rel: str) -> bool:
    return rel.endswith(".py")


def _production_desktop(rel: str) -> bool:
    return _under(rel, "apps/desktop/src", "apps/desktop/src-tauri/src")


def _python_authority(rel: str) -> bool:
    return _python(rel) and _under(
        rel, "apps", "core", "packages", "providers", "services", "integrations"
    )


def _packaging_surface(rel: str) -> bool:
    return rel == "pyproject.toml" or _under(
        rel, "packaging", "scripts", ".github/workflows"
    )


RULES = (
    Rule(
        "python-web-backend",
        "Python application web backend",
        re.compile(
            r"(?im)^\s*(?:from\s+fastapi\b|import\s+(?:fastapi|uvicorn)\b)|"
            r"\bFastAPI\s*\(|\buvicorn\.run\s*\("
        ),
        _python_authority,
    ),
    Rule(
        "python-web-dependency",
        "Python application web backend dependency",
        re.compile(r'["\'](?:fastapi|uvicorn(?:\[standard\])?)\s*[=<>@]', re.I),
        lambda rel: rel == "pyproject.toml",
    ),
    Rule(
        "server-packaging",
        "retired Python server packaging",
        re.compile(
            r"(?i)\bdelta-server(?:\.exe)?\b|\bserver_entry\.py\b|"
            r"\bpyinstaller\b[^\n]*\bdelta-server\b|"
            r"packaging[\\/]server|sidecar[\\/]delta-server"
        ),
        _packaging_surface,
    ),
    Rule(
        "desktop-legacy-transport",
        "desktop localhost HTTP/WebSocket runtime transport",
        re.compile(
            r"\bstart_proxy\b|__DELTA_HTTP__|__DELTA_WS__|"
            r"(?:https?|wss?)://127\.0\.0\.1:8765|/ws/session\b"
        ),
        _production_desktop,
    ),
    Rule(
        "python-turn-authority",
        "Python turn/session/agent-loop authority",
        re.compile(
            r"^\s*class\s+(?:TurnEngine|SessionManager)\b|"
            r"^\s*(?:from|import)\s+(?:core\.)?(?:engine|sessions\.manager)\b|"
            r"^\s*(?:[A-Za-z_]\w*\s*=\s*|return\s+)"
            r"(?:TurnEngine|SessionManager)\s*\(",
            re.M,
        ),
        _python_authority,
    ),
    Rule(
        "python-provider-authority",
        "Python provider routing used by an application authority",
        re.compile(
            r"^\s*from\s+providers\.(?:anthropic_provider|openai_provider|"
            r"openai_responses|registry|router)\s+import\b|"
            r"\b(?:AnthropicProvider|OpenAIProvider|OpenAIResponsesProvider|"
            r"ProviderRouter)\s*\(",
            re.M,
        ),
        lambda rel: _python(rel)
        and _under(rel, "apps", "core", "packages", "services", "integrations"),
    ),
    Rule(
        "aisuite-application-runtime",
        "aisuite used outside a controlled capability definition",
        re.compile(r"^\s*(?:from\s+aisuite\b|import\s+aisuite\b)", re.M),
        lambda rel: _python(rel)
        and _under(rel, "apps", "providers", "services", "core"),
    ),
    Rule(
        "persona-platform",
        "retired multi-persona product authority",
        re.compile(
            r"(?i)\b(?:code|chat|ops)\s+persona\b|\bmulti[- ]persona\b|"
            r"\b(?:PersonasTab|PersonaView|personaScope)\b"
        ),
        lambda rel: _under(
            rel,
            "apps/desktop/src",
            "apps/desktop/src-tauri/src",
            "core",
            "packages",
            "providers",
            "services",
            "integrations",
        ),
    ),
    # -- Authority and architecture regression guards ---------------------------
    Rule(
        "retired-model-authority-write",
        "retired dual ModelAuthority files used as a production write target (must be model-authority.json)",
        re.compile(
            r"(?:std::fs::write|fs::write)\s*\(\s*[^)]*\bprefs\.json\b"
            r"|"
            r"(?:std::fs::write|fs::write)\s*\(\s*[^)]*\bsecrets\.json\b"
        ),
        lambda rel: rel.endswith(".rs") and _under(rel, "crates/delta-core/src"),
    ),
    Rule(
        "retired-connector-brand-icons",
        "retired frontend connector brand-icon catalog (registry.tsx is the sole visual layer)",
        re.compile(r"brandIcons"),
        lambda rel: _under(rel, "apps/desktop/src"),
    ),
    Rule(
        "second-runtime-scheduler",
        "a second runtime scheduler thread (delta-rust-scheduler is the sole scheduler)",
        re.compile(r".*Builder\s*\(\s*\)\s*\.name\s*\(\s*[\"']delta-rust-scheduler[\"']"),
        lambda rel: rel.endswith(".rs") and _under(rel, "apps/desktop/src-tauri/src"),
    ),
    Rule(
        "tauri-direct-authority-construction",
        "Tauri shell opens a durable Core authority directly",
        re.compile(
            r"\b(?:ModelAuthority|RuntimeAuthorities|MemoryStore|AutomationStore|McpStore|"
            r"SkillStore|ApplicationStore|ApprovalWriter|SourceCitationReader|ValidationReader)"
            r"::open\s*\(|\bCapabilityHost::product_defaults\s*\("
        ),
        lambda rel: rel.endswith(".rs") and _under(rel, "apps/desktop/src-tauri/src"),
    ),
    Rule(
        "tauri-direct-control-plane",
        "Tauri shell bypasses CoreControlPlane and calls persistence control functions directly",
        re.compile(r"(?:delta_core::)?control_plane::"),
        lambda rel: rel.endswith(".rs") and _under(rel, "apps/desktop/src-tauri/src"),
    ),
    Rule(
        "tauri-runtime-authority",
        "Tauri shell creates RuntimeHost/RuntimeHandle instead of delegating to CoreControlPlane",
        re.compile(r"\bRuntimeHost::new\s*\(|\bRuntimeHandle::spawn\s*\("),
        lambda rel: rel.endswith(".rs") and _under(rel, "apps/desktop/src-tauri/src"),
    ),
    # -- Cross-layer and contract guards -----------------------------------------
    Rule(
        "cross-layer-import-core-to-apps",
        "Core Rust code imports from apps/ (layer violation: core must not depend on apps)",
        re.compile(r"use\s+crate::apps\b|use\s+apps\b|use\s+delta_core::apps\b"),
        lambda rel: rel.endswith(".rs") and _under(rel, "crates/delta-core/src"),
    ),
    Rule(
        "cross-layer-import-integrations-to-core",
        "Integrations Python code imports from core/ authority internals (layer violation)",
        re.compile(r"^\s*(?:from|import)\s+core\.(?:engine|sessions\.manager|model_authority|automation|ledger)\b", re.M),
        lambda rel: _python(rel) and _under(rel, "integrations"),
    ),
    Rule(
        "private-type-leak-rust",
        "Private Rust type (not pub) exported in public API via pub use of non-pub item",
        re.compile(r"pub\s+use\s+crate::[a-z_]+::[a-z_]{3,};"),
        lambda rel: rel.endswith(".rs") and _under(rel, "crates"),
    ),
    Rule(
        "private-fn-leak-python",
        "Python private function (leading underscore) used outside its module",
        re.compile(r"from\s+[^.]+\s+import\s+_[A-Za-z_][A-Za-z0-9_]*\b"),
        _python,
    ),
    Rule(
        "schema-compat-manifest",
        "Capability manifest must validate against schemas/capability-manifest.schema.json",
        re.compile(r"^integrations/capabilities/[^/]+/manifest\.json$"),
        lambda rel: True,
    ),
    Rule(
        "api-compat-capability-job",
        "CapabilityJob wire format divergence: check for field mismatches vs delta-sdk",
        re.compile(r"(?<!delta_worker_sdk\.)(?<!protocol\.)(?<!context\.)(?<!test_)\bCapabilityJob\b"),
        lambda rel: rel.endswith(".py") and _under(rel, "integrations/capabilities") and not rel.endswith("_test.py") and "test_" not in rel.split("/")[-1],
    ),
Rule(
        "entry-point-consistency",
        "Extension entry point group must be 'delta.extensions' (reject non-standard groups)",
        re.compile(r"ENTRY_POINT_GROUP\s*=\s*[\"'](?!delta\.extensions)[\"']"),
        lambda rel: rel.endswith(".py") and _under(rel, "extensions", "integrations/connectors"),
    ),
)


# Enforcement files must name retired tokens to reject them. These are the
# only file-level exemptions, and each exemption is restricted to named rules.
EXEMPT_FILES: dict[str, Exemption] = {
    "scripts/check_architecture_boundary.py": Exemption(
        frozenset(rule.id for rule in RULES),
        "the guard must encode every forbidden token in order to reject it",
    ),
    "packaging/portable/build-portable.ps1": Exemption(
        frozenset({"server-packaging"}),
        "portable build contains a negative archive-content assertion for retired server names",
    ),
    ".github/workflows/release.yml": Exemption(
        frozenset({"server-packaging"}),
        "release validation contains a negative archive-content assertion for retired server names",
    ),
    "scripts/check_retired_paths.py": Exemption(
        frozenset({"server-packaging"}),
        "the legacy-path guard names deleted server packaging paths solely to reject references",
    ),
    "scripts/check_architecture_boundary.py": Exemption(
        frozenset(rule.id for rule in RULES),
        "the guard must encode every forbidden token in order to reject it",
    ),
}


STRUCTURAL_BANS: dict[str, str] = {
    "apps/desktop/src-tauri/src/proxy.rs": "retired desktop localhost proxy module",
    "services/server": "retired Python application backend directory",
    "packaging/server": "retired Python server packaging directory",
    "packaging/sidecar/delta-server": "retired Python application sidecar",
    "providers": "retired Python provider authority package",
}


def _iter_files(repo: Path) -> Iterable[tuple[Path, str]]:
    for target_name in SCAN_TARGETS:
        target = repo / target_name
        if target.is_file():
            yield target, target_name
            continue
        if not target.is_dir():
            continue
        for path in target.rglob("*"):
            if not path.is_file() or SKIP_PARTS.intersection(path.relative_to(repo).parts):
                continue
            if path.suffix.lower() not in TEXT_SUFFIXES:
                continue
            yield path, path.relative_to(repo).as_posix()


def _line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def scan_repository(repo: Path) -> tuple[list[str], set[str]]:
    """Return all violations and the rule ids that were actually evaluated."""
    repo = repo.resolve()
    violations: list[str] = []
    evaluated: set[str] = set()

    for rel, description in STRUCTURAL_BANS.items():
        evaluated.add(f"path:{rel}")
        target = repo / rel
        if target.is_file() or (
            target.is_dir()
            and any(
                path.is_file()
                and not SKIP_PARTS.intersection(path.relative_to(repo).parts)
                for path in target.rglob("*")
            )
        ):
            violations.append(f"{rel}: {description}")

    for path, rel in _iter_files(repo):
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        exemption = EXEMPT_FILES.get(rel)
        for rule in RULES:
            evaluated.add(rule.id)
            if not rule.applies(rel):
                continue
            if exemption and rule.id in exemption.rules:
                continue
            for match in rule.pattern.finditer(text):
                violations.append(
                    f"{rel}:{_line_number(text, match.start())}: "
                    f"{rule.id} - {rule.description}"
                )

    return sorted(set(violations)), evaluated


def main() -> int:
    violations, evaluated = scan_repository(REPO)
    expected = {rule.id for rule in RULES} | {
        f"path:{rel}" for rel in STRUCTURAL_BANS
    }
    missing = sorted(expected - evaluated)
    if missing:
        violations.extend(f"guard-internal: rule was not evaluated: {rule}" for rule in missing)

    if violations:
        print("architecture boundary guard FAILED:", file=sys.stderr)
        for violation in violations:
            print(f"  - {violation}", file=sys.stderr)
        print(
            "\nProduction must remain React -> Tauri IPC -> Rust Runtime -> "
            "controlled capability workers.",
            file=sys.stderr,
        )
        return 1

    print(
        f"architecture boundary guard passed: {len(RULES)} content rules and "
        f"{len(STRUCTURAL_BANS)} structural rules evaluated across "
        f"{len(SCAN_TARGETS)} required targets"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
