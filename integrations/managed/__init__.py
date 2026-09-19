"""Managed Capability Ports — vendor-agnostic Foundation contracts.

Delta Desktop can operate fully without any managed service (local-first).
These protocols define optional capability boundaries that a managed service
or third-party adapter can implement for OAuth, relay, and external identity
federation.

Only interfaces and Null* defaults live in Foundation. Concrete managed
service implementations and vendor-specific brokers belong outside the public
Foundation repository.
"""

from __future__ import annotations

from integrations.managed.errors import ManagedUnavailableError
from integrations.managed.identity import (
    ExternalIdentity,
    ExternalIdentityProvider,
    NullIdentityProvider,
)
from integrations.managed.models import ManagedConfig
from integrations.managed.oauth import NullOAuthBroker, OAuthBroker
from integrations.managed.relay import NullRelayTransport, RelayTransport

__all__ = [
    "ExternalIdentity",
    "ExternalIdentityProvider",
    "NullIdentityProvider",
    "ManagedConfig",
    "ManagedUnavailableError",
    "NullOAuthBroker",
    "NullRelayTransport",
    "OAuthBroker",
    "RelayTransport",
]
