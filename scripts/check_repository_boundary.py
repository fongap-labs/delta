from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]

FORBIDDEN_TOP_LEVEL = {
    "suite",
    "commercial",
    "enterprise",
    "pro",
    "delta-suite",
    "delta-commercial",
}

FORBIDDEN_MIGRATED_PATHS = {
    pathlib.Path("apps/desktop/src/features/connectors/components/GmailDetail.tsx"),
    pathlib.Path("apps/desktop/src/features/connectors/components/CalendarDetail.tsx"),
    pathlib.Path("apps/desktop/src/features/connectors/components/HubSpotDetail.tsx"),
    pathlib.Path("apps/desktop/src/features/connectors/components/SlackDetail.tsx"),
    pathlib.Path("apps/desktop/src/features/connectors/components/GithubDetail.tsx"),
    pathlib.Path("integrations/connectors/integration_auth.py"),
    pathlib.Path("integrations/connectors/integration_github.py"),
    pathlib.Path("integrations/connectors/github_installs.py"),
    pathlib.Path("integrations/connectors/slack_addr.py"),
    pathlib.Path("integrations/managed/github_app.py"),
}

SOURCE_SUFFIXES = {
    ".rs", ".py", ".ts", ".tsx", ".js", ".mjs", ".json", ".toml",
    ".yaml", ".yml", ".sh", ".ps1",
}

FORBIDDEN_PATTERNS = {
    "Suite repository reference": re.compile(
        r"fongap/delta-suite|github\.com/fongap/delta-suite", re.I
    ),
    "retired repository identity": re.compile(
        r"fongap/delta-commercial|github\.com/fongap/delta-commercial|\bdelta-commercial\b", re.I
    ),
    "Suite/Core dependency": re.compile(
        r"private-sdk|suite-core|commercial-core|enterprise-core|pro-core|delta-suite-app|delta-commercial-app|delta-pro-app",
        re.I,
    ),
    "Suite git dependency": re.compile(
        r"git\s*=\s*[\"'][^\"']*delta-(?:suite|commercial)[^\"']*[\"']", re.I
    ),
}

MIGRATED_IMPLEMENTATION_GUARDS: dict[pathlib.Path, tuple[tuple[str, re.Pattern[str]], ...]] = {
    pathlib.Path("integrations/connectors/browser_automation.py"): (
        ("Playwright implementation", re.compile(r"playwright\.sync_api", re.I)),
        ("browser process implementation", re.compile(r"chromium\.launch\s*\(", re.I)),
        ("browser worker thread implementation", re.compile(r"ThreadPoolExecutor", re.I)),
        ("private browser controller", re.compile(r"class\s+_BrowserController\b")),
    ),
    pathlib.Path("integrations/connectors/descriptors.py"): (
        ("vendor validation endpoint", re.compile(r"api\.(?:slack|notion|figma|canva)|googleapis\.com|graph\.microsoft\.com|hubapi\.com", re.I)),
        ("first-party vendor descriptor", re.compile(r"ConnectorDescriptor\(\s*[\"'](?:gmail|slack|notion|outlook|hubspot|canva|figma|github)[\"']", re.I)),
        ("managed product catalog", re.compile(r"CASA|managed sign-in|managed broker", re.I)),
    ),
    pathlib.Path("integrations/connectors/catalog_copy.py"): (
        ("first-party catalog prose", re.compile(r"\b(?:Gmail|Slack|Notion|Outlook|HubSpot|Canva|Figma|QuickBooks)\b")),
        ("vendor-specific access catalog", re.compile(r"\"(?:gmail|slack|notion|outlook|hubspot|canva|figma|quickbooks)\"\s*:", re.I)),
    ),
    pathlib.Path("integrations/connectors/gmail_accounts.py"): (
        ("Gmail profile storage implementation", re.compile(r"gmail:account:", re.I)),
        ("Gmail SecretStore mutation", re.compile(r"secrets\.(?:put|delete|status)\s*\(", re.I)),
        ("Gmail sender-filter algorithm", re.compile(r"rule\.startswith\(\s*[\"']@[\"']\s*\)", re.I)),
    ),
    pathlib.Path("integrations/connectors/gcal_accounts.py"): (
        ("Google Calendar profile storage implementation", re.compile(r"google_calendar:account:", re.I)),
        ("Google Calendar SecretStore mutation", re.compile(r"secrets\.(?:put|delete|status)\s*\(", re.I)),
    ),
    pathlib.Path("integrations/connectors/hubspot_portals.py"): (
        ("HubSpot portal storage implementation", re.compile(r"hubspot:portal:", re.I)),
        ("HubSpot SecretStore mutation", re.compile(r"secrets\.(?:put|delete|status)\s*\(", re.I)),
        ("HubSpot field filtering algorithm", re.compile(r"properties|hidden_fields.*lower\(\)", re.I)),
    ),
    pathlib.Path("integrations/connectors/tool_defs.py"): (
        ("Gmail first-party tool catalog", re.compile(r"gmail_(?:search_messages|get_message|send_email)", re.I)),
        ("Google Calendar first-party tool catalog", re.compile(r"gcal_(?:list_events|free_busy|create_event|update_event|delete_event)", re.I)),
        ("HubSpot first-party tool catalog", re.compile(r"hubspot_(?:search|get_object|create_contact|update_object|log_note|create_task)", re.I)),
        ("first-party hosted MCP allowlist", re.compile(r"mcp__(?:jira|monday|asana)__", re.I)),
    ),
    pathlib.Path("integrations/connectors/connector_tools.py"): (
        ("first-party SaaS network implementation", re.compile(r"gmail\.googleapis\.com|api\.hubapi\.com|graph\.microsoft\.com|api\.notion\.com|api\.stripe\.com|api\.canva\.com|api\.figma\.com", re.I)),
        ("coding-only Git execution", re.compile(r"github_(?:clone|pull)\b|\bgit\s+clone\b", re.I)),
        ("first-party execution runtime dependency", re.compile(r"integration_(?:github|auth)\s+import", re.I)),
    ),
    pathlib.Path("integrations/connectors/adapters.py"): (
        ("Slack SDK adapter", re.compile(r"slack_(?:bolt|sdk)|AsyncSocketModeHandler|AsyncWebClient", re.I)),
        ("Telegram SDK adapter", re.compile(r"telegram\.ext|Application\.builder", re.I)),
        ("concrete first-party adapter", re.compile(r"class\s+(?:Slack|Telegram)Adapter\b")),
    ),
    pathlib.Path("integrations/connectors/senders.py"): (
        ("Slack transport endpoint", re.compile(r"slack\.com/api|chat\.postMessage|files\.getUploadURLExternal", re.I)),
        ("Telegram transport endpoint", re.compile(r"api\.telegram\.org", re.I)),
    ),
    pathlib.Path("integrations/connectors/slack_directory.py"): (
        ("Slack roster implementation", re.compile(r"conversations\.list|users\.list|SLACK_API_URL|httpx\.(?:get|post)", re.I)),
    ),
    pathlib.Path("integrations/connectors/slack_sender.py"): (
        ("Slack identity lookup implementation", re.compile(r"users\.info|SLACK_API_URL|httpx\.get", re.I)),
    ),
    pathlib.Path("integrations/connectors/gateway.py"): (
        ("vendor interaction endpoint", re.compile(r"hooks\.slack|response_type.*ephemeral|event\.platform\s*!=\s*[\"']slack", re.I)),
    ),
    pathlib.Path("integrations/connectors/config.py"): (
        ("hard-coded messaging platform catalog", re.compile(r"PLATFORMS\s*=|slack:team:|github:install", re.I)),
    ),
    pathlib.Path("integrations/managed/oauth.py"): (
        ("managed OAuth network implementation", re.compile(r"\b(?:httpx|requests|aiohttp|websockets)\b|urllib\.request|https?://", re.I)),
        ("managed OAuth concrete broker implementation", re.compile(r"class\s+(?!NullOAuthBroker\b)\w+OAuthBroker\b", re.I)),
    ),
    pathlib.Path("integrations/managed/relay.py"): (
        ("managed relay network implementation", re.compile(r"\b(?:httpx|requests|aiohttp|websockets)\b|urllib\.request|wss?://", re.I)),
        ("managed relay concrete transport implementation", re.compile(r"class\s+(?!NullRelayTransport\b)\w+RelayTransport\b", re.I)),
    ),
    pathlib.Path("integrations/managed/identity.py"): (
        ("managed identity verification implementation", re.compile(r"\b(?:jwt|jose|authlib|httpx|requests|aiohttp|websockets)\b|urllib\.request|https?://", re.I)),
    ),
    pathlib.Path("crates/delta-core/src/application.rs"): (
        ("compiled first-party connector catalog", re.compile(r'name:\s*"(?:telegram|slack|gmail|google_calendar|browser|github|outlook|jira|monday|confluence|zendesk|linear|gitlab|discord|stripe|asana|hubspot|dropbox|box|whatsapp|quickbooks|datadog|salesforce|docusign|clickup|google_drive|canva|figma|descript|clay|close|notion|attio|posthog|mixpanel|amplitude|apollo|hunter|pagerduty)"\.to_string\(\)', re.I)),
        ("static connector catalog authority", re.compile(r"const\s+CONNECTORS\s*:", re.I)),
        ("vendor-specific credential exception", re.compile(r'name\s*!=\s*"browser"', re.I)),
    ),
    pathlib.Path("apps/desktop/src/api.ts"): (
        ("vendor-specific connector DTO type", re.compile(r"\b(?:SlackWorkspace|SlackMember|SlackChannelEntry|SlackStatus|GithubInstallation|GithubStatus|HubSpotPortal|GmailAccount|GmailFilters)\b")),
        ("vendor-specific connector API wrapper", re.compile(r"export\s+async\s+function\s+(?:getSlack|addSlack|removeSlack|disconnectSlack|disconnectGmail|setGmail|disconnectGcal|setGcal|getGithub|disconnectGithub|disconnectHubSpot|setHubSpot)", re.I)),
        ("vendor-specific connector DTO field", re.compile(r"export\s+interface\s+Connector\s*\{.*?\b(?:workspaces|portals|installations|filters|hidden_fields)\?\s*:", re.I | re.S)),
    ),
    pathlib.Path("apps/desktop/src/features/connectors/components/ConnectorsSection.tsx"): (
        ("vendor-name detail routing", re.compile(r"(?:slack|gmail|google_calendar|hubspot|github)\s*:\s*\(p\)\s*=>", re.I)),
        ("vendor-name page lookup", re.compile(r"DETAIL_PAGES\s*\[\s*[\"'](?:slack|gmail|google_calendar|hubspot|github)[\"']\s*\]", re.I)),
        ("vendor-specific health polling", re.compile(r"\b(?:getSlackStatus|SlackStatus)\b")),
    ),
    pathlib.Path("apps/desktop/src/features/connectors/components/ConnectorsList.tsx"): (
        ("vendor-specific list health type", re.compile(r"\bSlackStatus\b")),
    ),
}

SKIP_DIRS = {".git", "node_modules", "target", ".venv", "dist", "build", "docs"}
SKIP_FILES = {pathlib.Path(__file__).resolve()}


def main() -> int:
    errors: list[str] = []

    for item in ROOT.iterdir():
        if item.is_dir() and item.name in FORBIDDEN_TOP_LEVEL:
            errors.append(f"forbidden Suite-only top-level directory: {item.name}")

    for rel in FORBIDDEN_MIGRATED_PATHS:
        if (ROOT / rel).exists():
            errors.append(f"migrated commercial UI path returned: {rel}")

    for path in ROOT.rglob("*"):
        if path in SKIP_FILES or not path.is_file():
            continue
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        if path.name == "README.md":
            continue
        if path.suffix.lower() not in SOURCE_SUFFIXES and path.name not in {"Cargo.toml", "package.json", "pyproject.toml"}:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        rel = path.relative_to(ROOT)
        for label, pattern in FORBIDDEN_PATTERNS.items():
            if pattern.search(text):
                errors.append(f"{rel}: {label}")

    for rel, guards in MIGRATED_IMPLEMENTATION_GUARDS.items():
        path = ROOT / rel
        if not path.exists():
            errors.append(f"missing public contract after migration: {rel}")
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            errors.append(f"public contract is not UTF-8 text: {rel}")
            continue
        for label, pattern in guards:
            if pattern.search(text):
                errors.append(f"{rel}: migrated commercial implementation returned ({label})")

    if errors:
        print("Delta Foundation repository boundary violations:")
        for error in sorted(set(errors)):
            print(f"  - {error}")
        return 1

    print("Delta Foundation repository boundary: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
