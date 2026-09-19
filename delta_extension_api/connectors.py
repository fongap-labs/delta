"""Stable connector contracts exposed to optional Delta extensions."""

from integrations.connectors.catalog_copy import register_catalog, register_catalog_copy
from integrations.connectors.descriptors import (
    ConnectorDescriptor,
    Field,
    ValidationResult,
    get_descriptor,
    list_descriptors,
    register_descriptor,
    register_descriptors,
)
from integrations.connectors.email_tools import make_email_tools, validate_email_account
from integrations.connectors.tool_defs import (
    ConnectorToolDef,
    approval_for_tool,
    connector_for_tool,
    register_runtime_tool,
    register_tool_def,
    register_tool_defs,
    target_arg_for,
)
from integrations.connectors.tool_factories import (
    ConnectorToolFactory,
    register_tool_factory,
)

__all__ = [
    "ConnectorDescriptor",
    "ConnectorToolDef",
    "ConnectorToolFactory",
    "Field",
    "ValidationResult",
    "approval_for_tool",
    "connector_for_tool",
    "get_descriptor",
    "list_descriptors",
    "make_email_tools",
    "register_catalog",
    "register_catalog_copy",
    "register_descriptor",
    "register_descriptors",
    "register_runtime_tool",
    "register_tool_def",
    "register_tool_defs",
    "register_tool_factory",
    "target_arg_for",
    "validate_email_account",
]
