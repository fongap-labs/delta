"""Foundation-owned connector tool contract, registry and approval policy.

Concrete first-party tool catalogs are extension data. Foundation owns the schema,
registration rules, local enablement state and the final interpretation of read/write
semantics. Unknown tools remain conservative: their call-site approval default wins.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Iterable

from packages.credential_store import CredentialStore as SecretStore


@dataclass(frozen=True)
class ConnectorToolDef:
    connector: str
    name: str
    label: str
    kind: str
    description: str
    is_enabled_default: bool = True
    target_arg: str | None = None


TOOL_DEFS: list[ConnectorToolDef] = []
TOOL_TO_CONNECTOR: dict[str, str] = {}
TOOLS_BY_CONNECTOR: dict[str, list[ConnectorToolDef]] = {}
TARGET_ARGS: dict[str, str] = {"send_message": "target"}
_KIND_BY_NAME: dict[str, str] = {}
_BY_NAME: dict[str, ConnectorToolDef] = {}

_GENERIC_CAPS = {
    "read",
    "write",
    "network",
    "integration",
    "connector",
    "external",
    "secrets",
}

# Generic, standalone Foundation capabilities may keep public safety semantics. These
# are not first-party SaaS catalogs: local IMAP/SMTP is the usable baseline and the
# public-URL reader is part of Foundation's egress boundary.
_FOUNDATION_BASELINE: tuple[ConnectorToolDef, ...] = (
    ConnectorToolDef("email", "email_list_folders", "List folders", "read", "List mailbox folders."),
    ConnectorToolDef("email", "email_search", "Search email", "read", "Search the connected mailbox."),
    ConnectorToolDef("email", "email_read", "Read email", "read", "Read one email message."),
    ConnectorToolDef(
        "email",
        "email_download_attachment",
        "Download attachment",
        "write",
        "Save an email attachment into a granted local directory.",
    ),
    ConnectorToolDef(
        "email",
        "email_send",
        "Send email",
        "write",
        "Send email through the configured IMAP/SMTP account.",
        target_arg="to",
    ),
    ConnectorToolDef(
        "browser",
        "browser_read_url",
        "Read URL",
        "write",
        "Fetch a model-selected public URL through Foundation network guards.",
    ),
)



def _validate(tool: ConnectorToolDef) -> None:
    if not tool.connector.strip() or not tool.name.strip():
        raise ValueError("connector tool definition requires connector and name")
    if tool.kind not in {"read", "write"}:
        raise ValueError(f"unsupported connector tool kind: {tool.kind}")
    if tool.kind == "read" and tool.target_arg:
        raise ValueError("read tools cannot declare standing-rule targets")


def _index(tool: ConnectorToolDef) -> None:
    _BY_NAME[tool.name] = tool
    _KIND_BY_NAME[tool.name] = tool.kind
    TOOL_TO_CONNECTOR[tool.name] = tool.connector
    if tool.target_arg:
        TARGET_ARGS[tool.name] = tool.target_arg
    else:
        TARGET_ARGS.pop(tool.name, None)
    bucket = TOOLS_BY_CONNECTOR.setdefault(tool.connector, [])
    for index, existing in enumerate(bucket):
        if existing.name == tool.name:
            bucket[index] = tool
            break
    else:
        bucket.append(tool)
    for index, existing in enumerate(TOOL_DEFS):
        if existing.name == tool.name:
            TOOL_DEFS[index] = tool
            break
    else:
        TOOL_DEFS.append(tool)


def register_tool_def(tool: ConnectorToolDef, *, should_replace: bool = False) -> None:
    """Register one connector tool definition through the public extension contract.

    Registration declares capability metadata only. It does not grant permission or
    bypass Trust/Approval. Replacing an existing definition must be explicit.
    """

    _validate(tool)
    if tool.name in _BY_NAME and not should_replace:
        raise ValueError(f"connector tool already registered: {tool.name}")
    _index(tool)


def register_tool_defs(
    tools: Iterable[ConnectorToolDef], *, should_replace: bool = False
) -> None:
    for tool in tools:
        register_tool_def(tool, should_replace=should_replace)


def register_runtime_tool(
    name: str,
    *,
    capabilities: list[str] | None,
    description: str,
    label: str | None = None,
    is_enabled_default: bool = True,
    is_approval_required: bool | None = None,
    target_arg: str | None = None,
) -> ConnectorToolDef | None:
    """Register minimal metadata carried by a concrete runtime tool.

    Explicit side-effect evidence is fail-safe: a declared write capability or an
    explicit approval requirement can never be downgraded by a stale ``read`` tag.
    A curated installed-extension definition, when already present, still wins.
    """

    existing = _BY_NAME.get(name)
    if existing is not None:
        return existing
    caps = [str(cap).strip() for cap in (capabilities or []) if str(cap).strip()]
    connector = next((cap for cap in caps if cap not in _GENERIC_CAPS), "")
    if not connector:
        return None
    if "write" in caps or is_approval_required is True:
        kind = "write"
    elif "read" in caps:
        kind = "read"
    else:
        return None
    tool = ConnectorToolDef(
        connector=connector,
        name=name,
        label=label or name.replace("_", " ").strip().capitalize(),
        kind=kind,
        description=str(description or ""),
        is_enabled_default=bool(is_enabled_default),
        target_arg=target_arg,
    )
    register_tool_def(tool)
    return tool


def _register_foundation_baseline() -> None:
    for tool in _FOUNDATION_BASELINE:
        register_tool_def(tool, should_replace=True)


def clear_tool_defs(
    *, should_keep_target: bool = True, should_restore_baseline: bool = True
) -> None:
    """Clear extension/runtime registrations while preserving the usable baseline."""

    TOOL_DEFS.clear()
    TOOL_TO_CONNECTOR.clear()
    TOOLS_BY_CONNECTOR.clear()
    _KIND_BY_NAME.clear()
    _BY_NAME.clear()
    TARGET_ARGS.clear()
    if should_keep_target:
        TARGET_ARGS["send_message"] = "target"
    if should_restore_baseline:
        _register_foundation_baseline()


def get_tool_def(name: str) -> ConnectorToolDef | None:
    return _BY_NAME.get(name)


def approval_for_tool(name: str, is_default_approval: bool = True) -> bool:
    """Foundation's final approval interpretation for connector effects."""

    kind = _KIND_BY_NAME.get(name)
    if kind is None:
        return is_default_approval
    return kind != "read"


def target_arg_for(tool_name: str) -> str | None:
    return TARGET_ARGS.get(tool_name)


def connector_for_tool(tool_name: str) -> str | None:
    return TOOL_TO_CONNECTOR.get(tool_name)


def load_tool_settings(secrets: SecretStore, connector: str) -> dict[str, bool]:
    raw = secrets.get(f"{connector}:tools") or {}
    enabled = raw.get("enabled") if isinstance(raw, dict) else None
    return {str(k): bool(v) for k, v in (enabled or {}).items()}


def tool_enabled(secrets: SecretStore, connector: str, tool_name: str) -> bool:
    overrides = load_tool_settings(secrets, connector)
    if tool_name in overrides:
        return overrides[tool_name]
    tool = _BY_NAME.get(tool_name)
    return bool(tool.is_enabled_default) if tool and tool.connector == connector else False


def patch_tool_settings(
    secrets: SecretStore, connector: str, enabled: dict[str, Any]
) -> dict[str, Any]:
    known = {tool.name for tool in TOOLS_BY_CONNECTOR.get(connector, [])}
    if not known:
        return {"ok": False, "error": "unknown connector or no tools"}
    current = load_tool_settings(secrets, connector)
    for name, value in enabled.items():
        if name in known:
            current[name] = bool(value)
    secrets.put(f"{connector}:tools", {"enabled": current})
    return {"ok": True, "tools": current}


def mcp_tool_defs(connector: str) -> list[ConnectorToolDef]:
    return [
        tool
        for tool in TOOLS_BY_CONNECTOR.get(connector, [])
        if tool.name.startswith("mcp__")
    ]


def mcp_pinned_tools(connector: str) -> list[str]:
    prefix = f"mcp__{connector}__"
    return [tool.name.removeprefix(prefix) for tool in mcp_tool_defs(connector)]


def active_tool_defs(secrets: SecretStore, connector: str) -> list[ConnectorToolDef]:
    defs = list(TOOLS_BY_CONNECTOR.get(connector, []))
    mcp = [tool for tool in defs if tool.name.startswith("mcp__")]
    api = [tool for tool in defs if not tool.name.startswith("mcp__")]
    if not mcp or not api:
        return defs
    profile = secrets.get(f"{connector}:default") or {}
    return mcp if profile.get("mode") == "mcp" else api


def tool_dicts(secrets: SecretStore, connector: str) -> list[dict[str, Any]]:
    overrides = load_tool_settings(secrets, connector)
    return [
        {
            "name": tool.name,
            "label": tool.label,
            "kind": tool.kind,
            "description": tool.description,
            "enabled": bool(overrides.get(tool.name, tool.is_enabled_default)),
            "requires_approval": tool.kind != "read",
        }
        for tool in active_tool_defs(secrets, connector)
    ]


_register_foundation_baseline()
