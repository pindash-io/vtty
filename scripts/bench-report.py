#!/usr/bin/env python3
"""Render criterion benchmark results as Markdown tables.

Reads the JSON that criterion saves under ``target/criterion`` and prints one
pivoted table per benchmark group: rows are workloads, columns are parser
implementations, cells are median wall time plus aggregate throughput.

Usage:
    cargo bench -p vtty_bench
    scripts/bench-report.py
    scripts/bench-report.py --unit MiB/s
    scripts/bench-report.py --group vt_bandwidth vt_parallel

Only the *new* (latest) criterion results are read; the saved ``base``
baselines are ignored.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
from collections import defaultdict

# Canonical column order so the table reads consistently across runs. Unknown
# implementations are appended after these, sorted alphabetically.
IMPL_ORDER = [
    "vtty",
    "vtty-simd",
    "vte",
    "vtparse",
    "rio-vt",
    "fux-vt",
    "vt100",
]

# Canonical row order. Unknown workloads are appended after these, sorted
# alphabetically.
WORKLOAD_ORDER = [
    "plain",
    "scroll_storm",
    "utf8_mixed",
    "ansi_mixed",
    "ansi_two_styles",
    "ansi_bare_csi",
    "sgr_churn",
    "cursor_motion",
    "sync_churn",
    "truecolor_unique",
    "sgr_nvim",
    "bracketed_paste",
    "osc_heavy",
    "dcs_sixel",
]


def load_results(criterion_dir: pathlib.Path):
    """Yield ``(full_id, bytes, median_ns)`` for every ``new`` estimate."""
    for benchmark_json in sorted(criterion_dir.glob("*/**/new/benchmark.json")):
        estimates_json = benchmark_json.with_name("estimates.json")
        if not estimates_json.exists():
            continue
        benchmark = json.loads(benchmark_json.read_text())
        estimates = json.loads(estimates_json.read_text())
        throughput = benchmark.get("throughput") or {}
        nbytes = throughput.get("Bytes")
        median_ns = estimates["median"]["point_estimate"]
        yield benchmark["full_id"], nbytes, median_ns


def order_key(value: str, preferred: list[str]) -> tuple[int, str]:
    """Sort ``preferred`` items first in list order, then the rest by name."""
    if value in preferred:
        return (0, preferred.index(value))
    return (1, value)


def fmt_throughput(nbytes: int | None, median_ns: float, unit: str) -> str:
    """Format median throughput in the requested unit, or ``-`` if unknown."""
    if nbytes is None:
        return "-"
    bytes_per_sec = nbytes / (median_ns / 1e9)
    if unit == "GiB/s":
        return f"{bytes_per_sec / 1024 ** 3:.2f} GiB/s"
    if unit == "MiB/s":
        return f"{bytes_per_sec / 1024 ** 2:.0f} MiB/s"
    raise ValueError(f"unknown unit: {unit}")


def render_tables(
    criterion_dir: pathlib.Path,
    unit: str,
    groups: list[str] | None,
) -> str:
    results = load_results(criterion_dir)
    by_group: dict[str, list[tuple[str, int | None, float]]] = defaultdict(list)
    for full_id, nbytes, median_ns in results:
        group, _, _ = full_id.split("/", 2)
        by_group[group].append((full_id, nbytes, median_ns))

    out: list[str] = []
    for group in sorted(by_group):
        if groups and group not in groups:
            continue

        rows = by_group[group]
        workloads: set[str] = set()
        impls: set[str] = set()
        data: dict[tuple[str, str], tuple[int | None, float]] = {}
        for full_id, nbytes, median_ns in rows:
            _, impl, workload = full_id.split("/", 2)
            workloads.add(workload)
            impls.add(impl)
            data[(workload, impl)] = (nbytes, median_ns)

        workloads_sorted = sorted(
            workloads, key=lambda w: order_key(w, WORKLOAD_ORDER)
        )
        impls_sorted = sorted(impls, key=lambda i: order_key(i, IMPL_ORDER))

        out.append(f"## {group}")
        out.append("")
        out.append("| workload | " + " | ".join(impls_sorted) + " |")
        out.append("|---|" + "---|" * len(impls_sorted))
        for workload in workloads_sorted:
            cells = []
            for impl in impls_sorted:
                if (workload, impl) in data:
                    nbytes, median_ns = data[(workload, impl)]
                    ms = median_ns / 1e6
                    throughput = fmt_throughput(nbytes, median_ns, unit)
                    cells.append(f"{ms:.2f} ms ({throughput})")
                else:
                    cells.append("-")
            out.append("| " + workload + " | " + " | ".join(cells) + " |")
        out.append("")

    return "\n".join(out).rstrip() + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--criterion-dir",
        type=pathlib.Path,
        default=pathlib.Path("target/criterion"),
        help="criterion output directory (default: target/criterion)",
    )
    parser.add_argument(
        "--unit",
        choices=["GiB/s", "MiB/s"],
        default="GiB/s",
        help="throughput unit (default: GiB/s)",
    )
    parser.add_argument(
        "--group",
        action="append",
        dest="groups",
        help="only render this benchmark group (repeatable)",
    )
    args = parser.parse_args()

    if not args.criterion_dir.exists():
        print(
            f"error: {args.criterion_dir} not found; run `cargo bench` first",
            file=sys.stderr,
        )
        return 1

    sys.stdout.write(
        render_tables(args.criterion_dir, args.unit, args.groups)
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
