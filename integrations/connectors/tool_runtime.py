"""Runtime support for connector tools."""

from __future__ import annotations

import re
from html.parser import HTMLParser
from typing import Any, Callable

from integrations.connectors.tool_defs import approval_for_tool, register_runtime_tool
from integrations.web.guard import request_checked
from integrations.tools import metadata as ai
from integrations.tools.metadata import attach_tool_metadata


def build_tool_metadata(
    name: str, *, is_approval_required: bool = False, capabilities: list[str] | None = None
):
    return ai.ToolMetadata(
        name=name,
        category="connector",
        risk_level="medium" if is_approval_required else "low",
        capabilities=capabilities or ["integration"],
        requires_approval=is_approval_required,
    )


def build_tool_schema(
    name: str, description: str, properties: dict[str, Any], required: list[str]
) -> dict[str, Any]:
    return {
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": required,
            },
        },
    }


def attach_connector_tool(
    tool: Callable[..., Any],
    schema: dict[str, Any],
    *,
    is_approval_required: bool | None = None,
    capabilities: list[str] | None = None,
):
    name = schema["function"]["name"]
    description = str(schema["function"].get("description") or "")
    register_runtime_tool(
        name,
        capabilities=capabilities,
        description=description,
        is_approval_required=is_approval_required,
    )
    is_default_approval = True if is_approval_required is None else is_approval_required
    resolved_approval = approval_for_tool(name, is_default_approval=is_default_approval)
    attach_tool_metadata(
        tool,
        schema=schema,
        metadata=build_tool_metadata(name, is_approval_required=resolved_approval, capabilities=capabilities),
    )
    tool.__doc__ = description
    return tool


def execute_http_request(
    method: str,
    url: str,
    *,
    headers=None,
    params=None,
    json=None,
    auth=None,
    should_check_address: bool = False,
) -> dict[str, Any]:
    """HTTP for connectors, with address checks for model-supplied URLs."""
    try:
        import httpx

        with httpx.Client(timeout=30.0, follow_redirects=not should_check_address) as client:
            if should_check_address:
                try:
                    response = request_checked(
                        client,
                        method,
                        url,
                        headers=headers,
                        params=params,
                        json=json,
                        auth=auth,
                        max_redirects=0,
                    )
                except (PermissionError, RuntimeError) as exc:
                    return {"error": str(exc)}
            else:
                response = client.request(
                    method, url, headers=headers, params=params, json=json, auth=auth
                )
            content_type = response.headers.get("content-type", "")
            response_data: Any = response.json() if "json" in content_type.lower() else response.text
            if response.status_code >= 400:
                return {"error": f"HTTP {response.status_code}", "details": response_data}
            return {"ok": True, "data": response_data}
    except Exception as exc:
        return {"error": str(exc)}


class _TextExtractor(HTMLParser):
    _SKIP = {"script", "style", "noscript", "svg", "head"}

    def __init__(self) -> None:
        super().__init__()
        self._skip = 0
        self.parts: list[str] = []

    def handle_starttag(self, tag: str, attrs: Any) -> None:
        if tag in self._SKIP:
            self._skip += 1

    def handle_endtag(self, tag: str) -> None:
        if tag in self._SKIP and self._skip:
            self._skip -= 1

    def handle_data(self, data: str) -> None:
        if not self._skip:
            text = data.strip()
            if text:
                self.parts.append(text)


def extract_html_text(html: str) -> str:
    parser = _TextExtractor()
    try:
        parser.feed(html)
    except Exception:
        pass
    return re.sub(r"\n{3,}", "\n\n", "\n".join(parser.parts))


def get_time_ms() -> int:
    from time import time

    return int(time() * 1000)


def resolve_limit(value: Any, default: int = 10, ceiling: int = 20) -> int:
    return max(1, min(int(value or default), ceiling))
