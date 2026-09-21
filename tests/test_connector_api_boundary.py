from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
API = ROOT / "apps" / "desktop" / "src" / "api.ts"
DESKTOP = ROOT / "apps" / "desktop" / "src"


def test_connector_api_owns_only_generic_account_shape() -> None:
    text = API.read_text(encoding="utf-8")

    assert "export interface AccountRow" in text
    assert "accounts?: AccountRow[]" in text

    connector_start = text.index("export interface Connector {")
    connector_end = text.index("\n}\n", connector_start)
    connector = text[connector_start:connector_end]

    for legacy_type in (
        "SlackWorkspace",
        "SlackMember",
        "SlackChannelEntry",
        "SlackStatus",
        "GithubInstallation",
        "GithubStatus",
        "HubSpotPortal",
        "GmailAccount",
        "GmailFilters",
    ):
        assert legacy_type not in text

    for legacy_field in (
        "workspaces?:",
        "portals?:",
        "installations?:",
        "filters?:",
        "hidden_fields?:",
    ):
        assert legacy_field not in connector

    for legacy_wrapper in (
        "getSlackDirectory",
        "getSlackChannels",
        "addSlackApprovalOwner",
        "removeSlackApprovalOwner",
        "disconnectSlackWorkspace",
        "disconnectGmailAccount",
        "setGmailDefaultAccount",
        "disconnectGcalAccount",
        "setGcalDefaultAccount",
        "setGmailFilters",
        "getGithubStatus",
        "disconnectGithubInstallation",
        "disconnectHubSpotPortal",
        "setHubSpotDefaultPortal",
        "setHubSpotHiddenFields",
        "getSlackStatus",
    ):
        assert legacy_wrapper not in text


def test_connector_consumers_narrow_capability_payloads_locally() -> None:
    subscriptions = (DESKTOP / "components" / "SubscriptionsChip.tsx").read_text(encoding="utf-8")
    inbox = (DESKTOP / "components" / "InboxConfigure.tsx").read_text(encoding="utf-8")
    listing = (
        DESKTOP / "features" / "connectors" / "components" / "ConnectorsList.tsx"
    ).read_text(encoding="utf-8")

    assert "getSlackChannels" not in subscriptions
    assert "workspaceChannels" in subscriptions
    assert 'ui?.detail === "workspace_chat"' in subscriptions

    assert "slack?.workspaces" not in inbox
    assert 'ui?.detail === "workspace_chat"' in inbox

    assert "c.portals" not in listing
    assert 'ui?.detail === "crm_portals_privacy"' in listing
