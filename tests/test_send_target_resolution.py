"""Connector approval, baseline and messaging target contract tests."""

from integrations.connectors.base import SendResult
from integrations.connectors.messaging_providers import (
    MessagingProvider,
    clear_messaging_providers,
    register_messaging_provider,
)
from integrations.connectors.tool_defs import TOOL_DEFS, approval_for_tool
from integrations.connectors.tools import build_send_message
from packages.credential_store import CredentialStore as SecretStore


def test_registry_kinds_are_exhaustive_and_drive_approval():
    for d in TOOL_DEFS:
        assert d.kind in ("read", "write"), f"{d.name} has kind {d.kind!r}"
        assert approval_for_tool(d.name) is (d.kind != "read")
    assert approval_for_tool("mcp_mystery_tool", is_default_approval=True) is True
    assert approval_for_tool("mcp_mystery_tool", is_default_approval=False) is False


def test_standalone_foundation_exposes_only_baseline_tools(tmp_path):
    from integrations.connectors.connector_tools import build_connector_tools

    tools = {
        t.__name__: t
        for t in build_connector_tools(SecretStore(tmp_path / "secrets.json"))
    }
    assert {
        "email_list_folders",
        "email_search",
        "email_read",
        "email_download_attachment",
        "email_send",
        "browser_read_url",
    } <= set(tools)
    assert tools["email_search"].__delta_tool_metadata__.requires_approval is False
    assert tools["email_send"].__delta_tool_metadata__.requires_approval is True
    assert tools["browser_read_url"].__delta_tool_metadata__.requires_approval is True
    assert "gmail_search_messages" not in tools
    assert "hubspot_search" not in tools
    assert "github_search" not in tools
    assert "github_clone" not in tools
    assert "github_pull" not in tools


def test_browser_automation_requires_explicit_effect_metadata():
    from integrations.connectors.browser_automation import (
        attach_browser_tool,
        browser_tool_schema,
    )

    def tool(name: str, kind: str):
        def fn():
            return {"ok": True}

        fn.__name__ = name
        return attach_browser_tool(
            fn,
            browser_tool_schema(name, name, {}, []),
            is_approval_required=True,
            capabilities=["browser", kind],
        )

    tools = {
        "browser_snapshot": tool("browser_snapshot", "read"),
        "browser_open_url": tool("browser_open_url", "write"),
        "browser_click": tool("browser_click", "write"),
        "browser_type": tool("browser_type", "write"),
    }
    assert tools["browser_snapshot"].__delta_tool_metadata__.requires_approval is False
    assert tools["browser_open_url"].__delta_tool_metadata__.requires_approval is True
    assert tools["browser_click"].__delta_tool_metadata__.requires_approval is True
    assert tools["browser_type"].__delta_tool_metadata__.requires_approval is True


def test_messaging_provider_owns_human_friendly_target_resolution(tmp_path):
    clear_messaging_providers()
    record: list[dict] = []

    def send(_secrets, chat_id, text, thread_id):
        record.append({"chat_id": chat_id, "text": text, "thread_id": thread_id})
        return SendResult(True, message_id="1")

    def parse_bare(_secrets, raw):
        return ("ROOM1", None) if raw == "#team" else None

    register_messaging_provider(
        MessagingProvider(platform="fake", send=send, parse_bare_target=parse_bare)
    )
    try:
        tool = build_send_message(SecretStore(tmp_path / "secrets.json"))
        assert tool("#team", "Hi")["ok"] is True
        assert record == [{"chat_id": "ROOM1", "text": "Hi", "thread_id": None}]
    finally:
        clear_messaging_providers()


def test_explicit_target_dispatches_without_vendor_logic(tmp_path):
    clear_messaging_providers()
    record: list[tuple[str, str | None]] = []

    def send(_secrets, chat_id, _text, thread_id):
        record.append((chat_id, thread_id))
        return SendResult(True, message_id="2")

    register_messaging_provider(MessagingProvider(platform="fake", send=send))
    try:
        tool = build_send_message(SecretStore(tmp_path / "secrets.json"))
        assert tool("fake:ROOM2:thread-3", "Hi")["ok"] is True
        assert record == [("ROOM2", "thread-3")]
        assert "unknown messaging platform" in tool("missing:ROOM", "Hi")["error"]
    finally:
        clear_messaging_providers()
