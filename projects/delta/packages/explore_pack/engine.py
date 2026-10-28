"""Run one curated Explore Pack method on a CSV file and return a `delta.result` v2 document."""

from __future__ import annotations

from typing import Any

import csv
import hashlib
from pathlib import Path

from packages.explore_pack import stats
from packages.explore_pack.result import build_result, split_publishable

_CSV_LIMITATION = (
    "blank cells are skipped; rows missing a value used by the method are excluded"
)


def _file_sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _read_table(
    path: Path, columns: tuple[str, ...]
) -> tuple[list[dict[str, str]], int]:
    with path.open(encoding="utf-8", newline="") as handle:
        reader = csv.DictReader(handle)
        header = reader.fieldnames or []
        missing = [column for column in columns if column not in header]
        if missing:
            raise ValueError(f"column not found in {path.name}: {', '.join(missing)}")
        rows = list(reader)
    return rows, len(rows)


def _numeric_values(
    rows: list[dict[str, str]], column: str, *, path: Path
) -> list[float | None]:
    values: list[float | None] = []
    for line_number, row in enumerate(rows, start=2):
        cell = (row.get(column) or "").strip()
        if not cell:
            values.append(None)
            continue
        try:
            values.append(float(cell))
        except ValueError as exc:
            raise ValueError(
                f"{path.name} line {line_number}: {column} is not a number: {cell!r}"
            ) from exc
    return values


def _pairs(
    rows: list[dict[str, str]], x_column: str, y_column: str, *, path: Path
) -> tuple[list[float], list[float], int]:
    xs = _numeric_values(rows, x_column, path=path)
    ys = _numeric_values(rows, y_column, path=path)
    kept_x: list[float] = []
    kept_y: list[float] = []
    skipped = 0
    for x_value, y_value in zip(xs, ys, strict=True):
        if x_value is None or y_value is None:
            skipped += 1
            continue
        kept_x.append(x_value)
        kept_y.append(y_value)
    return kept_x, kept_y, skipped


def describe_column(path: Path, column: str) -> dict[str, Any]:
    rows, _ = _read_table(path, (column,))
    values = [
        value for value in _numeric_values(rows, column, path=path) if value is not None
    ]
    skipped = len(rows) - len(values)
    warnings_list: list[str] = []
    summary = split_publishable(stats.describe(values), warnings_list)
    if skipped:
        warnings_list.insert(0, f"{skipped} blank cell(s) skipped")
    return build_result(
        method_id="explore.describe",
        values=summary,
        parameters={"column": column},
        inputs=[(path.name, _file_sha256(path))],
        warnings=warnings_list,
        limitations=[_CSV_LIMITATION],
    )


def compare_groups(
    path: Path,
    *,
    group_column: str,
    value_column: str,
    group_a: str,
    group_b: str,
) -> dict[str, Any]:
    rows, _ = _read_table(path, (group_column, value_column))
    first: list[float] = []
    second: list[float] = []
    skipped = 0
    for row, value in zip(
        rows, _numeric_values(rows, value_column, path=path), strict=True
    ):
        group = (row.get(group_column) or "").strip()
        if value is None:
            skipped += 1
        elif group == group_a:
            first.append(value)
        elif group == group_b:
            second.append(value)
    if not first or not second:
        raise ValueError("both groups must have at least one value")
    warnings_list: list[str] = []
    summary = split_publishable(stats.welch_t(first, second), warnings_list)
    if skipped:
        warnings_list.append(f"{skipped} row(s) with a blank value skipped")
    return build_result(
        method_id="explore.compare_welch",
        values=summary,
        parameters={
            "group_column": group_column,
            "value_column": value_column,
            "group_a": group_a,
            "group_b": group_b,
        },
        inputs=[(path.name, _file_sha256(path))],
        warnings=warnings_list,
        limitations=[_CSV_LIMITATION, "Welch two-sample t-test, two-sided"],
    )


def correlation(path: Path, *, x_column: str, y_column: str) -> dict[str, Any]:
    rows, _ = _read_table(path, (x_column, y_column))
    xs, ys, skipped = _pairs(rows, x_column, y_column, path=path)
    warnings_list: list[str] = []
    values = split_publishable(stats.pearson(xs, ys), warnings_list)
    if skipped:
        warnings_list.append(f"{skipped} row(s) with a blank value skipped")
    return build_result(
        method_id="explore.correlation",
        values=values,
        parameters={"x_column": x_column, "y_column": y_column},
        inputs=[(path.name, _file_sha256(path))],
        warnings=warnings_list,
        limitations=[_CSV_LIMITATION, "Pearson correlation; assumes a linear relation"],
    )


def regression(path: Path, *, x_column: str, y_column: str) -> dict[str, Any]:
    rows, _ = _read_table(path, (x_column, y_column))
    xs, ys, skipped = _pairs(rows, x_column, y_column, path=path)
    warnings_list: list[str] = []
    values = split_publishable(stats.linear_regression(xs, ys), warnings_list)
    if skipped:
        warnings_list.append(f"{skipped} row(s) with a blank value skipped")
    return build_result(
        method_id="explore.regression",
        values=values,
        parameters={"x_column": x_column, "y_column": y_column},
        inputs=[(path.name, _file_sha256(path))],
        warnings=warnings_list,
        limitations=[_CSV_LIMITATION, "Simple least squares; exploratory only"],
    )
