"""Explore Pack results must validate against the Foundation result schema and stay exploratory."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from packages.explore_pack import engine

SCHEMA = Path(__file__).resolve().parents[3] / "schemas" / "analysis-result.schema.json"


@pytest.fixture
def sample_csv(tmp_path: Path) -> Path:
    path = tmp_path / "sample.csv"
    path.write_text(
        "group,value,x,y\n"
        "A,5.1,1,2.1\nA,4.9,2,3.9\nA,5.6,3,6.2\nA,5.8,4,8.1\nA,6.0,5,9.8\nA,5.2,6,12.2\n"
        "B,6.3,7,13.8\nB,6.1,8,16.1\nB,6.6,,\nB,6.8,9,\n",
        encoding="utf-8",
    )
    return path


def _validate(document: dict[str, object]) -> None:
    jsonschema = pytest.importorskip("jsonschema")
    schema = json.loads(SCHEMA.read_text(encoding="utf-8"))
    jsonschema.validate(document, schema)


def test_every_method_writes_a_valid_exploratory_result(sample_csv: Path) -> None:
    documents = [
        engine.describe_column(sample_csv, "value"),
        engine.compare_groups(
            sample_csv,
            group_column="group",
            value_column="value",
            group_a="A",
            group_b="B",
        ),
        engine.correlation(sample_csv, x_column="x", y_column="y"),
        engine.regression(sample_csv, x_column="x", y_column="y"),
    ]

    for document in documents:
        assert document["assurance"] == "exploratory"
        assert document["format"] == "delta.result"
        assert document["version"] == 2
        assert "validation" not in document
        _validate(document)


def test_blank_cells_are_counted_as_warnings(sample_csv: Path) -> None:
    document = engine.describe_column(sample_csv, "value")
    assert document["values"]["n"] == {"value": 10.0}

    correlation = engine.correlation(sample_csv, x_column="x", y_column="y")
    assert correlation["values"]["n"] == {"value": 8.0}
    assert any("blank value skipped" in warning for warning in correlation["warnings"])


def test_a_non_numeric_cell_names_the_line(tmp_path: Path) -> None:
    path = tmp_path / "bad.csv"
    path.write_text("value\n1.0\nnot-a-number\n", encoding="utf-8")

    with pytest.raises(ValueError, match="line 3: value is not a number"):
        engine.describe_column(path, "value")


def test_a_missing_column_is_reported(sample_csv: Path) -> None:
    with pytest.raises(ValueError, match="column not found"):
        engine.describe_column(sample_csv, "absent")
