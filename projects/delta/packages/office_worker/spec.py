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
        raise ValueError(f"{label} must be a string (text in double quotes)")
    if is_required and not value.strip():
        raise ValueError(f"{label} must not be empty: give a value of 1 to {max_chars} characters")
    if len(value) > max_chars:
        raise ValueError(f"{label} is longer than {max_chars} characters: shorten it to at most {max_chars}")
    return value


def items(value: Any, label: str, *, limit: int = MAX_BLOCKS) -> list[Any]:
    """A bounded list."""
    if not isinstance(value, list):
        raise ValueError(f"{label} must be a list (a JSON array)")
    if len(value) > limit:
        raise ValueError(f"{label} has more than {limit} entries: at most {limit} are allowed")
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
    raise ValueError(f"{label} must be text, a number, true or false, or null")


def table(columns: Any, rows: Any, label: str) -> tuple[list[str], list[list[Any]]]:
    """A header row and body rows of one rectangular table, within the cell limit."""
    headers = [
        text(name, f"{label} column name", max_chars=256)
        for name in items(columns, f"{label} columns", limit=MAX_TABLE_COLUMNS)
    ]
    if not headers:
        raise ValueError(f"{label} needs at least one column: give 1 to {MAX_TABLE_COLUMNS} column names")
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


CHART_TYPES = ("column", "bar", "line", "pie")
MAX_CHART_POINTS = 200
MAX_CHART_SERIES = 8


def chart(value: Any, label: str) -> dict[str, Any]:
    """One chart over a list of categories and one or more numeric series.

    `column` and `bar` compare series across the categories, `line` shows a trend, and `pie`
    takes exactly one series. Every series needs one value per category.
    """
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be an object with type, categories and series")
    kind = value.get("type")
    if kind not in CHART_TYPES:
        raise ValueError(f"{label} type must be one of {', '.join(CHART_TYPES)}")
    categories = [
        text(name, f"{label} category", max_chars=100)
        for name in items(
            value.get("categories"), f"{label} categories", limit=MAX_CHART_POINTS
        )
    ]
    if not categories:
        raise ValueError(
            f"{label} needs at least one category: give 1 to {MAX_CHART_POINTS} names"
        )
    raw_series = items(value.get("series"), f"{label} series", limit=MAX_CHART_SERIES)
    if not raw_series:
        raise ValueError(f"{label} needs at least one series: give 1 to {MAX_CHART_SERIES}")
    if kind == "pie" and len(raw_series) != 1:
        raise ValueError(f"{label} pie charts take exactly one series")
    series: list[dict[str, Any]] = []
    for index, raw in enumerate(raw_series):
        series_label = f"{label} series[{index}]"
        if not isinstance(raw, dict):
            raise ValueError(f"{series_label} must be an object with name and values")
        values = items(raw.get("values"), f"{series_label} values", limit=MAX_CHART_POINTS)
        if len(values) != len(categories):
            raise ValueError(
                f"{series_label} has {len(values)} values for {len(categories)} categories: "
                "give one value per category"
            )
        numbers: list[float] = []
        for value_index, number in enumerate(values):
            if not is_number(number) or not math.isfinite(float(number)):
                raise ValueError(
                    f"{series_label} value {value_index} must be a finite number"
                )
            numbers.append(float(number))
        series.append(
            {
                "name": text(raw.get("name"), f"{series_label} name", max_chars=100),
                "values": numbers,
            }
        )
    return {
        "type": kind,
        "title": text(value.get("title"), f"{label} title", is_required=False, max_chars=200),
        "categories": categories,
        "series": series,
    }
