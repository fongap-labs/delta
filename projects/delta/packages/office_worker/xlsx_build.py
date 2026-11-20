"""Builds a finished XLSX from a structured spec (ADR-0085).

Each sheet has a styled header row that stays visible while scrolling, an autofilter, column widths
fitted to the content, and numbers stored as numbers. A text cell that begins with "=" is a
formula. Formulas that reach outside the workbook (external references, network or indirect
functions) are refused, because a deliverable must not change when it is opened elsewhere.
"""

from __future__ import annotations

import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from openpyxl import Workbook
from openpyxl.chart import BarChart, LineChart, PieChart, Reference
from openpyxl.styles import Alignment, Font, PatternFill
from openpyxl.utils import get_column_letter

from packages.office_worker import spec

HEADER_FILL = PatternFill("solid", fgColor="1F3A5F")
HEADER_FONT = Font(bold=True, color="FFFFFF")
MIN_WIDTH = 8
MAX_WIDTH = 60
FORMULA_LIMIT = 8_192
FORBIDDEN_IN_FORMULA = re.compile(
    r"\[|\b(?:HYPERLINK|WEBSERVICE|INDIRECT|OFFSET|FILEPATH|CELL|INFO)\s*\(",
    re.IGNORECASE,
)
SHEET_NAME_FORBIDDEN = re.compile(r"[\[\]:*?/\\]")
XLSX_CHART_TYPES = ("column", "bar", "line", "pie")
MAX_CHARTS_PER_SHEET = 4
CHART_ROWS = 20


def _sheet_name(raw: Any, label: str, taken: set[str]) -> str:
    name = spec.text(raw, f"{label} name", max_chars=31)
    if SHEET_NAME_FORBIDDEN.search(name):
        raise ValueError(f"{label} name contains a character a sheet name cannot hold")
    if name.lower() in taken:
        raise ValueError(f"{label} name {name!r} is used by another sheet")
    taken.add(name.lower())
    return name


def _formula(value: str, label: str) -> str:
    if len(value) > FORMULA_LIMIT:
        raise ValueError(f"{label} formula is longer than {FORMULA_LIMIT} characters")
    if FORBIDDEN_IN_FORMULA.search(value):
        raise ValueError(
            f"{label} formula reaches outside the workbook, which is refused"
        )
    return value


def _fill_sheet(
    sheet: Any,
    headers: list[str],
    rows: list[list[Any]],
    formats: dict[str, str],
    label: str,
) -> None:
    sheet.append(headers)
    for column in range(1, len(headers) + 1):
        cell = sheet.cell(row=1, column=column)
        cell.font = HEADER_FONT
        cell.fill = HEADER_FILL
        cell.alignment = Alignment(
            horizontal="center", vertical="center", wrap_text=True
        )
    for row in rows:
        sheet.append(
            [
                _formula(value, f"{label} cell")
                if isinstance(value, str) and value.startswith("=")
                else value
                for value in row
            ]
        )
    for index, name in enumerate(headers, start=1):
        number_format = formats.get(name)
        if number_format:
            for row_index in range(2, len(rows) + 2):
                sheet.cell(row=row_index, column=index).number_format = number_format
    for index, name in enumerate(headers, start=1):
        widest = max(
            [len(name)]
            + [
                len(spec.display(row[index - 1]))
                for row in rows
                if index - 1 < len(row)
            ]
        )
        sheet.column_dimensions[get_column_letter(index)].width = min(
            max(widest + 2, MIN_WIDTH), MAX_WIDTH
        )
    sheet.freeze_panes = "A2"
    sheet.auto_filter.ref = (
        f"A1:{get_column_letter(len(headers))}{max(len(rows) + 1, 1)}"
    )


def _charts(
    sheet: Any,
    sheet_spec: dict[str, Any],
    headers: list[str],
    rows: list[list[Any]],
    label: str,
) -> int:
    """Native Excel charts that read the sheet's own columns, so they stay live when data changes."""
    if sheet_spec.get("charts") is None:
        return 0
    specs = spec.items(sheet_spec.get("charts"), f"{label} charts", limit=MAX_CHARTS_PER_SHEET)
    if specs and not rows:
        raise ValueError(f"{label} charts need at least one row of data")
    for index, chart_spec in enumerate(specs):
        chart_label = f"{label} charts[{index}]"
        if not isinstance(chart_spec, dict):
            raise ValueError(f"{chart_label} must be an object")
        kind = chart_spec.get("type")
        if kind not in XLSX_CHART_TYPES:
            raise ValueError(f"{chart_label} type must be one of {', '.join(XLSX_CHART_TYPES)}")
        category = chart_spec.get("category_column")
        if category not in headers:
            raise ValueError(
                f"{chart_label} category_column must be one of the column names: {', '.join(headers)}"
            )
        values = spec.items(
            chart_spec.get("value_columns"),
            f"{chart_label} value_columns",
            limit=spec.MAX_CHART_SERIES,
        )
        if not values:
            raise ValueError(f"{chart_label} needs at least one value column")
        if kind == "pie" and len(values) != 1:
            raise ValueError(f"{chart_label} pie charts take exactly one value column")
        for name in values:
            if name not in headers or name == category:
                raise ValueError(
                    f"{chart_label} value column {name!r} must be a different column of the sheet"
                )
            column = headers.index(name)
            for row_index, row in enumerate(rows, start=2):
                value = row[column]
                if not (
                    spec.is_number(value)
                    or (isinstance(value, str) and value.startswith("="))
                ):
                    raise ValueError(
                        f"{chart_label} value column {name!r} row {row_index} must be a number or a formula"
                    )
        chart_class = {"column": BarChart, "bar": BarChart, "line": LineChart, "pie": PieChart}[kind]
        chart = chart_class()
        if kind in ("column", "bar"):
            chart.type = "col" if kind == "column" else "bar"
        title = spec.text(
            chart_spec.get("title"), f"{chart_label} title", is_required=False, max_chars=200
        )
        chart.title = title or None
        last_row = len(rows) + 1
        for name in values:
            chart.add_data(
                Reference(sheet, min_col=headers.index(name) + 1, min_row=1, max_row=last_row),
                titles_from_data=True,
            )
        chart.set_categories(
            Reference(
                sheet,
                min_col=headers.index(category) + 1,
                min_row=2,
                max_row=last_row,
            )
        )
        chart.width, chart.height = 18, 9
        anchor = f"{get_column_letter(len(headers) + 2)}{2 + index * CHART_ROWS}"
        sheet.add_chart(chart, anchor)
    return len(specs)


def build(content: dict[str, Any], path: Path) -> dict[str, Any]:
    """Write the XLSX to `path` and return what it contains, for the result document."""
    title = spec.text(content.get("title"), "title", max_chars=512)
    sheets = spec.items(content.get("sheets"), "sheets", limit=spec.MAX_SHEETS)
    if not sheets:
        raise ValueError("sheets must contain at least one sheet")
    workbook = Workbook()
    workbook.remove(workbook.worksheets[0])
    workbook.properties.title = title
    workbook.properties.creator = "Delta"
    workbook.properties.created = workbook.properties.modified = datetime.now(
        timezone.utc
    ).replace(tzinfo=None)
    taken: set[str] = set()
    cells = 0
    charts = 0
    for index, sheet_spec in enumerate(sheets):
        label = f"sheets[{index}]"
        if not isinstance(sheet_spec, dict):
            raise ValueError(f"{label} must be an object")
        name = _sheet_name(sheet_spec.get("name"), label, taken)
        headers, rows = spec.table(
            sheet_spec.get("columns"), sheet_spec.get("rows"), label
        )
        formats = sheet_spec.get("number_formats") or {}
        if not isinstance(formats, dict) or not all(
            isinstance(value, str) for value in formats.values()
        ):
            raise ValueError(
                f"{label} number_formats must map column names to format strings"
            )
        sheet = workbook.create_sheet(title=name)
        _fill_sheet(sheet, headers, rows, formats, label)
        cells += (len(rows) + 1) * len(headers)
        charts += _charts(sheet, sheet_spec, headers, rows, label)
    workbook.save(str(path))
    counts = {"sheets": len(sheets), "cells": cells}
    if charts:
        counts["charts"] = charts
    return counts
