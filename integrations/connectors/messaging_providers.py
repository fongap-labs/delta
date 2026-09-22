"""Foundation-owned provider contract for optional messaging platforms.

Foundation owns message/event types, authorization, approval and routing. Extensions may
supply platform transports, target resolution and adapter construction, but registration
never grants permission or creates a second messaging authority.
"""

from __future__ import annotations

from collections.abc import Awaitable, Callable, Mapping
from dataclasses import dataclass, field
from threading import RLock
from typing import Any

from packages.credential_store import CredentialStore as SecretStore
from integrations.connectors.base import (
    BasePlatformAdapter,
    InteractionEvent,
    SendResult,
    parse_target,
)

MessageSender = Callable[[SecretStore, str, str, str | None], SendResult]
FileSender = Callable[
    [SecretStore, str, str | None, str, bytes, str | None, str | None], SendResult
]
BareTargetParser = Callable[[SecretStore, str], tuple[str, str | None] | None]
AdapterFactory = Callable[[SecretStore], BasePlatformAdapter | None]
SettingsLoader = Callable[[SecretStore], Any]
InteractionRejector = Callable[[InteractionEvent, str], Awaitable[None]]
ProviderOperation = Callable[..., Any]


@dataclass(frozen=True)
class MessagingProvider:
    platform: str
    send: MessageSender
    send_file: FileSender | None = None
    parse_bare_target: BareTargetParser | None = None
    make_adapter: AdapterFactory | None = None
    load_settings: SettingsLoader | None = None
    reject_interaction: InteractionRejector | None = None
    operations: Mapping[str, ProviderOperation] = field(default_factory=dict)


_LOCK = RLock()
_PROVIDERS: dict[str, MessagingProvider] = {}



def register_messaging_provider(
    provider: MessagingProvider, *, should_replace: bool = False
) -> None:
    key = str(provider.platform or "").strip().lower()
    if not key:
        raise ValueError("messaging provider platform is required")
    if not callable(provider.send):
        raise TypeError("messaging provider send must be callable")
    if any(not callable(operation) for operation in provider.operations.values()):
        raise TypeError("messaging provider operations must be callable")
    with _LOCK:
        if key in _PROVIDERS and not should_replace:
            raise ValueError(f"messaging provider already registered: {key}")
        _PROVIDERS[key] = provider


def unregister_messaging_provider(platform: str) -> None:
    with _LOCK:
        _PROVIDERS.pop(str(platform or "").strip().lower(), None)


def clear_messaging_providers() -> None:
    with _LOCK:
        _PROVIDERS.clear()


def messaging_provider(platform: str) -> MessagingProvider | None:
    with _LOCK:
        return _PROVIDERS.get(str(platform or "").strip().lower())


def registered_messaging_providers() -> tuple[str, ...]:
    with _LOCK:
        return tuple(sorted(_PROVIDERS))


def messaging_provider_settings(secrets: SecretStore) -> dict[str, Any]:
    """Load platform settings contributed by registered providers."""
    with _LOCK:
        providers = tuple(sorted(_PROVIDERS.items()))
    out: dict[str, Any] = {}
    for platform, provider in providers:
        if provider.load_settings is None:
            continue
        settings = provider.load_settings(secrets)
        if settings is not None:
            out[platform] = settings
    return out


def messaging_operation(platform: str, operation: str) -> ProviderOperation | None:
    provider = messaging_provider(platform)
    if provider is None:
        return None
    return provider.operations.get(str(operation or "").strip())


def call_messaging_operation(
    platform: str, operation: str, *args: Any, **kwargs: Any
) -> Any:
    fn = messaging_operation(platform, operation)
    if fn is None:
        raise RuntimeError(
            f"messaging provider operation is unavailable: {platform}.{operation}"
        )
    return fn(*args, **kwargs)


def resolve_messaging_target(
    secrets: SecretStore, target: str
) -> tuple[str, str, str | None]:
    """Resolve an explicit `platform:chat[:thread]` target or a provider-owned bare target."""
    raw = str(target or "").strip()
    try:
        platform, chat_id, thread_id = parse_target(raw)
    except ValueError as parse_error:
        with _LOCK:
            providers = tuple(sorted(_PROVIDERS.items()))
        matches: list[tuple[str, str, str | None]] = []
        for platform, provider in providers:
            if provider.parse_bare_target is None:
                continue
            parsed = provider.parse_bare_target(secrets, raw)
            if parsed is not None:
                chat_id, thread_id = parsed
                matches.append((platform, chat_id, thread_id))
        if not matches:
            raise parse_error
        if len(matches) > 1:
            raise ValueError("target matches more than one messaging provider")
        return matches[0]

    if messaging_provider(platform) is None:
        raise ValueError(f"unknown messaging platform: {platform}")
    return platform, chat_id, thread_id


def make_messaging_adapter(
    platform: str, secrets: SecretStore
) -> BasePlatformAdapter | None:
    provider = messaging_provider(platform)
    if provider is None or provider.make_adapter is None:
        return None
    return provider.make_adapter(secrets)


async def reject_messaging_interaction(
    event: InteractionEvent,
    text: str,
) -> None:
    provider = messaging_provider(event.platform)
    if provider is None or provider.reject_interaction is None:
        return
    await provider.reject_interaction(event, text)


__all__ = [
    "AdapterFactory",
    "BareTargetParser",
    "FileSender",
    "InteractionRejector",
    "MessageSender",
    "MessagingProvider",
    "ProviderOperation",
    "SettingsLoader",
    "call_messaging_operation",
    "clear_messaging_providers",
    "make_messaging_adapter",
    "messaging_operation",
    "messaging_provider",
    "messaging_provider_settings",
    "register_messaging_provider",
    "registered_messaging_providers",
    "reject_messaging_interaction",
    "resolve_messaging_target",
    "unregister_messaging_provider",
]
