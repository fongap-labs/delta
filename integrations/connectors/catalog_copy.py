"""Generic connector catalog copy registry.

Delta Foundation owns only the contract and registry. Product/vendor-specific
copy is supplied by installed extension providers.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping

_DEFAULT_ACCESS = [
    "Access is limited to what the installed connector provider declares."
]

_ABOUT: dict[str, str] = {}
_ACCESS: dict[str, tuple[str, ...]] = {}



def register_catalog_copy(
    name: str,
    *,
    about: str = "",
    access: Iterable[str] = (),
    should_replace: bool = False,
) -> None:
    """Register product copy for one connector supplied by an extension."""
    key = str(name).strip()
    if not key:
        raise ValueError("connector name is required")
    if not should_replace and (key in _ABOUT or key in _ACCESS):
        raise ValueError(f"catalog copy already registered: {key}")
    _ABOUT[key] = str(about or "")
    _ACCESS[key] = tuple(str(item) for item in access if str(item).strip())


def register_catalog(
    *,
    about: Mapping[str, str] | None = None,
    access: Mapping[str, Iterable[str]] | None = None,
    should_replace: bool = False,
) -> None:
    """Bulk-register connector catalog copy from an extension provider."""
    about = about or {}
    access = access or {}
    for name in dict.fromkeys([*about.keys(), *access.keys()]):
        register_catalog_copy(
            name,
            about=about.get(name, ""),
            access=access.get(name, ()),
            should_replace=should_replace,
        )


def unregister_catalog_copy(name: str) -> None:
    _ABOUT.pop(name, None)
    _ACCESS.pop(name, None)


def clear_catalog_copy() -> None:
    """Reset provider registrations. Primarily useful for tests."""
    _ABOUT.clear()
    _ACCESS.clear()


def about_for(name: str) -> str:
    return _ABOUT.get(name, "")


def access_for(name: str) -> list[str]:
    return list(_ACCESS.get(name) or _DEFAULT_ACCESS)


def registered_connector_names() -> tuple[str, ...]:
    return tuple(dict.fromkeys([*_ABOUT.keys(), *_ACCESS.keys()]))
