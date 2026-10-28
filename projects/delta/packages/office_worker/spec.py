"""Validation of the structured content the Office Worker builds from (ADR-0085).

A spec is plain JSON. Every limit is checked before anything is written, so a bad spec fails with
a message that names the field, and never leaves a half-written file behind.
"""

from __future__ import annotations

import math
from collections.abc import Sequence
from typing import Any

MAX_BLOCKS = 2_000
MAX_TEXT_CHARS = 32_000
MAX_CELLS = 200_000
MAX_SHEETS = 50
MAX_SLIDES = 200
MAX_TABLE_COLUMNS = 64


def text(
    value: Any, label: str, *, is_required: bool = True, max_chars: int = MAX_TEXT_CHARS
) -> str:
    """A plain string, bounded in length. Empty is allowed only when the field is optional."""
    if value is None and not is_required:
        return ""
    if not isinstance(value, str):
        raise ValueError(f"{label} must be a string")
    if is_required and not value.strip():
        raise ValueError(f"{label} must not be empty")
    if len(value) > max_chars:
        raise ValueError(f"{label} is longer than {max_chars} characters")
    return value


def items(value: Any, label: str, *, limit: int = MAX_BLOCKS) -> list[Any]:
    """A bounded list."""
    if not isinstance(value, list):
        raise ValueError(f"{label} must be a list")
    if len(value) > limit:
        raise ValueError(f"{label} has more than {limit} entries")
    return value


def cell(value: Any, label: str) -> str | int | float | bool | None:
    """One table cell: text, a finite number, a boolean or nothing."""
    if value is None or isinstance(value, bool):
        return value
    if isinstance(value, int):
        return value
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError(f"{label} must be a finite number")
        return value
    if isinstance(value, str):
        if len(value) > MAX_TEXT_CHARS:
            raise ValueError(f"{label} is longer than {MAX_TEXT_CHARS} characters")
        return value
    raise ValueError(f"{label} must be text, a number, a boolean or empty")


def table(columns: Any, rows: Any, label: str) -> tuple[list[str], list[list[Any]]]:
    """A header row and body rows of one rectangular table, within the cell limit."""
    headers = [
        text(name, f"{label} column name", max_chars=256)
        for name in items(columns, f"{label} columns", limit=MAX_TABLE_COLUMNS)
    ]
    if not headers:
        raise ValueError(f"{label} needs at least one column")
    body: list[list[Any]] = []
    for row_index, row in enumerate(items(rows, f"{label} rows", limit=MAX_CELLS)):
        if not isinstance(row, list) or len(row) != len(headers):
            raise ValueError(f"{label} row {row_index} must have {len(headers)} cells")
        body.append([cell(value, f"{label} row {row_index}") for value in row])
    if len(body) * len(headers) > MAX_CELLS:
        raise ValueError(f"{label} has more than {MAX_CELLS} cells")
    return headers, body


def is_number(value: Any) -> bool:
    """True for a numeric cell, which is aligned to the right and kept numeric in XLSX."""
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def display(value: Any) -> str:
    """The text a cell shows: numbers without a trailing '.0', booleans as yes or no."""
    if value is None:
        return ""
    if isinstance(value, bool):
        return "yes" if value else "no"
    if isinstance(value, float) and value.is_integer():
        return str(int(value))
    return str(value)


def blocks(value: Any, label: str = "blocks") -> Sequence[dict[str, Any]]:
    """Document blocks, each a JSON object with a known `type`."""
    result = items(value, label)
    for index, block in enumerate(result):
        if not isinstance(block, dict):
            raise ValueError(f"{label}[{index}] must be an object")
    return result
