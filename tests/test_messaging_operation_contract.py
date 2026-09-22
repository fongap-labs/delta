"""Contract checks for provider-owned messaging operations."""

from __future__ import annotations

import importlib.util

from integrations.connectors.messaging_providers import (
    MessagingProvider,
    clear_messaging_providers,
    messaging_operation,
    register_messaging_provider,
)


def _send(_store, _chat_id: str, _text: str, _thread_id: str | None):
    return None


def test_vendor_address_implementation_is_not_in_foundation() -> None:
    assert importlib.util.find_spec("integrations.connectors.slack_addr") is None


def test_provider_operation_can_supply_address_parsing() -> None:
    clear_messaging_providers()
    try:
        register_messaging_provider(
            MessagingProvider(
                platform="test_chat",
                send=_send,
                operations={"split_address": lambda value: ("workspace", value)},
            )
        )
        split_address = messaging_operation("test_chat", "split_address")
        assert split_address is not None
        assert split_address("channel") == ("workspace", "channel")
    finally:
        clear_messaging_providers()
