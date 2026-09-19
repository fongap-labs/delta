"""Foundation messaging settings and inbound authorization.

Platform-specific credential discovery belongs to messaging providers. Foundation owns the
authorization data model and the final allow/deny decision.
"""

from __future__ import annotations

from dataclasses import dataclass, field

from packages.credential_store import CredentialStore as SecretStore
from integrations.connectors.base import SessionSource


@dataclass
class TeamAuth:
    """Authorization scoped to one provider-defined workspace/team."""

    allowed_users: set[str] = field(default_factory=set)
    allow_all: bool = False


@dataclass
class ConnectorSettings:
    platform: str
    enabled: bool = False
    allowed_users: set[str] = field(default_factory=set)
    allow_all: bool = False
    teams: dict[str, TeamAuth] = field(default_factory=dict)


def is_authorized(settings: ConnectorSettings, source: SessionSource) -> bool:
    """Foundation-owned inbound authorization decision."""
    team_id = getattr(source, "team_id", None)
    if team_id:
        team = settings.teams.get(team_id)
        if team is None:
            return False
        if team.allow_all:
            return True
        uid = source.user_id
        return bool(uid) and uid in team.allowed_users
    if settings.allow_all:
        return True
    uid = source.user_id
    return bool(uid) and uid in settings.allowed_users


def load_settings(
    secrets: SecretStore | None = None,
) -> dict[str, ConnectorSettings]:
    """Load settings supplied by installed messaging providers.

    Providers may discover credentials/workspaces, but returned settings are interpreted by
    Foundation's `is_authorized`; providers cannot override the authorization decision.
    """
    from integrations.connectors.messaging_providers import messaging_provider_settings

    secrets = secrets or SecretStore()
    provided = messaging_provider_settings(secrets)
    out: dict[str, ConnectorSettings] = {}
    for platform, settings in provided.items():
        if not isinstance(settings, ConnectorSettings):
            raise TypeError(
                f"messaging provider {platform!r} returned invalid ConnectorSettings"
            )
        if settings.platform != platform:
            raise ValueError(
                f"messaging provider settings platform mismatch: {platform!r} != {settings.platform!r}"
            )
        out[platform] = settings
    return out


__all__ = ["ConnectorSettings", "TeamAuth", "is_authorized", "load_settings"]
