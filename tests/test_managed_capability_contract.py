from __future__ import annotations

import asyncio
import importlib.util

import integrations.managed as managed


def test_managed_foundation_exports_are_vendor_agnostic() -> None:
    exported = set(managed.__all__)

    assert {
        "ManagedConfig",
        "ManagedUnavailableError",
        "OAuthBroker",
        "NullOAuthBroker",
        "RelayTransport",
        "NullRelayTransport",
        "ExternalIdentity",
        "ExternalIdentityProvider",
        "NullIdentityProvider",
    } <= exported
    assert not any("github" in name.lower() for name in exported)


def test_retired_github_managed_modules_are_absent() -> None:
    assert importlib.util.find_spec("integrations.managed.github_app") is None
    assert importlib.util.find_spec("integrations.connectors.github_installs") is None


def test_managed_null_defaults_are_offline_safe() -> None:
    config = managed.ManagedConfig()
    assert config.enabled is False
    assert config.base_url == ""
    assert config.device_token == ""
    assert config.relay_ws_url == ""

    async def exercise() -> None:
        oauth = managed.NullOAuthBroker()
        begin = await oauth.begin("example")
        assert begin["ok"] is False

        relay = managed.NullRelayTransport()
        await relay.open()
        assert await relay.recv() is None
        await relay.close()

    asyncio.run(exercise())
