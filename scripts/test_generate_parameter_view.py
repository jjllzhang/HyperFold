import csv
import unittest
from collections import Counter

import generate_parameter_view as view


class ParameterViewTests(unittest.TestCase):
    def test_generated_view_matches_all_source_rows(self):
        expected = []
        for source in view.SOURCES:
            with (view.DATA / source).open(newline="", encoding="utf-8") as file:
                expected.extend(
                    tuple(row[column] for column in view.COLUMNS)
                    for row in csv.DictReader(file)
                )

        generated = view.generate()
        actual = []
        group = None
        for line in generated.splitlines():
            if line.startswith("## "):
                group = tuple(line[3:].split(" / "))
            elif line.startswith("| "):
                cells = tuple(cell.strip() for cell in line.strip("|").split("|"))
                if cells[0] in ("base_field", "---"):
                    continue
                self.assertEqual(cells[:2], group)
                actual.append(cells)
            else:
                self.assertEqual(line, "")

        self.assertEqual(len(expected), 144)
        self.assertEqual(len(actual), 144)
        self.assertEqual(Counter(actual), Counter(expected))
        self.assertEqual(
            (view.DATA / "parameter-profiles.md").read_text(encoding="utf-8"),
            generated,
        )


if __name__ == "__main__":
    unittest.main()
