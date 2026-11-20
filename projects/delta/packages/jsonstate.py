"""Read legacy JSON state and retain corrupt files for recovery.

Rust owns Runtime state; the Python credential store still reads this format.
"""

from __future__ import annotations

import json
import os
import time
from pathlib import Path
from typing import Any


def load_json_state(path: Path, default: Any = None) -> Any:
    """Return parsed JSON or `default`; move malformed JSON aside for recovery."""
    try:
        data = path.read_text(encoding="utf-8")
    except OSError:
        return default
    try:
        return json.loads(data)
    except ValueError:
        ts = time.strftime("%Y%m%d%H%M%S")
        backup = path.with_name(path.name + f".corrupt-{ts}")
        try:
            os.replace(path, backup)  # keep the bad bytes, not crash the app
        except OSError:
            pass
        return default
