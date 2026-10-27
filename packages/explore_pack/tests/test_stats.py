"""Explore Pack statistics against independent 50-digit reference values (mpmath)."""

from __future__ import annotations

import math

import pytest

from packages.explore_pack import stats

GROUP_A = [5.1, 4.9, 5.6, 5.8, 6.0, 5.2]
GROUP_B = [6.3, 6.1, 6.6, 6.8, 6.4]
X = [float(value) for value in range(1, 9)]
Y = [2.1, 3.9, 6.2, 8.1, 9.8, 12.2, 13.8, 16.1]


def test_describe_matches_the_reference() -> None:
    summary = stats.describe(GROUP_A)

    assert summary["n"] == 6.0
    assert math.isclose(summary["mean"], 5.433333333333333, rel_tol=1e-12)
    assert math.isclose(summary["sd"], 0.43204937989385734, rel_tol=1e-12)
    assert summary["min"] == 4.9
    assert math.isclose(summary["median"], 5.4, rel_tol=1e-12)
    assert summary["max"] == 6.0


def test_welch_t_matches_the_reference() -> None:
    result = stats.welch_t(GROUP_A, GROUP_B)

    assert math.isclose(result["t"], -4.708415701185243, rel_tol=1e-9)
    assert math.isclose(result["df"], 8.463983429091806, rel_tol=1e-9)
    assert math.isclose(result["p_value"], 0.0013083016604108745, rel_tol=1e-9)
    assert math.isclose(result["mean_difference"], -1.0066666666666667, rel_tol=1e-12)


def test_pearson_matches_the_reference() -> None:
    result = stats.pearson(X, Y)

    assert math.isclose(result["r"], 0.9994260171014314, rel_tol=1e-12)
    assert math.isclose(result["t"], 72.26435488054515, rel_tol=1e-9)
    assert result["df"] == 6.0
    assert math.isclose(result["p_value"], 4.7255231047849784e-10, rel_tol=1e-7)


def test_linear_regression_matches_the_reference() -> None:
    result = stats.linear_regression(X, Y)

    assert math.isclose(result["slope"], 1.9904761904761904, rel_tol=1e-12)
    assert math.isclose(result["intercept"], 0.06785714285714286, rel_tol=1e-9)
    assert math.isclose(result["r_squared"], 0.9988523636592306, rel_tol=1e-12)
    assert math.isclose(result["slope_se"], 0.027544370855679804, rel_tol=1e-9)
    assert math.isclose(result["p_value"], 4.7255231047849784e-10, rel_tol=1e-7)


def test_pearson_reports_infinite_t_for_a_perfect_line_without_failing() -> None:
    result = stats.pearson([1.0, 2.0, 3.0, 4.0], [2.0, 4.0, 6.0, 8.0])

    assert result["r"] == pytest.approx(1.0)
    assert result["p_value"] == 0.0
    assert math.isinf(result["t"])


@pytest.mark.parametrize(
    ("call", "message"),
    [
        (lambda: stats.describe([1.0]), "at least two"),
        (lambda: stats.describe([1.0, math.nan]), "finite"),
        (lambda: stats.welch_t([1.0, 1.0], [1.0, 1.0]), "zero variance"),
        (lambda: stats.pearson([1.0, 2.0], [1.0, 2.0]), "at least three"),
        (
            lambda: stats.linear_regression([1.0, 1.0, 1.0], [1.0, 2.0, 3.0]),
            "must vary",
        ),
    ],
)
def test_invalid_input_raises_a_clear_error(call, message: str) -> None:
    with pytest.raises(ValueError, match=message):
        call()
