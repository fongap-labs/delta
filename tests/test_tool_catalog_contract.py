from integrations.connectors.tool_defs import (
    ConnectorToolDef,
    approval_for_tool,
    clear_tool_defs,
    connector_for_tool,
    get_tool_def,
    register_runtime_tool,
    register_tool_def,
    target_arg_for,
)


def setup_function():
    clear_tool_defs()


def teardown_function():
    clear_tool_defs()


def test_unknown_tool_keeps_conservative_default():
    assert approval_for_tool("unknown", is_default_approval=True) is True
    assert approval_for_tool("unknown", is_default_approval=False) is False
    assert connector_for_tool("unknown") is None


def test_runtime_metadata_registers_read_and_write_effects():
    register_runtime_tool(
        "example_read",
        capabilities=["example", "read"],
        description="Read example data.",
    )
    register_runtime_tool(
        "example_write",
        capabilities=["example", "write"],
        description="Change example data.",
    )

    assert connector_for_tool("example_read") == "example"
    assert approval_for_tool("example_read", is_default_approval=True) is False
    assert approval_for_tool("example_write", is_default_approval=False) is True


def test_curated_extension_metadata_wins_over_runtime_fallback():
    register_tool_def(
        ConnectorToolDef(
            connector="example",
            name="example_send",
            label="Send example",
            kind="write",
            description="Curated provider description.",
            target_arg="recipient",
        )
    )
    register_runtime_tool(
        "example_send",
        capabilities=["example", "read"],
        description="Runtime fallback must not downgrade this tool.",
    )

    tool = get_tool_def("example_send")
    assert tool is not None
    assert tool.kind == "write"
    assert tool.label == "Send example"
    assert target_arg_for("example_send") == "recipient"
    assert approval_for_tool("example_send", is_default_approval=False) is True


def test_read_tool_cannot_declare_standing_rule_target():
    try:
        register_tool_def(
            ConnectorToolDef(
                connector="example",
                name="unsafe_read",
                label="Unsafe",
                kind="read",
                description="",
                target_arg="target",
            )
        )
    except ValueError as exc:
        assert "read tools cannot declare" in str(exc)
    else:
        raise AssertionError("read target declaration must be rejected")
