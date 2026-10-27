"""Command line entry point: `python -m packages.explore_pack.cli <method> ...`.

The CLI reads one CSV file and writes one `delta.result` v2 JSON file. Every result is exploratory.
"""

from __future__ import annotations

import argparse
import sys
from collections.abc import Sequence
from pathlib import Path

from packages.explore_pack import engine
from packages.explore_pack.result import write_result


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="explore_pack", description=__doc__)
    commands = parser.add_subparsers(dest="method", required=True)

    describe = commands.add_parser("describe", help="summary statistics for one column")
    describe.add_argument("--csv", type=Path, required=True)
    describe.add_argument("--column", required=True)
    describe.add_argument("--out", type=Path, required=True)

    compare = commands.add_parser("compare", help="Welch t-test between two groups")
    compare.add_argument("--csv", type=Path, required=True)
    compare.add_argument("--group-column", required=True)
    compare.add_argument("--value-column", required=True)
    compare.add_argument("--group-a", required=True)
    compare.add_argument("--group-b", required=True)
    compare.add_argument("--out", type=Path, required=True)

    correlation = commands.add_parser(
        "correlation", help="Pearson correlation of two columns"
    )
    correlation.add_argument("--csv", type=Path, required=True)
    correlation.add_argument("--x", dest="x_column", required=True)
    correlation.add_argument("--y", dest="y_column", required=True)
    correlation.add_argument("--out", type=Path, required=True)

    regression = commands.add_parser("regression", help="simple linear regression")
    regression.add_argument("--csv", type=Path, required=True)
    regression.add_argument("--x", dest="x_column", required=True)
    regression.add_argument("--y", dest="y_column", required=True)
    regression.add_argument("--out", type=Path, required=True)
    return parser


def run(argv: Sequence[str]) -> int:
    args = _build_parser().parse_args(argv)
    try:
        if args.method == "describe":
            document = engine.describe_column(args.csv, args.column)
        elif args.method == "compare":
            document = engine.compare_groups(
                args.csv,
                group_column=args.group_column,
                value_column=args.value_column,
                group_a=args.group_a,
                group_b=args.group_b,
            )
        elif args.method == "correlation":
            document = engine.correlation(
                args.csv, x_column=args.x_column, y_column=args.y_column
            )
        else:
            document = engine.regression(
                args.csv, x_column=args.x_column, y_column=args.y_column
            )
    except (OSError, ValueError) as exc:
        print(f"explore_pack: {exc}", file=sys.stderr)
        return 2
    write_result(document, args.out)
    return 0


def main() -> None:
    raise SystemExit(run(sys.argv[1:]))


if __name__ == "__main__":
    main()
