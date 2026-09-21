"""Foundation-owned execution factory contract for optional connector tools.

Extensions may contribute concrete tool implementations, but registration does not grant
permissions or create a second tool authority. Every returned tool still passes through
Foundation Runtime, Policy and Approval using its attached metadata/effects.
"""

from __future__ import annotations

from collections.abc import Callable
from threading import RLock
from typing import Any, Protocol

from packages.credential_store import CredentialStore as SecretStore


Tool = Callable[..., Any]


class ConnectorToolFactory(Protocol):
    def __call__(
        self,
        secrets: SecretStore,
        *,
        enabled_connectors: set[str] | None = None,
        enabled_tools: set[str] | None = None,
        roots: list[Any] | None = None,
    ) -> list[Tool]: ...


_LOCK = RLock()
_FACTORIES: dict[str, ConnectorToolFactory] = {}



def register_tool_factory(
    name: str,
    factory: ConnectorToolFactory,
    *,
    should_replace: bool = False,
) -> None:
    """Register one optional execution provider through the public extension seam."""

    key = str(name or "").strip()
    if not key:
        raise ValueError("tool factory name is required")
    if not callable(factory):
        raise TypeError("tool factory must be callable")
    with _LOCK:
        if key in _FACTORIES and not should_replace:
            raise ValueError(f"tool factory already registered: {key}")
        _FACTORIES[key] = factory


def unregister_tool_factory(name: str) -> None:
    with _LOCK:
        _FACTORIES.pop(str(name or "").strip(), None)


def clear_tool_factories() -> None:
    """Clear optional execution providers. Intended for packaging changes/tests."""

    with _LOCK:
        _FACTORIES.clear()


def registered_tool_factories() -> tuple[str, ...]:
    with _LOCK:
        return tuple(sorted(_FACTORIES))


def make_extension_tools(
    secrets: SecretStore,
    *,
    enabled_connectors: set[str] | None = None,
    enabled_tools: set[str] | None = None,
    roots: list[Any] | None = None,
) -> list[Tool]:
    """Build tools from registered providers in deterministic provider order."""
    with _LOCK:
        providers = tuple(sorted(_FACTORIES.items()))
    tools: list[Tool] = []
    seen: set[str] = set()
    for _name, factory in providers:
        provided = factory(
            secrets,
            enabled_connectors=enabled_connectors,
            enabled_tools=enabled_tools,
            roots=roots,
        )
        for tool in provided:
            tool_name = str(getattr(tool, "__name__", "") or "")
            if not tool_name or tool_name in seen:
                continue
            seen.add(tool_name)
            tools.append(tool)
    return tools


__all__ = [
    "ConnectorToolFactory",
    "Tool",
    "clear_tool_factories",
    "make_extension_tools",
    "register_tool_factory",
    "registered_tool_factories",
    "unregister_tool_factory",
]
