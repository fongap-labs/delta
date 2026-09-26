"""Foundation messaging gateway.

The Gateway owns inbound authorization, routing and lifecycle. Platform transports are
optional messaging providers; Foundation never embeds vendor HTTP endpoints or SDK logic.
"""

from __future__ import annotations

import logging
from collections import OrderedDict
from typing import Callable

from integrations.connectors.secret_source import SecretSource
from integrations.connectors.base import (
    BasePlatformAdapter,
    InteractionEvent,
    MessageEvent,
    MessageHandler,
    SendResult,
    SessionSource,
    parse_target,
)
from integrations.connectors.config import ConnectorSettings, is_authorized, load_settings
from integrations.connectors.messaging_providers import (
    make_messaging_adapter,
    reject_messaging_interaction,
)

logger = logging.getLogger("integrations.connectors")

_RECENT_CAP = 20


class Gateway:
    def __init__(
        self,
        *,
        secrets: SecretSource | None = None,
        settings: dict[str, ConnectorSettings] | None = None,
        handler: MessageHandler | None = None,
        reply_resolver: Callable[[MessageEvent], bool] | None = None,
        interaction_handler: Callable | None = None,
        on_unauthorized: Callable | None = None,
    ) -> None:
        self.secrets = secrets
        if settings is None:
            if secrets is None:
                raise ValueError("secrets is required when connector settings are not supplied")
            self.settings = load_settings(secrets)
        else:
            self.settings = settings
        self._handler = handler
        self._reply_resolver = reply_resolver
        self._interaction_handler = interaction_handler
        self._on_unauthorized = on_unauthorized
        self._adapters: dict[str, BasePlatformAdapter] = {}
        self._recent: OrderedDict[tuple[str, str, str], dict] = OrderedDict()

    def set_handler(self, handler: MessageHandler) -> None:
        self._handler = handler

    def set_reply_resolver(
        self, resolver: Callable[[MessageEvent], bool] | None
    ) -> None:
        self._reply_resolver = resolver

    def register(self, adapter: BasePlatformAdapter) -> None:
        adapter.set_message_handler(self._on_inbound)
        if self._interaction_handler is not None:
            adapter.set_interaction_handler(self._on_interaction)
        self._adapters[adapter.platform] = adapter

    async def _on_interaction(self, event: InteractionEvent) -> None:
        source = SessionSource(
            platform=event.platform,
            chat_id=event.chat_id,
            user_id=event.user_id,
            user_name=event.user_name,
            chat_type="channel",
            team_id=event.team_id,
        )
        settings = self.settings.get(event.platform)
        if settings is None or not is_authorized(settings, source):
            logger.info("rejecting unauthorized interaction from %s", source.label())
            await self.reject_interaction(event)
            return
        if self._interaction_handler is not None:
            await self._interaction_handler(event)

    async def reject_interaction(
        self,
        event: InteractionEvent,
        text: str = "Only a designated approval owner can respond to this request.",
    ) -> None:
        """Ask the installed provider for best-effort rejection feedback."""
        await reject_messaging_interaction(event, text)

    async def _on_inbound(self, event: MessageEvent) -> None:
        self._record_recent(event)
        settings = self.settings.get(event.source.platform)
        if settings is None or not is_authorized(settings, event.source):
            logger.info("parking unauthorized inbound from %s", event.source.label())
            if self._on_unauthorized is not None:
                try:
                    await self._on_unauthorized(event)
                except Exception:
                    logger.exception("parking unauthorized inbound failed")
            return
        if self._reply_resolver is not None:
            try:
                if self._reply_resolver(event):
                    return
            except Exception:
                logger.exception("inbox reply resolver failed")
        if self._handler is not None:
            await self._handler(event)

    def _record_recent(self, event: MessageEvent) -> None:
        s = event.source
        if not s.user_id:
            return
        key = (s.platform, s.team_id or "", s.user_id)
        self._recent.pop(key, None)
        self._recent[key] = {
            "platform": s.platform,
            "user_id": s.user_id,
            "user_name": s.user_name,
            "chat_id": s.chat_id,
            "chat_type": s.chat_type,
            "target": s.target,
            "team_id": s.team_id,
        }
        while len(self._recent) > _RECENT_CAP:
            self._recent.popitem(last=False)

    def recent_senders(self, platform: str | None = None) -> list[dict]:
        items = list(self._recent.values())[::-1]
        return [e for e in items if platform is None or e["platform"] == platform]

    async def start(self) -> list[str]:
        """Connect every enabled platform through registered/provider-built adapters."""
        live: list[str] = []
        for platform, settings in self.settings.items():
            if not settings.enabled:
                continue
            adapter = self._adapters.get(platform)
            if adapter is None:
                if self.secrets is None:
                    logger.warning(
                        "messaging provider %s requires an injected secret source",
                        platform,
                    )
                    continue
                adapter = make_messaging_adapter(platform, self.secrets)
                if adapter is not None:
                    self.register(adapter)
            if adapter is None:
                continue
            try:
                if await adapter.connect():
                    live.append(platform)
            except Exception:
                logger.exception("failed to connect %s adapter", platform)
        return live

    async def stop(self) -> None:
        for adapter in self._adapters.values():
            try:
                await adapter.disconnect()
            except Exception:
                logger.exception("error disconnecting %s adapter", adapter.platform)

    async def deliver(self, target: str, text: str) -> SendResult:
        platform, chat_id, thread_id = parse_target(target)
        adapter = self._adapters.get(platform)
        if adapter is None:
            return SendResult(False, error=f"no adapter for {platform}")
        return await adapter.send(chat_id, text, thread_id=thread_id)

    async def deliver_interactive(self, target: str, text: str, buttons) -> SendResult:
        platform, chat_id, thread_id = parse_target(target)
        adapter = self._adapters.get(platform)
        if adapter is None:
            return SendResult(False, error=f"no adapter for {platform}")
        return await adapter.send_interactive(
            chat_id, text, buttons, thread_id=thread_id
        )

    async def update_message(
        self, platform: str, chat_id: str, message_id: str, text: str
    ) -> None:
        adapter = self._adapters.get(platform)
        fn = getattr(adapter, "update_message", None)
        if fn is not None:
            await fn(chat_id, message_id, text)

    def status(self) -> list[dict]:
        out = []
        for platform, settings in self.settings.items():
            out.append(
                {
                    "platform": platform,
                    "enabled": settings.enabled,
                    "connected": platform in self._adapters,
                    "allow_all": settings.allow_all,
                    "allowed_users": len(settings.allowed_users),
                }
            )
        return out
