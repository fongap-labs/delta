"""Stable Python extension API for Delta Foundation.

Optional first-party and third-party extensions should import from this package,
not from Foundation's internal `integrations.*` or `packages.*` modules.
The facade is intentionally narrow: Foundation may reorganize internals without
breaking installed extensions as long as this surface remains compatible.
"""

from .credentials import CredentialStore, SecretStore

__all__ = ["CredentialStore", "SecretStore"]
