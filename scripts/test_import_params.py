"""Tests for validation before importing certified parameters."""

import csv
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class ImportParamsTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.source = self.root / 'input'
        self.source.mkdir()
        for directory in ('scripts', 'data', 'src'):
            (self.root / directory).mkdir()
        shutil.copy(ROOT / 'scripts/import_params.py', self.root / 'scripts')
        for name in ('goldilocks', 'm127'):
            shutil.copy(ROOT / 'data' / f'{name}-parameters.csv', self.source)
        self.targets = [self.root / 'data/goldilocks-parameters.csv',
                        self.root / 'data/m127-parameters.csv',
                        self.root / 'src/certified_params.rs']
        for target in self.targets:
            target.write_bytes(b'original')

    def run_import(self, optimized=False):
        return subprocess.run([sys.executable] + (['-O'] if optimized else []) +
                              [str(self.root / 'scripts/import_params.py'), str(self.source)],
                              capture_output=True, text=True)

    def mutate(self, name, column, value):
        path = self.source / f'{name}-parameters.csv'
        with path.open(newline='') as stream:
            reader = csv.DictReader(stream)
            fields = reader.fieldnames
            rows = list(reader)
        rows[-1][column] = value
        with path.open('w', newline='') as stream:
            writer = csv.DictWriter(stream, fieldnames=fields)
            writer.writeheader()
            writer.writerows(rows)

    def test_valid_import(self):
        result = self.run_import()
        self.assertEqual(result.returncode, 0, result.stderr)
        for name in ('goldilocks', 'm127'):
            self.assertEqual((self.root / 'data' / f'{name}-parameters.csv').read_bytes(),
                             (self.source / f'{name}-parameters.csv').read_bytes())
        self.assertEqual(self.targets[2].read_text().count('    HyperFoldConfig {'), 144)

    def test_invalid_input_never_writes_even_under_optimization(self):
        for column, value in [('certificate_version', 'bad'), ('target_bits', '99'),
                              ('total_IOP_failure_upper_exact', '0'),
                              ('extension_degree', '3'), ('eta_exact', 'invalid')]:
            with self.subTest(column=column):
                shutil.copy(ROOT / 'data/m127-parameters.csv', self.source)
                self.mutate('m127', column, value)
                result = self.run_import(optimized=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertTrue(all(target.read_bytes() == b'original' for target in self.targets))

    def test_missing_or_short_second_input_never_writes(self):
        path = self.source / 'm127-parameters.csv'
        for missing in (False, True):
            with self.subTest(missing=missing):
                if missing:
                    path.unlink()
                else:
                    path.write_text('header\n')
                result = self.run_import()
                self.assertNotEqual(result.returncode, 0)
                self.assertTrue(all(target.read_bytes() == b'original' for target in self.targets))


if __name__ == '__main__':
    unittest.main()
