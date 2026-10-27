"""Curated exploratory statistics for the Explore Pack (ADR-0080).

The functions use only the standard library, so the pack adds no runtime dependency. Results are
exploratory: they are written with `assurance: exploratory` and cannot enter a formal report.
"""

from __future__ import annotations

import math
import statistics
from collections.abc import Sequence


def _finite(values: Sequence[float], label: str) -> list[float]:
    cleaned = [float(value) for value in values]
    if any(not math.isfinite(value) for value in cleaned):
        raise ValueError(f"{label} must contain only finite numbers")
    return cleaned


def _betacf(a: float, b: float, x: float) -> float:
    """Continued fraction for the regularized incomplete beta function (modified Lentz)."""
    tiny = 1e-300
    qab = a + b
    qap = a + 1.0
    qam = a - 1.0
    c = 1.0
    d = 1.0 - qab * x / qap
    if abs(d) < tiny:
        d = tiny
    d = 1.0 / d
    h = d
    for m in range(1, 1000):
        m2 = 2 * m
        aa = m * (b - m) * x / ((qam + m2) * (a + m2))
        d = 1.0 + aa * d
        if abs(d) < tiny:
            d = tiny
        c = 1.0 + aa / c
        if abs(c) < tiny:
            c = tiny
        d = 1.0 / d
        h *= d * c
        aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2))
        d = 1.0 + aa * d
        if abs(d) < tiny:
            d = tiny
        c = 1.0 + aa / c
        if abs(c) < tiny:
            c = tiny
        d = 1.0 / d
        delta = d * c
        h *= delta
        if abs(delta - 1.0) < 3e-16:
            break
    return h


def regularized_beta(a: float, b: float, x: float) -> float:
    """I_x(a, b), the regularized incomplete beta function."""
    if x <= 0.0:
        return 0.0
    if x >= 1.0:
        return 1.0
    log_front = math.lgamma(a + b) - math.lgamma(a) - math.lgamma(b)
    front = math.exp(log_front + a * math.log(x) + b * math.log1p(-x))
    if x < (a + 1.0) / (a + b + 2.0):
        return front * _betacf(a, b, x) / a
    return 1.0 - front * _betacf(b, a, 1.0 - x) / b


def t_p_value(t_value: float, df: float) -> float:
    """Two-sided p-value of a Student t statistic with `df` degrees of freedom."""
    x = df / (df + t_value * t_value)
    return regularized_beta(df / 2.0, 0.5, x)


def describe(values: Sequence[float]) -> dict[str, float]:
    """Count, mean, sample standard deviation, minimum, median and maximum."""
    data = _finite(values, "values")
    if len(data) < 2:
        raise ValueError("describe needs at least two values")
    return {
        "n": float(len(data)),
        "mean": statistics.fmean(data),
        "sd": statistics.stdev(data),
        "min": min(data),
        "median": statistics.median(data),
        "max": max(data),
    }


def welch_t(first: Sequence[float], second: Sequence[float]) -> dict[str, float]:
    """Welch's two-sample t-test (unequal variances), two-sided."""
    a = _finite(first, "first group")
    b = _finite(second, "second group")
    if len(a) < 2 or len(b) < 2:
        raise ValueError("each group needs at least two values")
    var_a = statistics.variance(a) / len(a)
    var_b = statistics.variance(b) / len(b)
    if var_a + var_b == 0.0:
        raise ValueError("both groups have zero variance")
    t_value = (statistics.fmean(a) - statistics.fmean(b)) / math.sqrt(var_a + var_b)
    df = (var_a + var_b) ** 2 / (var_a**2 / (len(a) - 1) + var_b**2 / (len(b) - 1))
    return {
        "t": t_value,
        "df": df,
        "p_value": t_p_value(t_value, df),
        "mean_difference": statistics.fmean(a) - statistics.fmean(b),
    }


def _paired(x: Sequence[float], y: Sequence[float]) -> tuple[list[float], list[float]]:
    xs = _finite(x, "x")
    ys = _finite(y, "y")
    if len(xs) != len(ys):
        raise ValueError("x and y must have the same number of values")
    if len(xs) < 3:
        raise ValueError("correlation and regression need at least three pairs")
    return xs, ys


def pearson(x: Sequence[float], y: Sequence[float]) -> dict[str, float]:
    """Pearson correlation coefficient with a two-sided t-test of zero correlation."""
    xs, ys = _paired(x, y)
    mean_x = statistics.fmean(xs)
    mean_y = statistics.fmean(ys)
    sxx = sum((value - mean_x) ** 2 for value in xs)
    syy = sum((value - mean_y) ** 2 for value in ys)
    if sxx == 0.0 or syy == 0.0:
        raise ValueError("x and y must each vary")
    sxy = sum((a - mean_x) * (b - mean_y) for a, b in zip(xs, ys, strict=True))
    r = sxy / math.sqrt(sxx * syy)
    df = len(xs) - 2
    if abs(r) >= 1.0:
        p_value = 0.0
        t_value = math.copysign(math.inf, r)
    else:
        t_value = r * math.sqrt(df / (1.0 - r * r))
        p_value = t_p_value(t_value, df)
    return {
        "n": float(len(xs)),
        "r": r,
        "t": t_value,
        "df": float(df),
        "p_value": p_value,
    }


def linear_regression(x: Sequence[float], y: Sequence[float]) -> dict[str, float]:
    """Simple least-squares regression y = intercept + slope * x, with slope test."""
    xs, ys = _paired(x, y)
    n = len(xs)
    mean_x = statistics.fmean(xs)
    mean_y = statistics.fmean(ys)
    sxx = sum((value - mean_x) ** 2 for value in xs)
    if sxx == 0.0:
        raise ValueError("x must vary")
    sxy = sum((a - mean_x) * (b - mean_y) for a, b in zip(xs, ys, strict=True))
    slope = sxy / sxx
    intercept = mean_y - slope * mean_x
    residual = sum(
        (b - (intercept + slope * a)) ** 2 for a, b in zip(xs, ys, strict=True)
    )
    syy = sum((value - mean_y) ** 2 for value in ys)
    r_squared = 1.0 - residual / syy if syy > 0.0 else 1.0
    df = n - 2
    residual_variance = residual / df
    slope_se = math.sqrt(residual_variance / sxx)
    if slope_se == 0.0:
        t_value = math.copysign(math.inf, slope)
        p_value = 0.0
    else:
        t_value = slope / slope_se
        p_value = t_p_value(t_value, df)
    return {
        "n": float(n),
        "slope": slope,
        "intercept": intercept,
        "r_squared": r_squared,
        "slope_se": slope_se,
        "t": t_value,
        "df": float(df),
        "p_value": p_value,
    }
