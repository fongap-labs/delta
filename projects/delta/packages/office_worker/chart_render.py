"""Chart pictures for Word documents (ADR-0085).

Word has no native chart object that python-docx can write, so each chart is drawn with matplotlib
into a PNG and inserted as a picture. The picture keeps its glyphs, so the document looks the same
wherever it is opened.

Labels that need a CJK font use a font installed on the machine that runs the worker. Delta ships
no font file: a machine without a suitable font is refused with a message, never given a picture of
empty boxes.
"""

from __future__ import annotations

import io
from functools import lru_cache
from typing import Any

import matplotlib

matplotlib.use("Agg")

from matplotlib import font_manager  # noqa: E402
from matplotlib import pyplot as plt  # noqa: E402

# Windows, macOS and Linux names, in the order they are preferred.
CJK_FONT_NAMES = (
    "Microsoft YaHei",
    "SimHei",
    "PingFang SC",
    "Noto Sans CJK SC",
    "Source Han Sans SC",
    "Source Han Sans",
    "WenQuanYi Zen Hei",
    "Droid Sans Fallback",
)
DEFAULT_FONT = "DejaVu Sans"
DPI = 200
WIDTH_INCHES = 6.0
HEIGHT_INCHES = 3.6


@lru_cache(maxsize=1)
def find_cjk_font() -> str | None:
    """The first CJK font the machine has installed, or None."""
    installed = {font.name for font in font_manager.fontManager.ttflist}
    for name in CJK_FONT_NAMES:
        if name in installed:
            return name
    return None


def _has_non_ascii(texts: list[str]) -> bool:
    return any(ord(character) > 127 for text in texts for character in text)


def render(chart: dict[str, Any]) -> bytes:
    """A PNG of one chart, from a chart spec already checked by `spec.chart`."""
    if chart["type"] == "pie" and any(value < 0 for value in chart["series"][0]["values"]):
        raise ValueError("a pie chart cannot show negative values")
    texts = [chart["title"], *chart["categories"], *(item["name"] for item in chart["series"])]
    cjk = find_cjk_font()
    if _has_non_ascii(texts) and cjk is None:
        raise ValueError(
            "the chart has non-ASCII text, but this machine has no CJK font installed: "
            "install one (for example Microsoft YaHei) or use English labels"
        )
    family = cjk or DEFAULT_FONT
    with plt.rc_context({"font.family": [family], "axes.unicode_minus": False}):
        figure, axes = plt.subplots(figsize=(WIDTH_INCHES, HEIGHT_INCHES), dpi=DPI)
        try:
            _draw(axes, chart)
            if chart["title"]:
                axes.set_title(chart["title"])
            figure.tight_layout()
            buffer = io.BytesIO()
            figure.savefig(buffer, format="png", dpi=DPI, metadata={"Software": "Delta"})
            return buffer.getvalue()
        finally:
            plt.close(figure)


def _draw(axes: Any, chart: dict[str, Any]) -> None:
    kind = chart["type"]
    categories = chart["categories"]
    series = chart["series"]
    if kind == "pie":
        axes.pie(
            series[0]["values"],
            labels=categories,
            autopct="%1.1f%%",
            startangle=90,
            counterclock=False,
        )
        axes.axis("equal")
        return
    positions = list(range(len(categories)))
    if kind == "line":
        for item in series:
            axes.plot(positions, item["values"], marker="o", label=item["name"])
        axes.set_xticks(positions, categories)
        axes.grid(axis="y", alpha=0.3)
    else:
        width = 0.8 / len(series)
        for index, item in enumerate(series):
            offsets = [position - 0.4 + width * (index + 0.5) for position in positions]
            if kind == "column":
                axes.bar(offsets, item["values"], width=width, label=item["name"])
            else:
                axes.barh(offsets, item["values"], height=width, label=item["name"])
        if kind == "column":
            axes.set_xticks(positions, categories)
            axes.grid(axis="y", alpha=0.3)
        else:
            axes.set_yticks(positions, categories)
            axes.invert_yaxis()
            axes.grid(axis="x", alpha=0.3)
    if len(series) > 1:
        axes.legend(frameon=False)
