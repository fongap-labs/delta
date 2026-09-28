"""Stable tooling contract for extension-provided connector tools."""

from typing import Any, Callable

from integrations.connectors.tool_runtime import (
    attach_connector_tool,
    build_tool_schema,
    execute_http_request,
    extract_html_text,
    get_time_ms,
    resolve_limit,
)

__all__ = [
    "attach_connector_tool",
    "build_tool_schema",
    "execute_http_request",
    "extract_html_text",
    "get_time_ms",
    "resolve_limit",
]
