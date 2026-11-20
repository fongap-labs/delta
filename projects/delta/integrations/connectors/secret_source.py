"""Read-only secret source contract for Foundation connector code.

Production Runtime paths should receive grant-scoped credentials from the trusted
Capability boundary. Headless/operator compatibility paths may inject the local
CredentialStore explicitly, but Foundation connector modules must not construct it.
"""

from __future__ import annotations

from typing import Any, Protocol


class SecretSource(Protocol):
    """Minimal credential read surface required by connector providers/tools."""

    def get(self, profile: str) -> dict[str, Any] | None: ...


__all__ = ["SecretSource"]
