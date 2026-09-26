"""Compatibility entrypoint for provider-owned messaging adapters.

Concrete platform adapters live in optional extensions. Foundation exposes only the generic
adapter factory and BasePlatformAdapter contract.
"""

from __future__ import annotations

from integrations.connectors.secret_source import SecretSource
from integrations.connectors.base import BasePlatformAdapter
from integrations.connectors.messaging_providers import make_messaging_adapter


def make_adapter(platform: str, secrets: SecretSource) -> BasePlatformAdapter | None:
    return make_messaging_adapter(platform, secrets)


__all__ = ["make_adapter"]
