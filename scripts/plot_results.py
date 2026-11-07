#!/usr/bin/env python3
"""Plot the four recorded HyperFold result tables; never run benchmarks.

Reproduce: python3 scripts/plot_results.py --input-dir results --output-dir figures
Install plotting dependencies: python3 -m pip install -r scripts/requirements-plot.txt
Relative paths are resolved from the repository root, not the caller's cwd.

Layout inspired by ReedWeave, ePrint 2026/2147, Figures 2/3 (mirror revision
2026-10-29). Only the layout is borrowed, not measurements or sampling claims.
The CSVs contain recorded benchmark means from historical experiment batches;
these plots do not imply a new uniform rerun or provide a security proof.
"""

import argparse
import csv
from dataclasses import dataclass
import math
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.ticker import FixedLocator, LogFormatterMathtext, NullLocator

ROOT = Path(__file__).resolve().parents[1]
FIELDS = ("goldilocks", "m127")
CODES = ("qa", "rs")
MUS = (20, 22, 24, 26, 28, 30)
RS = (2, 4, 8)
CS = (2, 4)
HEADER = ("mu", "r", "c", "commit_ms", "open_ms", "verify_ms", "proof_bytes")
METRICS = (
    ("commit_ms", "Commit time (ms)"),
    ("open_ms", "Open time (ms)"),
    ("verify_ms", "Verification time (ms)"),
    ("proof_bytes", "Proof size (KiB)"),
)
COLORS = {"qa": "#009E73", "rs": "#AA4499"}
MARKERS = {2: "o", 4: "s", 8: "^"}
LINESTYLES = {2: "-", 4: "--", 8: ":"}
STYLE = {
    "font.family": "DejaVu Serif",
    "font.size": 11,
    "mathtext.fontset": "dejavuserif",
    "pdf.fonttype": 42,
    "text.usetex": False,
    "axes.spines.top": False,
    "axes.spines.right": False,
    "axes.linewidth": 0.7,
}


@dataclass(frozen=True)
class Record:
    mu: int
    r: int
    c: int
    commit_ms: float
    open_ms: float
    verify_ms: float
    proof_bytes: int


def load_results(input_dir):
    """Validate all four complete tables before producing any output."""
    tables = {}
    expected = {(mu, r, c) for mu in MUS for r in RS for c in CS}
    for field in FIELDS:
        for code in CODES:
            path = Path(input_dir) / f"{field}-{code}.csv"
            rows = {}
            with path.open(newline="", encoding="utf-8") as source:
                reader = csv.DictReader(source)
                if reader.fieldnames != list(HEADER):
                    raise ValueError(f"{path}: expected header {','.join(HEADER)}")
                for line, row in enumerate(reader, start=2):
                    if None in row or any(value is None for value in row.values()):
                        raise ValueError(f"{path}:{line}: expected exactly 7 columns")
                    try:
                        record = Record(
                            *(int(row[key]) for key in HEADER[:3]),
                            *(float(row[key]) for key in HEADER[3:6]),
                            int(row["proof_bytes"]),
                        )
                    except (ValueError, OverflowError) as error:
                        raise ValueError(
                            f"{path}:{line}: invalid numeric value"
                        ) from error
                    key = (record.mu, record.r, record.c)
                    if key not in expected:
                        raise ValueError(
                            f"{path}:{line}: unexpected configuration {key}"
                        )
                    if key in rows:
                        raise ValueError(
                            f"{path}:{line}: duplicate configuration {key}"
                        )
                    for metric, _ in METRICS:
                        value = getattr(record, metric)
                        if not math.isfinite(value) or value <= 0:
                            raise ValueError(
                                f"{path}:{line}: {metric} must be finite and positive"
                            )
                    rows[key] = record
            if set(rows) != expected:
                raise ValueError(
                    f"{path}: missing configurations {sorted(expected - rows.keys())}"
                )
            tables[field, code] = rows
    return tables


def curve(tables, field, c, code, r, metric):
    """Select six real observations in mu order; only bytes -> KiB is scaled."""
    divisor = 1024 if metric == "proof_bytes" else 1
    records = [tables[field, code][mu, r, c] for mu in MUS]
    return [2**row.mu for row in records], [
        getattr(row, metric) / divisor for row in records
    ]


def common_limits(tables):
    """Use the same non-clipping range for a metric across all four figures."""
    limits = {}
    for metric, _ in METRICS:
        divisor = 1024 if metric == "proof_bytes" else 1
        values = [
            getattr(row, metric) / divisor
            for table in tables.values()
            for row in table.values()
        ]
        limits[metric] = (min(values) / 1.3, max(values) * 1.3)
    return limits


def make_figure(tables, field, c, limits):
    with plt.rc_context(STYLE):
        fig, axes = plt.subplots(2, 2, figsize=(10, 7.2))
        for ax, (metric, label) in zip(axes.flat, METRICS):
            for code in CODES:
                for r in RS:
                    x, y = curve(tables, field, c, code, r, metric)
                    ax.plot(
                        x,
                        y,
                        color=COLORS[code],
                        marker=MARKERS[r],
                        linestyle=LINESTYLES[r],
                        linewidth=1.25,
                        markersize=4.5,
                        label=f"{code.upper()}, r={r}",
                    )
            ax.set_xscale("log", base=2)
            ax.set_yscale("log", base=10)
            ax.xaxis.set_major_locator(FixedLocator([2**mu for mu in MUS]))
            ax.xaxis.set_major_formatter(LogFormatterMathtext(base=2))
            ax.xaxis.set_minor_locator(NullLocator())
            ax.set_xlim(2 ** MUS[0] / 1.2, 2 ** MUS[-1] * 1.2)
            ax.set_ylim(*limits[metric])
            ax.set_xlabel("Polynomial size")
            ax.set_ylabel(label)
            ax.grid(True, which="major", color="#CBD3E2", linewidth=0.55)
            ax.set_axisbelow(True)
            ax.tick_params(axis="both", which="major", labelsize=10)
        field_name = "Goldilocks" if field == "goldilocks" else "M127"
        fig.suptitle(
            f"HyperFold · {field_name} · c={c} · 32 threads", y=0.985, fontsize=14
        )
        handles, labels = axes[0, 0].get_legend_handles_labels()
        fig.legend(
            handles,
            labels,
            loc="upper center",
            bbox_to_anchor=(0.53, 0.94),
            ncol=3,
            frameon=False,
            fontsize=10,
        )
        fig.subplots_adjust(
            left=0.105, right=0.975, bottom=0.10, top=0.825, wspace=0.32, hspace=0.38
        )
        return fig


def save_figures(tables, output_dir):
    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    limits = common_limits(tables)
    outputs = []
    for field in FIELDS:
        for c in CS:
            fig = make_figure(tables, field, c, limits)
            base = output_dir / f"hyperfold-{field}-c{c}"
            try:
                pdf = base.with_suffix(".pdf")
                png = base.with_suffix(".png")
                fig.savefig(pdf, metadata={"CreationDate": None, "ModDate": None})
                fig.savefig(png, dpi=300)
                outputs.extend((pdf, png))
            finally:
                plt.close(fig)
    return outputs


def repo_path(path):
    return path if path.is_absolute() else ROOT / path


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--input-dir", type=Path, default=Path("results"))
    parser.add_argument("--output-dir", type=Path, default=Path("figures"))
    args = parser.parse_args()
    try:
        tables = load_results(repo_path(args.input_dir))
        outputs = save_figures(tables, repo_path(args.output_dir))
    except (OSError, ValueError, OverflowError) as error:
        parser.error(str(error))
    for output in outputs:
        print(output)


if __name__ == "__main__":
    main()
