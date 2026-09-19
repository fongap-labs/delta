"""Credential interface exposed to extension implementations.

Production capability workers should normally receive grant-scoped, memory-only
credentials from the Rust CapabilityHost. The concrete store alias remains here
for legacy/headless extension contracts and tests.
"""

from packages.credential_store import CredentialStore

SecretStore = CredentialStore

__all__ = ["CredentialStore", "SecretStore"]
