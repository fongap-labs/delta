from __future__ import annotations

from integrations.connectors.tool_defs import ConnectorToolDef, register_tool_def
from integrations.connectors.tool_factories import (
    clear_tool_factories,
    make_extension_tools,
    register_tool_factory,
    registered_tool_factories,
)
from packages.credential_store import CredentialStore as SecretStore


def setup_function():
    clear_tool_factories()


def teardown_function():
    clear_tool_factories()


def _named(name: str):
    def tool():
        return {"ok": True}

    tool.__name__ = name
    return tool


def test_extension_factory_registration_and_dedup(tmp_path):
    one = _named("example_read")
    duplicate = _named("example_read")

    def factory(secrets, **kwargs):
        return [one, duplicate]

    register_tool_factory("example", factory)
    tools = make_extension_tools(SecretStore(tmp_path / "secrets.json"))

    assert registered_tool_factories() == ("example",)
    assert [tool.__name__ for tool in tools] == ["example_read"]


def test_duplicate_factory_requires_explicit_replace():
    register_tool_factory("example", lambda secrets, **kwargs: [])
    try:
        register_tool_factory("example", lambda secrets, **kwargs: [])
    except ValueError as exc:
        assert "already registered" in str(exc)
    else:
        raise AssertionError("duplicate factory registration must fail")

    register_tool_factory("example", lambda secrets, **kwargs: [], should_replace=True)


def test_foundation_baseline_wins_extension_name_collision(tmp_path):
    from integrations.connectors.connector_tools import build_connector_tools

    collision = _named("email_search")
    extra = _named("example_read")
    register_tool_def(
        ConnectorToolDef(
            connector="example",
            name="example_read",
            label="Example read",
            kind="read",
            description="Read extension data.",
        ),
        should_replace=True,
    )

    def factory(secrets, **kwargs):
        return [collision, extra]

    register_tool_factory("example", factory)
    tools = build_connector_tools(SecretStore(tmp_path / "secrets.json"))
    names = [tool.__name__ for tool in tools]

    assert names.count("email_search") == 1
    assert "example_read" in names


def test_enabled_connector_filter_applies_after_provider_build(tmp_path):
    from integrations.connectors.connector_tools import build_connector_tools

    extra = _named("example_read")
    register_tool_def(
        ConnectorToolDef(
            connector="example",
            name="example_read",
            label="Example read",
            kind="read",
            description="Read extension data.",
        ),
        should_replace=True,
    )
    register_tool_factory("example", lambda secrets, **kwargs: [extra])

    tools = build_connector_tools(
        SecretStore(tmp_path / "secrets.json"), enabled_connectors={"email"}
    )
    assert "example_read" not in {tool.__name__ for tool in tools}
