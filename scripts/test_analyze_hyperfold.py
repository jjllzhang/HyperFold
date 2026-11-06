"""Evidence-state validation using an explicitly synthetic failure fixture."""
import csv
import json
import pathlib
import subprocess
import sys
import tarfile
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[1]


class AnalysisStateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="hyperfold-evidence-fixture-")
        self.root = pathlib.Path(self.temp.name) / "evidence"
        self.root.mkdir()
        archive = REPO / "experiments/retained-2026-11-06/raw-evidence.tar.gz"
        with tarfile.open(archive) as stream:
            stream.extractall(self.root, filter="data")

    def tearDown(self):
        self.temp.cleanup()

    def validate(self):
        result = subprocess.run([sys.executable, str(REPO / "scripts/analyze_hyperfold.py"),
                                 "--root", str(self.root), "--output", str(self.root / "unwritten"),
                                 "--parameter-dir", str(REPO / "data"), "--validate-only"],
                                capture_output=True, text=True)
        self.assertFalse((self.root / "unwritten").exists())
        return result

    def make_runtime_failure_fixture(self, update_status=True):
        path = self.root / "final-results.csv"
        with path.open(newline="") as stream:
            records = list(csv.DictReader(stream))
        for row in records:
            if row["status"] == "memory_guard_stopped":
                row["status"] = "runtime_failed"
        with path.open("w", newline="") as stream:
            writer = csv.DictWriter(stream, fieldnames=list(records[0]))
            writer.writeheader()
            writer.writerows(records)
        path = self.root / "final-finished.json"
        summary = json.loads(path.read_text())
        summary.update(memory_guard_stopped=0, runtime_failed=2, memory_guard_enabled=False)
        path.write_text(json.dumps(summary))
        if update_status:
            for path in (self.root / "actual-attempts").glob("*/status.json"):
                state = json.loads(path.read_text())
                state.update(memory_guard=False, memory_guard_enabled=False, stop_reason=None,
                             exit_code=137, system_before={"vmstat": {"oom_kill": 100}},
                             system_after={"vmstat": {"oom_kill": 101}})
                path.write_text(json.dumps(state))

    def test_existing_guarded_evidence_is_accepted(self):
        result = self.validate()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_explicit_synthetic_runtime_failure_is_accepted_without_plots(self):
        self.make_runtime_failure_fixture()
        result = self.validate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["runtime_failed"], 2)

    def test_guard_stops_cannot_be_relabelled_as_runtime_failures(self):
        self.make_runtime_failure_fixture(update_status=False)
        self.assertNotEqual(self.validate().returncode, 0)


if __name__ == "__main__":
    unittest.main()
