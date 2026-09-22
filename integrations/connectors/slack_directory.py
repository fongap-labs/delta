"""Compatibility shim for the Suite-owned Slack directory implementation."""

from __future__ import annotations

from typing import Any

from packages.credential_store import CredentialStore as SecretStore
from integrations.connectors.messaging_providers import call_messaging_operation


def list_members(
    secrets: SecretStore,
    team_id: str,
    query: str = "",
    limit: int = 25,
    *,
    should_refresh: bool = False,
) -> dict[str, Any]:
    return call_messaging_operation(
        "slack", "list_members", secrets, team_id, query, limit, should_refresh=should_refresh
    )


def list_channels(
    secrets: SecretStore,
    team_id: str,
    query: str = "",
    limit: int = 25,
    *,
    should_refresh: bool = False,
) -> dict[str, Any]:
    return call_messaging_operation(
        "slack", "list_channels", secrets, team_id, query, limit, should_refresh=should_refresh
    )


def clear_cache(team_id: str | None = None) -> None:
    call_messaging_operation("slack", "clear_cache", team_id)


__all__ = ["clear_cache", "list_channels", "list_members"]
