"""Compatibility aliases for Foundation messaging provider contracts.

Concrete outbound transports belong to installed providers.
"""

from integrations.connectors.messaging_providers import FileSender, MessageSender

Sender = MessageSender

__all__ = ["FileSender", "Sender"]
