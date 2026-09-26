from __future__ import annotations

import pytest

from integrations.connectors.config import ConnectorSettings
from integrations.connectors.gateway import Gateway
from integrations.connectors import tool_defs


def test_gateway_requires_injected_secret_source_when_loading_settings() -> None:
    with pytest.raises(
        ValueError,
        match="secrets is required when connector settings are not supplied",
    ):
        Gateway()


def test_gateway_allows_explicit_settings_without_secret_source() -> None:
    gateway = Gateway(
        settings={
            "fake": ConnectorSettings(
                platform="fake",
                enabled=True,
                allow_all=True,
            )
        }
    )

    assert gateway.secrets is None
    assert gateway.settings["fake"].enabled is True


def test_legacy_secret_backed_tool_enablement_authority_is_removed() -> None:
    for name in (
        "load_tool_settings",
        "patch_tool_settings",
        "tool_enabled",
        "active_tool_defs",
        "tool_dicts",
    ):
        assert not hasattr(tool_defs, name)
