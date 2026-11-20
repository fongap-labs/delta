"""Directory grants for controlled Python capability workers.

Rust validates grants; Python only consumes the resulting scope.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass
class RootDir:
    path: Path
    writable: bool = False
    label: str = ""  # display name; defaults to the dir's basename

    def __post_init__(self) -> None:
        self.path = Path(self.path).expanduser().resolve()
        if not self.label:
            self.label = self.path.name or str(self.path)

    def to_dict(self) -> dict[str, Any]:
        return {"path": str(self.path), "writable": self.writable, "label": self.label}
