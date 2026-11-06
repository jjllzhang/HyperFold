"""Regression checks for immutable attempt evidence; no PCS workloads are run."""

import json
import pathlib
import tempfile
import unittest

from benchmark_evidence import prepare_attempt


class AttemptContractTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="hyperfold-contract-")
        self.root = pathlib.Path(self.temp.name)
        self.command = ["/usr/bin/time", "bench", "--max-estimated-gib", "1024.0"]
        self.contract = {
            "schema": 1,
            "binary_sha256": "b" * 64,
            "command": self.command,
            "memory_guard_enabled": True,
            "min_available_gib": 12,
            "repetitions": 3,
            "policy_directive": "old-directive",
        }

    def tearDown(self):
        self.temp.cleanup()

    def completed(self):
        self.assertIsNone(prepare_attempt(self.root, self.command, self.contract))
        state = {"exit_code": 143, "memory_guard": True, "memory_guard_enabled": True}
        (self.root / "status.json").write_text(json.dumps(state))
        return state

    def snapshot(self):
        return {p.name: p.read_bytes() for p in self.root.iterdir()}

    def test_identical_contract_reuses_without_touching_evidence(self):
        state = self.completed()
        before = self.snapshot()
        self.assertEqual(prepare_attempt(self.root, self.command, self.contract), state)
        self.assertEqual(self.snapshot(), before)

    def test_guard_switch_is_rejected_without_overwriting_command(self):
        self.completed()
        before = self.snapshot()
        changed = dict(
            self.contract, memory_guard_enabled=False, min_available_gib=None
        )
        with self.assertRaisesRegex(RuntimeError, "contract changed"):
            prepare_attempt(self.root, self.command, changed)
        self.assertEqual(self.snapshot(), before)

    def test_command_or_policy_switch_is_rejected(self):
        self.completed()
        before = self.snapshot()
        command = self.command[:-1] + ["2048.0"]
        for cmd, changed in [
            (command, dict(self.contract, command=command)),
            (self.command, dict(self.contract, policy_directive="new-directive")),
        ]:
            with self.assertRaisesRegex(RuntimeError, "contract changed"):
                prepare_attempt(self.root, cmd, changed)
            self.assertEqual(self.snapshot(), before)

    def test_legacy_status_and_partial_attempt_are_not_silently_reused(self):
        (self.root / "status.json").write_text('{"exit_code": 143}')
        before = self.snapshot()
        with self.assertRaisesRegex(RuntimeError, "no immutable contract"):
            prepare_attempt(self.root, self.command, self.contract)
        self.assertEqual(self.snapshot(), before)
        (self.root / "status.json").unlink()
        (self.root / "stdout.log").write_text("partial attempt evidence")
        before = self.snapshot()
        with self.assertRaisesRegex(RuntimeError, "incomplete attempt"):
            prepare_attempt(self.root, self.command, self.contract)
        self.assertEqual(self.snapshot(), before)


if __name__ == "__main__":
    unittest.main()
