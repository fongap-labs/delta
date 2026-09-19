from integrations.connectors.catalog_copy import (
    access_for,
    about_for,
    clear_catalog_copy,
    register_catalog_copy,
)
from integrations.connectors.descriptors import (
    ConnectorDescriptor,
    Field,
    clear_descriptors,
    get_descriptor,
    list_descriptors,
    register_descriptor,
)


def setup_function():
    clear_descriptors()
    clear_catalog_copy()


def teardown_function():
    clear_descriptors()
    clear_catalog_copy()


def test_foundation_catalog_is_provider_driven():
    assert list_descriptors() == []
    assert about_for("vendor-x") == ""
    assert access_for("vendor-x")


def test_extension_can_register_descriptor_and_copy():
    descriptor = ConnectorDescriptor(
        name="example",
        title="Example",
        icon="E",
        blurb="Example extension connector.",
        auth="token",
        two_way=False,
        fields=[Field("token", "Token", secret=True)],
        instructions=["Provide a token."],
    )
    register_descriptor(descriptor)
    register_catalog_copy(
        "example",
        about="Example provider copy.",
        access=["Reads data exposed by the example provider."],
    )

    assert get_descriptor("example") is descriptor
    assert list_descriptors() == [descriptor]
    assert about_for("example") == "Example provider copy."
    assert access_for("example") == ["Reads data exposed by the example provider."]


def test_duplicate_registration_requires_explicit_replace():
    first = ConnectorDescriptor("example", "One", "E", "", "none", False, [], [])
    second = ConnectorDescriptor("example", "Two", "E", "", "none", False, [], [])
    register_descriptor(first)

    try:
        register_descriptor(second)
    except ValueError as exc:
        assert "already registered" in str(exc)
    else:
        raise AssertionError("duplicate descriptor registration must fail")

    register_descriptor(second, should_replace=True)
    assert get_descriptor("example") is second


def test_catalog_copy_replace_is_explicit():
    register_catalog_copy("example", about="one", access=["one"])
    try:
        register_catalog_copy("example", about="two", access=["two"])
    except ValueError as exc:
        assert "already registered" in str(exc)
    else:
        raise AssertionError("duplicate catalog registration must fail")

    register_catalog_copy("example", about="two", access=["two"], should_replace=True)
    assert about_for("example") == "two"
    assert access_for("example") == ["two"]
