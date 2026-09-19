"""Foundation messaging tools.

The tools own approval metadata and local file-access boundaries. Platform credentials,
target aliases, identity decoration and transport calls belong to registered messaging
providers and never become Foundation policy.
"""

from __future__ import annotations

import importlib
from pathlib import Path
from typing import Any, Callable

from integrations.tools import metadata as ai
from integrations.tools.metadata import attach_tool_metadata
from packages.credential_store import CredentialStore as SecretStore
from integrations.connectors.messaging_providers import (
    messaging_provider,
    resolve_messaging_target,
)

_MESSAGE_SCHEMA = {
    "type": "function",
    "function": {
        "name": "send_message",
        "description": (
            "Send a message through an installed messaging provider. `target` is normally "
            "the reply handle from an inbound message (`platform:chat_id[:thread]`); an "
            "installed provider may also support its own human-friendly target aliases."
        ),
        "parameters": {
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "Destination handle returned by an inbound message or understood by an installed provider.",
                },
                "text": {"type": "string", "description": "The message text to send."},
            },
            "required": ["target", "text"],
        },
    },
}


def build_send_message(secrets: SecretStore) -> Callable[..., Any]:
    """Build the approval-gated outbound messaging tool."""

    def send_message(target: str, text: str) -> dict[str, Any]:
        try:
            platform, chat_id, thread_id = resolve_messaging_target(secrets, target)
        except ValueError as exc:
            return {"error": str(exc)}
        provider = messaging_provider(platform)
        if provider is None:
            return {"error": f"messaging provider is not installed: {platform}"}
        result = provider.send(secrets, chat_id, text, thread_id)
        if result.ok:
            return {"ok": True, "message_id": result.message_id, "target": target}
        return {"error": result.error or "send failed"}

    send_message.__name__ = "send_message"
    send_message.__doc__ = _MESSAGE_SCHEMA["function"]["description"]
    attach_tool_metadata(
        send_message,
        schema=_MESSAGE_SCHEMA,
        metadata=ai.ToolMetadata(
            name="send_message",
            category="messaging",
            risk_level="medium",
            capabilities=["messaging"],
            requires_approval=True,
        ),
    )
    return send_message


_FILE_SCHEMA = {
    "type": "function",
    "function": {
        "name": "send_file",
        "description": (
            "Upload a file from the session workspace through an installed messaging "
            "provider. This is a distinct permission from send_message and always remains "
            "subject to Foundation approval."
        ),
        "parameters": {
            "type": "object",
            "properties": {
                "target": {"type": "string"},
                "path": {
                    "type": "string",
                    "description": "Workspace-relative path, or an absolute path inside an allowed folder.",
                },
                "title": {"type": "string"},
                "comment": {"type": "string"},
                "should_render_image": {
                    "type": "boolean",
                    "description": "HTML only: render the page and send a PNG preview.",
                },
            },
            "required": ["target", "path"],
        },
    },
}

_MAX_FILE_BYTES = 50 * 1024 * 1024


def _resolve_within(path: str, bases: list[Path]) -> Path | None:
    candidates: list[Path] = []
    p = Path(path).expanduser()
    if p.is_absolute():
        candidates.append(p)
    else:
        candidates.extend(base / p for base in bases)
    for cand in candidates:
        try:
            resolved = cand.resolve(strict=True)
        except OSError:
            continue
        for base in bases:
            try:
                resolved.relative_to(base.resolve())
                return resolved
            except ValueError:
                continue
    return None


def _render_html_png(path: Path) -> bytes:
    """Compatibility renderer; concrete browser automation remains separately pluggable."""
    sync_playwright = importlib.import_module("playwright.sync_api").sync_playwright
    with sync_playwright() as pw:
        browser = pw.chromium.launch()
        try:
            page = browser.new_page(viewport={"width": 1280, "height": 800})
            page.goto(path.as_uri())
            page.wait_for_timeout(500)
            return page.screenshot(full_page=False)
        finally:
            browser.close()


def build_send_file(
    secrets: SecretStore,
    *,
    workspace: Path | None = None,
    roots: list | None = None,
    render_html: Callable[[Path], bytes] | None = None,
) -> Callable[..., Any]:
    """Build the approval-gated file-send tool with Foundation path containment."""
    render_html = render_html or _render_html_png
    bases = [Path(r.path) for r in (roots or []) if getattr(r, "path", None)]
    if workspace is not None:
        bases.append(Path(workspace))

    def send_file(
        target: str,
        path: str,
        title: str | None = None,
        comment: str | None = None,
        should_render_image: bool = False,
    ) -> dict[str, Any]:
        try:
            platform, chat_id, thread_id = resolve_messaging_target(secrets, target)
        except ValueError as exc:
            return {"error": str(exc)}
        provider = messaging_provider(platform)
        if provider is None:
            return {"error": f"messaging provider is not installed: {platform}"}
        if provider.send_file is None:
            return {"error": f"file sending is not supported on {platform}"}
        if not bases:
            return {"error": "no workspace folders available to read from"}
        resolved = _resolve_within(path, bases)
        if resolved is None or not resolved.is_file():
            return {
                "error": "path is outside the folders this session can access (or missing)"
            }
        if should_render_image:
            if resolved.suffix.lower() not in (".html", ".htm"):
                return {"error": "should_render_image only applies to .html files"}
            try:
                data = render_html(resolved)
            except Exception as exc:
                return {"error": f"could not render the page: {exc}"}
            filename = resolved.stem + ".png"
        else:
            if resolved.stat().st_size > _MAX_FILE_BYTES:
                return {"error": "file is larger than 50 MB"}
            data = resolved.read_bytes()
            filename = resolved.name
        result = provider.send_file(
            secrets,
            chat_id,
            thread_id,
            filename,
            data,
            title,
            comment,
        )
        if result.ok:
            return {
                "ok": True,
                "file_id": result.message_id,
                "target": target,
                "filename": filename,
            }
        return {"error": result.error or "file send failed"}

    send_file.__name__ = "send_file"
    send_file.__doc__ = _FILE_SCHEMA["function"]["description"]
    attach_tool_metadata(
        send_file,
        schema=_FILE_SCHEMA,
        metadata=ai.ToolMetadata(
            name="send_file",
            category="messaging",
            risk_level="medium",
            capabilities=["messaging", "files"],
            requires_approval=True,
        ),
    )
    return send_file


__all__ = ["build_send_file", "build_send_message"]
