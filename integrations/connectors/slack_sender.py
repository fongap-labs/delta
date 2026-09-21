"""Compatibility shim for Suite-owned Slack outbound identity decoration."""

from __future__ import annotations

from packages.credential_store import CredentialStore as SecretStore
from integrations.connectors.messaging_providers import messaging_operation


def outbound_prefix(store: SecretStore, target: str) -> str:
    fn = messaging_operation("slack", "outbound_prefix")
    if fn is None:
        return ""
    return str(fn(store, target) or "")


__all__ = ["outbound_prefix"]
