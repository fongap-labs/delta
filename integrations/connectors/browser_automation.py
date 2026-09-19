"""Public browser-automation extension contract for Delta Foundation.

Foundation owns registration, tool metadata and network-safety boundaries. Concrete
interactive browser implementations are explicit extension providers and register
through this module.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable, Optional

from integrations.tools import metadata as ai
from integrations.tools.metadata import attach_tool_metadata
from integrations.web.guard import check_url


BrowserTool = Callable[..., Any]
BrowserToolFactory = Callable[[], list[BrowserTool]]
BrowserStateFn = Callable[[], dict[str, Any]]
BrowserActionFn = Callable[[], dict[str, Any]]


@dataclass(frozen=True)
class BrowserAutomationProvider:
    """Public provider surface for optional interactive browser implementations."""

    make_tools: BrowserToolFactory
    state: BrowserStateFn
    screenshot: BrowserActionFn
    close: BrowserActionFn
    name: str = "browser-automation"


_PROVIDER: BrowserAutomationProvider | None = None



def browser_tool_schema(
    name: str, description: str, properties: dict[str, Any], required: list[str]
) -> dict[str, Any]:
    """Build the stable Delta function schema used by browser extension tools."""

    return {
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": required,
            },
        },
    }


def attach_browser_tool(
    fn: BrowserTool,
    schema: dict[str, Any],
    *,
    is_approval_required: bool = True,
    capabilities: list[str] | None = None,
) -> BrowserTool:
    """Attach Foundation-owned metadata to an extension-provided browser tool."""

    from integrations.connectors.tool_defs import approval_for_tool, register_runtime_tool

    name = str(schema["function"]["name"])
    description = str(schema["function"].get("description") or "")
    register_runtime_tool(
        name,
        capabilities=capabilities or ["browser", "write"],
        description=description,
    )
    is_approval_required = approval_for_tool(name, is_default_approval=is_approval_required)
    metadata = ai.ToolMetadata(
        name=name,
        category="connector",
        risk_level="medium" if is_approval_required else "low",
        capabilities=capabilities or ["browser"],
        requires_approval=is_approval_required,
    )
    attach_tool_metadata(fn, schema=schema, metadata=metadata)
    fn.__doc__ = description
    return fn


def browser_url_refusal(url: str) -> Optional[str]:
    """Return a refusal reason when Foundation network policy rejects ``url``."""

    return check_url(url)


def redirect_refusal(requested: str, final: str) -> Optional[str]:
    """Re-check a browser landing URL after a redirect."""

    if not final or final == requested:
        return None
    return browser_url_refusal(final)


def register_browser_provider(
    provider: BrowserAutomationProvider, *, should_replace: bool = False
) -> None:
    """Register one optional browser provider.

    Registration changes capability availability only. It does not grant permissions;
    every returned tool still passes through Foundation metadata, Policy and Approval.
    """

    global _PROVIDER
    if _PROVIDER is not None and not should_replace:
        raise RuntimeError(
            f"browser automation provider already registered: {_PROVIDER.name}"
        )
    _PROVIDER = provider


def clear_browser_provider() -> None:
    """Remove the current provider, primarily for packaging changes and tests."""

    global _PROVIDER
    _PROVIDER = None


def browser_automation_provider() -> BrowserAutomationProvider | None:
    return _PROVIDER


def build_browser_tools() -> list[BrowserTool]:
    """Return provider tools, or an empty set in standalone Foundation."""
    provider = _PROVIDER
    return provider.make_tools() if provider is not None else []


def browser_state() -> dict[str, Any]:
    provider = _PROVIDER
    if provider is None:
        return {
            "available": False,
            "open": False,
            "status": "unavailable",
            "provider": None,
        }
    state = dict(provider.state())
    state.setdefault("available", True)
    state.setdefault("provider", provider.name)
    return state


def browser_take_screenshot() -> dict[str, Any]:
    provider = _PROVIDER
    if provider is None:
        return {"error": "interactive browser automation is not installed"}
    return provider.screenshot()


def browser_close_session() -> dict[str, Any]:
    provider = _PROVIDER
    if provider is None:
        return {"ok": True, "closed": False, "reason": "provider unavailable"}
    return provider.close()


_schema = browser_tool_schema
_attach = attach_browser_tool
