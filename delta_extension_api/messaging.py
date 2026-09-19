"""Stable messaging extension contracts exposed by Delta Foundation."""

from integrations.connectors.base import (
    BasePlatformAdapter,
    InteractionEvent,
    MessageEvent,
    MessageSource,
    MessageType,
    SendResult,
    SessionSource,
    format_target,
    parse_target,
)
from integrations.connectors.config import ConnectorSettings, TeamAuth, is_authorized
from integrations.connectors.messaging_providers import (
    MessagingProvider,
    call_messaging_operation,
    make_messaging_adapter,
    messaging_operation,
    messaging_provider,
    messaging_provider_settings,
    register_messaging_provider,
    registered_messaging_providers,
    reject_messaging_interaction,
    resolve_messaging_target,
)

__all__ = [
    "BasePlatformAdapter",
    "ConnectorSettings",
    "InteractionEvent",
    "MessageEvent",
    "MessageSource",
    "MessageType",
    "MessagingProvider",
    "SendResult",
    "SessionSource",
    "TeamAuth",
    "call_messaging_operation",
    "format_target",
    "is_authorized",
    "make_messaging_adapter",
    "messaging_operation",
    "messaging_provider",
    "messaging_provider_settings",
    "parse_target",
    "register_messaging_provider",
    "registered_messaging_providers",
    "reject_messaging_interaction",
    "resolve_messaging_target",
]
