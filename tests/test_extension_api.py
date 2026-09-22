from __future__ import annotations


def test_public_extension_api_imports() -> None:
    from delta_extension_api import CredentialStore, SecretStore
    from delta_extension_api.browser import (
        BrowserAutomationProvider,
        browser_tool_schema,
        register_browser_provider,
    )
    from delta_extension_api.connectors import (
        ConnectorDescriptor,
        ConnectorToolDef,
        Field,
        ValidationResult,
        register_descriptors,
        register_tool_defs,
        register_tool_factory,
    )
    from delta_extension_api.messaging import (
        BasePlatformAdapter,
        ConnectorSettings,
        MessagingProvider,
        SendResult,
        SessionSource,
        TeamAuth,
        register_messaging_provider,
    )
    from delta_extension_api.tooling import (
        attach_connector_tool,
        resolve_limit,
        extract_html_text,
        execute_http_request,
        get_time_ms,
        build_tool_schema,
    )

    assert SecretStore is CredentialStore
    assert BrowserAutomationProvider is not None
    assert callable(browser_tool_schema)
    assert callable(register_browser_provider)
    assert ConnectorDescriptor is not None
    assert ConnectorToolDef is not None
    assert Field is not None
    assert ValidationResult is not None
    assert callable(register_descriptors)
    assert callable(register_tool_defs)
    assert callable(register_tool_factory)
    assert BasePlatformAdapter is not None
    assert ConnectorSettings is not None
    assert MessagingProvider is not None
    assert SendResult is not None
    assert SessionSource is not None
    assert TeamAuth is not None
    assert callable(register_messaging_provider)
    assert callable(attach_connector_tool)
    assert resolve_limit(100, ceiling=20) == 20
    assert extract_html_text("<p>Hello</p>") == "Hello"
    assert callable(execute_http_request)
    assert isinstance(get_time_ms(), int)
    assert build_tool_schema("x", "x", {}, [])["function"]["name"] == "x"
