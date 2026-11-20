"""The command line writes one exploratory result file, or exits 2 with a clear message."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from packages.explore_pack import cli


def test_cli_writes_a_regression_result(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    csv_path = tmp_path / "data.csv"
    csv_path.write_text("x,y\n1,2.1\n2,3.9\n3,6.2\n4,8.1\n", encoding="utf-8")
    out = tmp_path / "result.json"

    code = cli.run(
        [
            "regression",
            "--csv",
            str(csv_path),
            "--x",
            "x",
            "--y",
            "y",
            "--out",
            str(out),
        ]
    )

    assert code == 0
    document = json.loads(out.read_text(encoding="utf-8"))
    assert document["assurance"] == "exploratory"
    assert document["method"]["id"] == "explore.regression"
    assert document["inputs"][0]["path"] == "data.csv"
    assert len(document["inputs"][0]["sha256"]) == 64
    assert capsys.readouterr().err == ""


def test_cli_exits_two_on_a_missing_column(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    csv_path = tmp_path / "data.csv"
    csv_path.write_text("x\n1\n2\n", encoding="utf-8")
    out = tmp_path / "result.json"

    code = cli.run(
        ["describe", "--csv", str(csv_path), "--column", "absent", "--out", str(out)]
    )

    assert code == 2
    assert "column not found" in capsys.readouterr().err
    assert not out.exists()
