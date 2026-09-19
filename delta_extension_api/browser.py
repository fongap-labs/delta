"""Public browser automation extension contract."""

from integrations.connectors.browser_automation import (
    BrowserActionFn,
    BrowserAutomationProvider,
    BrowserStateFn,
    BrowserTool,
    BrowserToolFactory,
    attach_browser_tool,
    browser_automation_provider,
    browser_close_session,
    browser_state,
    browser_take_screenshot,
    browser_tool_schema,
    browser_url_refusal,
    clear_browser_provider,
    build_browser_tools,
    redirect_refusal,
    register_browser_provider,
)

__all__ = [
    "BrowserActionFn",
    "BrowserAutomationProvider",
    "BrowserStateFn",
    "BrowserTool",
    "BrowserToolFactory",
    "attach_browser_tool",
    "browser_automation_provider",
    "browser_close_session",
    "browser_state",
    "browser_take_screenshot",
    "browser_tool_schema",
    "browser_url_refusal",
    "clear_browser_provider",
    "build_browser_tools",
    "redirect_refusal",
    "register_browser_provider",
]
