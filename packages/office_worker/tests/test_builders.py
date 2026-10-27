"""The three builders write what their spec says, and refuse specs they cannot honour."""

from __future__ import annotations

from pathlib import Path
from typing import Any, cast

import pytest
from docx import Document
from openpyxl import load_workbook
from pptx import Presentation

from packages.office_worker import docx_build, pptx_build, spec, xlsx_build


def _docx_spec(**overrides: object) -> dict:
    content: dict = {
        "title": "Quarter review",
        "subtitle": "Q3 2026",
        "header": "Confidential",
        "footer": "Delta",
        "blocks": [
            {"type": "heading", "level": 1, "text": "Results"},
            {"type": "paragraph", "text": "Revenue grew."},
            {"type": "bullets", "items": ["One", "Two"]},
            {"type": "numbered", "items": ["First", "Second"]},
            {
                "type": "table",
                "caption": "Table 1",
                "columns": ["Region", "Revenue"],
                "rows": [["North", 1200.5], ["South", 980]],
            },
            {"type": "page_break"},
            {"type": "heading", "level": 2, "text": "Appendix"},
        ],
    }
    content.update(overrides)
    return content


def test_docx_holds_its_headings_lists_table_and_header_footer(tmp_path: Path) -> None:
    out = tmp_path / "report.docx"
    counts = docx_build.build(_docx_spec(), out)

    document = Document(str(out))
    headings = [p.text for p in document.paragraphs if docx_build.is_heading(p)]
    assert headings == ["Results", "Appendix"]
    assert counts == {
        "headings": 2,
        "paragraphs": 1,
        "lists": 2,
        "tables": 1,
        "page_breaks": 1,
    }
    table = document.tables[0]
    assert [cell.text for cell in table.rows[0].cells] == ["Region", "Revenue"]
    assert [cell.text for cell in table.rows[2].cells] == ["South", "980"]
    assert table.rows[0]._tr.xpath(
        "./w:trPr/w:tblHeader"
    )  # header row repeats on each page
    assert document.sections[0].header.paragraphs[0].text == "Confidential"
    footer_xml = document.sections[0].footer._element.xml
    assert "PAGE" in footer_xml  # a live page number, not typed digits


def test_docx_numbers_are_right_aligned(tmp_path: Path) -> None:
    out = tmp_path / "numbers.docx"
    docx_build.build(_docx_spec(), out)

    cell = Document(str(out)).tables[0].rows[1].cells[1]
    assert cell.paragraphs[0].alignment is not None  # set explicitly, not inherited


def test_pptx_has_one_slide_per_entry_with_notes(tmp_path: Path) -> None:
    out = tmp_path / "deck.pptx"
    counts = pptx_build.build(
        {
            "title": "Q3",
            "slides": [
                {"layout": "title", "title": "Quarter review", "subtitle": "Q3 2026"},
                {
                    "layout": "bullets",
                    "title": "Highlights",
                    "items": ["Up 12%"],
                    "notes": "Lead with it.",
                },
                {
                    "layout": "table",
                    "title": "By region",
                    "columns": ["Region", "Revenue"],
                    "rows": [["North", 1200.5]],
                },
            ],
        },
        out,
    )

    presentation = Presentation(str(out))
    assert counts == {"slides": 3, "notes": 1}
    assert len(presentation.slides) == 3
    assert presentation.slides[0].shapes.title is not None
    assert presentation.slides[0].shapes.title.text == "Quarter review"
    notes = presentation.slides[1].notes_slide.notes_text_frame
    assert notes is not None and notes.text == "Lead with it."
    table_shapes = [shape for shape in presentation.slides[2].shapes if shape.has_table]
    assert cast(Any, table_shapes[0]).table.cell(1, 1).text == "1200.5"
    assert (presentation.slide_width or 0) > (
        presentation.slide_height or 0
    )  # 16:9, not the 4:3 template


def test_xlsx_keeps_numbers_formulas_filters_and_frozen_header(tmp_path: Path) -> None:
    out = tmp_path / "budget.xlsx"
    counts = xlsx_build.build(
        {
            "title": "Budget",
            "sheets": [
                {
                    "name": "Regions",
                    "columns": ["Region", "Revenue", "Share"],
                    "rows": [
                        ["North", 1200.5, "=B2/B4"],
                        ["South", 980, "=B3/B4"],
                        ["Total", "=SUM(B2:B3)", "=B4/B4"],
                    ],
                    "number_formats": {"Revenue": "#,##0.00", "Share": "0.0%"},
                },
                {"name": "Notes", "columns": ["Link"], "rows": [["=Regions!B4"]]},
            ],
        },
        out,
    )

    assert counts == {"sheets": 2, "cells": 14}
    workbook = load_workbook(out)
    regions = workbook["Regions"]
    assert regions["B2"].value == 1200.5 and isinstance(regions["B2"].value, float)
    assert regions["C2"].value == "=B2/B4"
    assert regions.freeze_panes == "A2"
    assert regions.auto_filter.ref == "A1:C4"
    assert regions["B2"].number_format == "#,##0.00"
    assert workbook["Notes"]["A2"].value == "=Regions!B4"


@pytest.mark.parametrize(
    ("builder", "content", "message"),
    [
        (
            docx_build.build,
            {"title": "t", "blocks": [{"type": "heading", "level": 4, "text": "x"}]},
            "heading level",
        ),
        (
            docx_build.build,
            {"title": "t", "blocks": [{"type": "script", "text": "x"}]},
            "unknown type",
        ),
        (docx_build.build, {"title": "  ", "blocks": []}, "must not be empty"),
        (
            docx_build.build,
            {
                "title": "t",
                "blocks": [
                    {"type": "paragraph", "text": "x" * (spec.MAX_TEXT_CHARS + 1)}
                ],
            },
            "longer than",
        ),
        (
            docx_build.build,
            {
                "title": "t",
                "blocks": [{"type": "table", "columns": ["a", "b"], "rows": [[1]]}],
            },
            "must have 2 cells",
        ),
        (
            pptx_build.build,
            {"title": "t", "slides": [{"layout": "video", "title": "x"}]},
            "layout must be one of",
        ),
        (pptx_build.build, {"title": "t", "slides": []}, "at least one slide"),
        (
            xlsx_build.build,
            {"title": "t", "sheets": [{"name": "a/b", "columns": ["x"], "rows": []}]},
            "cannot hold",
        ),
        (
            xlsx_build.build,
            {
                "title": "t",
                "sheets": [
                    {"name": "A", "columns": ["x"], "rows": []},
                    {"name": "a", "columns": ["x"], "rows": []},
                ],
            },
            "used by another sheet",
        ),
        (
            xlsx_build.build,
            {
                "title": "t",
                "sheets": [
                    {"name": "A", "columns": ["x"], "rows": [["=[book.xlsx]Sheet1!A1"]]}
                ],
            },
            "outside the workbook",
        ),
        (
            xlsx_build.build,
            {
                "title": "t",
                "sheets": [
                    {
                        "name": "A",
                        "columns": ["x"],
                        "rows": [['=HYPERLINK("http://x")']],
                    }
                ],
            },
            "outside the workbook",
        ),
        (
            xlsx_build.build,
            {
                "title": "t",
                "sheets": [{"name": "A", "columns": ["x"], "rows": [[float("nan")]]}],
            },
            "finite",
        ),
    ],
)
def test_a_bad_spec_is_refused_with_the_field_named(
    tmp_path: Path, builder, content, message: str
) -> None:
    with pytest.raises(ValueError, match=message):
        builder(content, tmp_path / "out")


def test_a_refused_spec_leaves_no_file_behind_for_xlsx(tmp_path: Path) -> None:
    out = tmp_path / "out.xlsx"
    with pytest.raises(ValueError):
        xlsx_build.build(
            {
                "title": "t",
                "sheets": [
                    {"name": "A", "columns": ["x"], "rows": [["=HYPERLINK(1)"]]}
                ],
            },
            out,
        )

    assert not out.exists()
