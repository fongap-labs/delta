"""Foundation connector contracts, gateway authority and extension seams."""

from __future__ import annotations

from integrations.connectors.adapters import make_adapter
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
from integrations.connectors.config import ConnectorSettings, TeamAuth, is_authorized, load_settings
from integrations.connectors.descriptors import ConnectorDescriptor, get_descriptor, list_descriptors
from integrations.connectors.fake import FakeAdapter
from integrations.connectors.gateway import Gateway
from integrations.connectors.secret_source import SecretSource
from integrations.connectors.connector_tools import build_connector_tools
from integrations.connectors.messaging_providers import (
    MessagingProvider,
    clear_messaging_providers,
    make_messaging_adapter,
    messaging_provider,
    register_messaging_provider,
    registered_messaging_providers,
    resolve_messaging_target,
)
from integrations.connectors.tool_defs import connector_for_tool
from integrations.connectors.tools import build_send_file, build_send_message

__all__ = [
    "BasePlatformAdapter",
    "ConnectorDescriptor",
    "ConnectorSettings",
    "FakeAdapter",
    "Gateway",
    "InteractionEvent",
    "MessageEvent",
    "MessageSource",
    "MessageType",
    "MessagingProvider",
    "SendResult",
    "SecretSource",
    "SessionSource",
    "TeamAuth",
    "clear_messaging_providers",
    "connector_for_tool",
    "format_target",
    "get_descriptor",
    "is_authorized",
    "list_descriptors",
    "load_settings",
    "make_adapter",
    "build_connector_tools",
    "make_messaging_adapter",
    "build_send_file",
    "build_send_message",
    "messaging_provider",
    "parse_target",
    "register_messaging_provider",
    "registered_messaging_providers",
    "resolve_messaging_target",
]
