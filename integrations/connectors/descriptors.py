"""Connector descriptor contract and registry.

Delta Foundation owns the descriptor schema and registration mechanism. It does
not ship Fongap-maintained vendor catalogs or vendor validation logic; those are
extension implementations.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Iterable


@dataclass
class Field:
    key: str
    label: str
    secret: bool = False
    required: bool = True
    help: str = ""
    placeholder: str = ""

    def to_dict(self) -> dict:
        return {
            "key": self.key,
            "label": self.label,
            "secret": self.secret,
            "required": self.required,
            "help": self.help,
            "placeholder": self.placeholder,
        }


@dataclass
class ValidationResult:
    ok: bool
    identity: str | None = None
    error: str | None = None


@dataclass
class ConnectorDescriptor:
    name: str
    title: str
    icon: str
    blurb: str
    auth: str
    two_way: bool
    fields: list[Field]
    instructions: list[str]
    available: bool = True
    channels: bool = False
    validate: Callable[[dict], ValidationResult] | None = None
    brand_color: str = "#6b7280"
    logo: str = ""
    aliases: tuple = ()
    mcp_url: str = ""
    experimental: bool = False
    risk_notice: str = ""
    managed: bool = False
    managed_paused: bool = False
    account_field: str = ""


_DESCRIPTORS: list[ConnectorDescriptor] = []
_BY_NAME: dict[str, ConnectorDescriptor] = {}

# Compatibility alias for callers that only iterate the registry after extension
# bootstrap. New callers should prefer list_descriptors()/get_descriptor().
DESCRIPTORS = _DESCRIPTORS



def register_descriptor(
    descriptor: ConnectorDescriptor,
    *,
    should_replace: bool = False,
) -> None:
    """Register one concrete connector descriptor supplied by an extension."""
    name = str(descriptor.name).strip()
    if not name:
        raise ValueError("descriptor.name is required")
    existing = _BY_NAME.get(name)
    if existing is not None and not should_replace:
        raise ValueError(f"connector descriptor already registered: {name}")
    if existing is not None:
        _DESCRIPTORS[:] = [item for item in _DESCRIPTORS if item.name != name]
    _DESCRIPTORS.append(descriptor)
    _BY_NAME[name] = descriptor


def register_descriptors(
    descriptors: Iterable[ConnectorDescriptor],
    *,
    should_replace: bool = False,
) -> None:
    for descriptor in descriptors:
        register_descriptor(descriptor, should_replace=should_replace)


def unregister_descriptor(name: str) -> None:
    _BY_NAME.pop(name, None)
    _DESCRIPTORS[:] = [item for item in _DESCRIPTORS if item.name != name]


def clear_descriptors() -> None:
    """Reset extension registrations. Primarily useful for tests."""
    _DESCRIPTORS.clear()
    _BY_NAME.clear()


def list_descriptors() -> list[ConnectorDescriptor]:
    return list(_DESCRIPTORS)


def get_descriptor(name: str) -> ConnectorDescriptor | None:
    return _BY_NAME.get(name)
