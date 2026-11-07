#!/usr/bin/env python3
"""Offline plotting regressions: python3 -m unittest discover -s scripts -p 'test_plot_results.py'."""

import csv
import hashlib
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

import matplotlib.image as mpimg
import matplotlib.pyplot as plt
import numpy as np

import plot_results as plot


class PlotTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.inputs = self.root / "results"
        shutil.copytree(plot.ROOT / "results", self.inputs)
        self.path = self.inputs / "goldilocks-qa.csv"

    def read_rows(self):
        with self.path.open(newline="") as source:
            return list(csv.DictReader(source))

    def write_rows(self, rows, header=plot.HEADER):
        with self.path.open("w", newline="") as output:
            writer = csv.DictWriter(output, fieldnames=header)
            writer.writeheader()
            writer.writerows(rows)

    def hashes(self):
        return {
            p.name: hashlib.sha256(p.read_bytes()).hexdigest()
            for p in self.inputs.glob("*.csv")
        }

    def test_complete_grid_selection_sorting_and_units(self):
        self.write_rows(list(reversed(self.read_rows())))
        tables = plot.load_results(self.inputs)
        self.assertEqual(sum(map(len, tables.values())), 144)
        seen = set()
        for field in plot.FIELDS:
            for c in plot.CS:
                for code in plot.CODES:
                    for r in plot.RS:
                        x, y = plot.curve(tables, field, c, code, r, "proof_bytes")
                        self.assertEqual(x, [2**mu for mu in plot.MUS])
                        for mu, value in zip(plot.MUS, y):
                            self.assertEqual(
                                value, tables[field, code][mu, r, c].proof_bytes / 1024
                            )
                            seen.add((field, code, mu, r, c))
        self.assertEqual(len(seen), 144)

    def test_missing_file(self):
        self.path.unlink()
        with self.assertRaises(FileNotFoundError):
            plot.load_results(self.inputs)

    def test_wrong_header(self):
        self.path.write_text("mu,r,c,commit_ms,open_ms,verify_ms,size\n")
        with self.assertRaisesRegex(ValueError, "header"):
            plot.load_results(self.inputs)

    def test_missing_duplicate_and_unexpected_configuration(self):
        original = self.read_rows()
        for kind in ("missing", "duplicate", "unexpected"):
            with self.subTest(kind=kind):
                rows = [dict(row) for row in original]
                if kind == "missing":
                    rows.pop()
                elif kind == "duplicate":
                    rows.append(dict(rows[0]))
                else:
                    rows[0]["mu"] = "21"
                self.write_rows(rows)
                with self.assertRaisesRegex(ValueError, kind):
                    plot.load_results(self.inputs)

    def test_invalid_metrics_and_parameters(self):
        original = self.read_rows()
        cases = [
            (key, value)
            for key in plot.HEADER[3:]
            for value in ("0", "-1", "nan", "inf", "-inf", "oops", "")
        ]
        cases += [("mu", "20.5"), ("r", "inf"), ("proof_bytes", "100.5")]
        for key, value in cases:
            with self.subTest(key=key, value=value):
                rows = [dict(row) for row in original]
                rows[0][key] = value
                self.write_rows(rows)
                with self.assertRaises(ValueError):
                    plot.load_results(self.inputs)

    def test_ragged_columns(self):
        original = self.path.read_text()
        for tail in (",extra", ","):
            with self.subTest(tail=tail):
                lines = original.splitlines()
                lines[1] += tail
                self.path.write_text("\n".join(lines) + "\n")
                with self.assertRaisesRegex(ValueError, "7 columns"):
                    plot.load_results(self.inputs)
        lines = original.splitlines()
        lines[1] = ",".join(lines[1].split(",")[:-1])
        self.path.write_text("\n".join(lines) + "\n")
        with self.assertRaisesRegex(ValueError, "7 columns"):
            plot.load_results(self.inputs)

    def test_all_figure_objects_match_source(self):
        tables = plot.load_results(self.inputs)
        limits = plot.common_limits(tables)
        for field in plot.FIELDS:
            for c in plot.CS:
                with self.subTest(field=field, c=c):
                    fig = plot.make_figure(tables, field, c, limits)
                    try:
                        self.assertEqual(len(fig.axes), 4)
                        self.assertEqual(len(fig.legends), 1)
                        expected_labels = [
                            f"{code.upper()}, r={r}"
                            for code in plot.CODES
                            for r in plot.RS
                        ]
                        self.assertEqual(
                            [t.get_text() for t in fig.legends[0].get_texts()],
                            expected_labels,
                        )
                        for ax, (metric, label) in zip(fig.axes, plot.METRICS):
                            self.assertEqual(ax.get_xscale(), "log")
                            self.assertEqual(ax.xaxis._scale.base, 2)
                            self.assertEqual(ax.get_yscale(), "log")
                            self.assertEqual(ax.yaxis._scale.base, 10)
                            self.assertEqual(ax.get_ylabel(), label)
                            self.assertEqual(ax.get_ylim(), limits[metric])
                            self.assertEqual(len(ax.lines), 6)
                            for line, (code, r) in zip(
                                ax.lines, [(a, b) for a in plot.CODES for b in plot.RS]
                            ):
                                self.assertEqual(len(line.get_xdata()), 6)
                                self.assertEqual(line.get_color(), plot.COLORS[code])
                                self.assertEqual(line.get_marker(), plot.MARKERS[r])
                                self.assertEqual(
                                    line.get_linestyle(), plot.LINESTYLES[r]
                                )
                                # Independent source lookup: no plotting selection helpers.
                                with (
                                    self.inputs / f"{field}-{code}.csv"
                                ).open() as source:
                                    source_rows = {
                                        int(row["mu"]): row
                                        for row in csv.DictReader(source)
                                        if int(row["r"]) == r and int(row["c"]) == c
                                    }
                                expected = [
                                    float(source_rows[mu][metric])
                                    / (1024 if metric == "proof_bytes" else 1)
                                    for mu in plot.MUS
                                ]
                                np.testing.assert_array_equal(
                                    line.get_xdata(), [2**mu for mu in plot.MUS]
                                )
                                np.testing.assert_array_equal(
                                    line.get_ydata(), expected
                                )
                                self.assertTrue(
                                    all(
                                        limits[metric][0] < y < limits[metric][1]
                                        for y in expected
                                    )
                                )
                    finally:
                        plt.close(fig)

    def test_render_files_hashes_and_other_cwd(self):
        before = self.hashes()
        output = self.root / "figures"
        output.mkdir()
        sentinel = output / "keep.txt"
        sentinel.write_text("unrelated user file")
        result = subprocess.run(
            [
                sys.executable,
                str(plot.ROOT / "scripts/plot_results.py"),
                "--input-dir",
                str(self.inputs),
                "--output-dir",
                str(output),
            ],
            cwd=self.root,
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertEqual(len(result.stdout.splitlines()), 8)
        self.assertEqual(before, self.hashes())
        self.assertEqual(sentinel.read_text(), "unrelated user file")
        self.assertEqual(len(list(output.iterdir())), 9)
        for field in plot.FIELDS:
            for c in plot.CS:
                base = output / f"hyperfold-{field}-c{c}"
                pdf = base.with_suffix(".pdf").read_bytes()
                self.assertTrue(pdf.startswith(b"%PDF-"))
                self.assertEqual(len(re.findall(rb"/Type /Page\b", pdf)), 1)
                image = mpimg.imread(base.with_suffix(".png"))
                self.assertEqual(image.shape[:2], (2160, 3000))
                self.assertGreater(image.std(), 0.05)
        # Default input paths remain repo-relative even from an unrelated cwd.
        self.assertEqual(plot.repo_path(Path("results")), plot.ROOT / "results")
        self.assertEqual(plot.repo_path(self.inputs), self.inputs)

    def test_no_output_before_all_tables_validated(self):
        (self.inputs / "m127-rs.csv").unlink()
        output = self.root / "not-created"
        result = subprocess.run(
            [
                sys.executable,
                str(plot.ROOT / "scripts/plot_results.py"),
                "--input-dir",
                str(self.inputs),
                "--output-dir",
                str(output),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("m127-rs.csv", result.stderr)
        self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
