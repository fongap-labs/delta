"""Compatibility entrypoint for provider-owned messaging adapters.

Concrete platform adapters live in optional extensions. Foundation exposes only the generic
adapter factory and BasePlatformAdapter contract.
"""

from __future__ import annotations

from packages.credential_store import CredentialStore as SecretStore
from integrations.connectors.base import BasePlatformAdapter
from integrations.connectors.messaging_providers import make_messaging_adapter


def make_adapter(platform: str, secrets: SecretStore) -> BasePlatformAdapter | None:
    return make_messaging_adapter(platform, secrets)


__all__ = ["make_adapter"]
