"""Native charts in XLSX and PPTX: built from the spec, and refused when the data cannot chart."""

from __future__ import annotations

import zipfile
from pathlib import Path

import pytest
from pptx import Presentation

from packages.office_worker import pptx_build, spec, xlsx_build


def _sales_sheet(**overrides: object) -> dict:
    sheet: dict[str, object] = {
        "name": "Sales",
        "columns": ["Region", "Q1", "Q2"],
        "rows": [["North", 120, 140], ["South", 90, 110], ["East", 75, 80]],
        "charts": [
            {
                "type": "column",
                "title": "Sales by region",
                "category_column": "Region",
                "value_columns": ["Q1", "Q2"],
            }
        ],
    }
    sheet.update(overrides)
    return sheet


def test_an_xlsx_chart_is_a_native_chart_part(tmp_path: Path) -> None:
    path = tmp_path / "sales.xlsx"
    counts = xlsx_build.build(
        {"title": "Sales", "sheets": [_sales_sheet(), _sales_sheet(name="Trend", charts=[{"type": "line", "category_column": "Region", "value_columns": ["Q2"]}])]},
        path,
    )

    assert counts["charts"] == 2
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
    assert "xl/charts/chart1.xml" in names
    assert "xl/charts/chart2.xml" in names


def test_a_pie_chart_takes_one_value_column(tmp_path: Path) -> None:
    sheet = _sales_sheet(charts=[{"type": "pie", "category_column": "Region", "value_columns": ["Q1", "Q2"]}])

    with pytest.raises(ValueError, match="pie charts take exactly one value column"):
        xlsx_build.build({"title": "Sales", "sheets": [sheet]}, tmp_path / "bad.xlsx")


def test_a_chart_over_a_text_column_is_refused(tmp_path: Path) -> None:
    sheet = _sales_sheet(charts=[{"type": "column", "category_column": "Q1", "value_columns": ["Region"]}])

    with pytest.raises(ValueError, match="must be a number or a formula"):
        xlsx_build.build({"title": "Sales", "sheets": [sheet]}, tmp_path / "bad.xlsx")


def test_a_chart_needs_a_column_the_sheet_has(tmp_path: Path) -> None:
    sheet = _sales_sheet(charts=[{"type": "column", "category_column": "Region", "value_columns": ["Missing"]}])

    with pytest.raises(ValueError, match="must be a different column of the sheet"):
        xlsx_build.build({"title": "Sales", "sheets": [sheet]}, tmp_path / "bad.xlsx")


def test_a_pptx_chart_slide_holds_the_series_as_data(tmp_path: Path) -> None:
    path = tmp_path / "deck.pptx"
    counts = pptx_build.build(
        {
            "title": "Deck",
            "slides": [
                {
                    "layout": "chart",
                    "title": "Revenue",
                    "chart": {
                        "type": "line",
                        "title": "By quarter",
                        "categories": ["Q1", "Q2", "Q3"],
                        "series": [{"name": "2025", "values": [1.5, 2, 2.75]}],
                    },
                }
            ],
        },
        path,
    )

    assert counts["charts"] == 1
    slide = Presentation(str(path)).slides[0]
    chart_shapes = [shape for shape in slide.shapes if shape.has_chart]
    assert len(chart_shapes) == 1
    chart = chart_shapes[0].chart
    assert list(chart.plots[0].categories) == ["Q1", "Q2", "Q3"]
    assert list(chart.series[0].values) == [1.5, 2.0, 2.75]


def test_a_chart_with_mismatched_values_says_what_to_change() -> None:
    with pytest.raises(ValueError, match="give one value per category"):
        spec.chart(
            {"type": "column", "categories": ["A", "B"], "series": [{"name": "x", "values": [1]}]},
            "chart",
        )


def test_a_non_finite_chart_value_is_refused() -> None:
    with pytest.raises(ValueError, match="must be a finite number"):
        spec.chart(
            {"type": "column", "categories": ["A"], "series": [{"name": "x", "values": [float("nan")]}]},
            "chart",
        )


def test_a_docx_chart_is_inserted_as_a_picture(tmp_path: Path) -> None:
    from docx import Document

    from packages.office_worker import docx_build

    path = tmp_path / "report.docx"
    counts = docx_build.build(
        {
            "title": "Report",
            "blocks": [
                {
                    "type": "chart",
                    "chart": {
                        "type": "column",
                        "title": "Sales by region",
                        "categories": ["North", "South"],
                        "series": [{"name": "Q1", "values": [120, 90]}],
                    },
                }
            ],
        },
        path,
    )

    assert counts == {"charts": 1}
    assert len(Document(str(path)).inline_shapes) == 1


def test_a_pie_chart_cannot_show_negative_values() -> None:
    from packages.office_worker import chart_render

    with pytest.raises(ValueError, match="cannot show negative values"):
        chart_render.render(
            {
                "type": "pie",
                "title": "",
                "categories": ["A", "B"],
                "series": [{"name": "x", "values": [1.0, -2.0]}],
            }
        )


@pytest.mark.parametrize("kind", ["column", "bar", "line", "pie"])
def test_every_chart_type_renders_a_png(kind: str) -> None:
    from packages.office_worker import chart_render

    series = [{"name": "x", "values": [1.0, 2.0, 3.0]}]
    if kind != "pie":
        series.append({"name": "y", "values": [2.0, 1.0, 4.0]})
    png = chart_render.render(
        {"type": kind, "title": "Check", "categories": ["A", "B", "C"], "series": series[: 1 if kind == "pie" else 2]}
    )

    assert png.startswith(b"\x89PNG")


def test_cjk_labels_are_refused_when_no_cjk_font_is_installed(monkeypatch: pytest.MonkeyPatch) -> None:
    from packages.office_worker import chart_render

    monkeypatch.setattr(chart_render, "find_cjk_font", lambda: None)

    with pytest.raises(ValueError, match="no CJK font installed"):
        chart_render.render(
            {"type": "column", "title": "销售", "categories": ["北区"], "series": [{"name": "一季度", "values": [1.0]}]}
        )


def test_cjk_labels_render_when_a_cjk_font_is_installed(monkeypatch: pytest.MonkeyPatch) -> None:
    from packages.office_worker import chart_render

    installed = chart_render.find_cjk_font()
    if installed is None:
        pytest.skip("no CJK font is installed on this machine")
    png = chart_render.render(
        {"type": "column", "title": "销售", "categories": ["北区"], "series": [{"name": "一季度", "values": [1.0]}]}
    )

    assert png.startswith(b"\x89PNG")
