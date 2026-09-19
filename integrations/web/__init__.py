"""Web search — a keyless DuckDuckGo default + configurable third-party providers."""

from __future__ import annotations

from integrations.web.fetch import build_fetch_tool
from integrations.web.providers import (
    BraveProvider,
    DuckDuckGoProvider,
    SearchResult,
    TavilyProvider,
    WebSearchProvider,
    build_provider,
    provider_names,
)
from integrations.web.tool import build_search_tool, resolve_provider

__all__ = [
    "BraveProvider",
    "DuckDuckGoProvider",
    "SearchResult",
    "TavilyProvider",
    "WebSearchProvider",
    "build_provider",
    "build_fetch_tool",
    "build_search_tool",
    "provider_names",
    "resolve_provider",
]
