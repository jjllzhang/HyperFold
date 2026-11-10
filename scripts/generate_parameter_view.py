#!/usr/bin/env python3
"""Generate the human-readable view of the bundled parameter certificates."""

import csv
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "data"
SOURCES = ("goldilocks-parameters.csv", "m127-parameters.csv")
COLUMNS = (
    "base_field", "backend", "mu", "r", "c", "extension_degree", "queries",
    "total_IOP_bits_lower",
)


def generate():
    groups = defaultdict(list)
    for source in SOURCES:
        with (DATA / source).open(newline="", encoding="utf-8") as file:
            for row in csv.DictReader(file):
                groups[(row["base_field"], row["backend"])].append(row)

    lines = []
    for (field, backend), rows in sorted(groups.items()):
        lines.extend([
            f"## {field} / {backend}",
            "",
            "| " + " | ".join(COLUMNS) + " |",
            "| " + " | ".join("---" for column in COLUMNS) + " |",
        ])
        for row in rows:
            lines.append("| " + " | ".join(row[column] for column in COLUMNS) + " |")
        lines.append("")
    return "\n".join(lines)


def main():
    (DATA / "parameter-profiles.md").write_text(generate(), encoding="utf-8")


if __name__ == "__main__":
    main()
