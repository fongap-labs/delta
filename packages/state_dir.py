"""Shared Delta state-directory resolution.

Keep Python extension state paths aligned with the Rust Core authority root. This
module is intentionally provider-neutral: extensions may place public-contract
sidecars under the same state directory, but Foundation remains the reader and
validator of those contracts. The resolver is part of the public extension
contract; it must stay path-compatible with Rust ``CoreControlPlane``.
"""

from __future__ import annotations

import os
from pathlib import Path


def default_state_dir() -> Path:
    """Return the same state directory used by Rust ``CoreControlPlane``."""
    override = os.environ.get("DELTA_STATE_DIR")
    if override:
        return Path(override)
    if os.name == "nt":
        appdata = os.environ.get("APPDATA")
        if appdata:
            return Path(appdata) / "delta"
    return Path(os.environ.get("HOME", ".")) / ".config" / "delta"


__all__ = ["default_state_dir"]
