from __future__ import annotations

from integrations.connectors import browser_automation as browser


def teardown_function() -> None:
    browser.clear_browser_provider()


def test_foundation_has_no_builtin_interactive_browser_provider() -> None:
    browser.clear_browser_provider()

    assert browser.build_browser_tools() == []
    state = browser.browser_state()
    assert state["available"] is False
    assert state["status"] == "unavailable"
    assert browser.browser_close_session()["ok"] is True


def test_extension_provider_registers_without_owning_policy() -> None:
    browser.clear_browser_provider()

    def sample_tool() -> dict[str, bool]:
        return {"ok": True}

    sample_tool.__name__ = "browser_sample"

    provider = browser.BrowserAutomationProvider(
        name="test-provider",
        make_tools=lambda: [sample_tool],
        state=lambda: {"open": True, "status": "open"},
        screenshot=lambda: {"ok": True},
        close=lambda: {"ok": True},
    )

    browser.register_browser_provider(provider)

    assert browser.browser_automation_provider() is provider
    assert browser.build_browser_tools() == [sample_tool]
    assert browser.browser_state()["provider"] == "test-provider"
    assert browser.browser_take_screenshot()["ok"] is True
    assert browser.browser_close_session()["ok"] is True


def test_provider_registration_is_single_owner_by_default() -> None:
    browser.clear_browser_provider()
    provider = browser.BrowserAutomationProvider(
        name="one",
        make_tools=list,
        state=dict,
        screenshot=dict,
        close=dict,
    )
    browser.register_browser_provider(provider)

    replacement = browser.BrowserAutomationProvider(
        name="two",
        make_tools=list,
        state=dict,
        screenshot=dict,
        close=dict,
    )

    try:
        browser.register_browser_provider(replacement)
    except RuntimeError:
        pass
    else:
        raise AssertionError("provider replacement must require explicit should_replace=True")

    browser.register_browser_provider(replacement, should_replace=True)
    assert browser.browser_automation_provider() is replacement
