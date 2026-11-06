"""Synthetic CLI and temporary CSV tests; these never measure PCS performance."""

import argparse
import csv
import io
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from contextlib import redirect_stdout
from unittest import mock
import benchmark_pcs as bench


class RunnerTests(unittest.TestCase):
    def setUp(self):
        self.addCleanup(os.sched_setaffinity, 0, os.sched_getaffinity(0))
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.output = self.root / "results"
        self.binary = self.root / "fake-cli"
        self.binary.write_text("""#!/usr/bin/env python3
import csv,os,pathlib,sys
argv=sys.argv[1:]
listing='--list' in argv
if listing: argv.remove('--list')
a=dict(zip(argv[::2],argv[1::2]))
if listing:
 print('schema_version,field,backend,mu,r,c')
 for field in ['Goldilocks','M127']:
  for code in ['Qa','Rs']:
   if field.lower() in a['--fields'].split(',') and code.lower() in a['--backends'].split(','):
    print('hyperfold-pcs-bench-v2,'+field+','+code+',20,2,4')
 sys.exit(0)
count=pathlib.Path(os.environ['HF_TEST_COUNTER'])
n=int(count.read_text())+1 if count.exists() else 1
count.write_text(str(n))
fail=n==int(os.environ.get('HF_TEST_FAIL_AT','0'))
short=n==int(os.environ.get('HF_TEST_SHORT_AT','0'))
p=pathlib.Path(a['--output']).with_suffix('.raw.csv')
with p.open('w',newline='') as f:
 w=csv.writer(f)
 w.writerow(['schema_version','field','backend','mu','r','c','trial_round','repetition','commit_ms','open_ms','verify_ms','proof_size_bytes'])
 for i in range(1 if fail or short else int(a['--repetitions'])):
  value=1+2*i+10*(int(a['--trial-round'])-1)
  w.writerow(['hyperfold-pcs-bench-v2',a['--fields'],a['--backends'],20,2,4,a['--trial-round'],i+1,value,value+100,value+200,12])
sys.exit(7 if fail else 0)
""")
        self.binary.chmod(0o755)
        environment = mock.patch.dict(
            os.environ,
            {
                "HF_TEST_COUNTER": str(self.root / "counter"),
                "HF_TEST_FAIL_AT": "0",
                "HF_TEST_SHORT_AT": "0",
            },
        )
        environment.start()
        self.addCleanup(environment.stop)

    def args(self, **changes):
        args = dict(
            binary=self.binary,
            output_dir=self.output,
            fields="goldilocks,m127",
            backends="qa,rs",
            mu="20",
            r="2",
            c="4",
            repetitions=2,
            rounds=1,
            continue_on_failure=False,
            role="protocol_performance",
        )
        args.update(changes)
        return argparse.Namespace(**args)

    def run_quiet(self, **changes):
        with redirect_stdout(io.StringIO()):
            return bench.run(self.args(**changes))

    def row(self, mu=20, commit="999.123456789"):
        return dict(
            mu=str(mu),
            r="2",
            c="4",
            commit_ms=commit,
            open_ms="888.123456789",
            verify_ms="777.123456789",
            proof_bytes="999",
        )

    def seed(self, field="m127", code="qa", rows=None):
        path = self.output / f"{field}-{code}.csv"
        path.parent.mkdir(exist_ok=True)
        with path.open("w", newline="") as stream:
            writer = csv.DictWriter(stream, bench.COLUMNS, lineterminator="\n")
            writer.writeheader()
            writer.writerows(rows if rows is not None else [self.row()])
        return path

    def test_four_file_dispatch_and_complete_batch_mean(self):
        self.assertEqual(self.run_quiet(), 0)
        self.assertEqual(
            {p.name for p in self.output.iterdir()},
            {"goldilocks-qa.csv", "goldilocks-rs.csv", "m127-qa.csv", "m127-rs.csv"},
        )
        for path in self.output.iterdir():
            self.assertEqual(
                path.read_text().splitlines()[0],
                "mu,r,c,commit_ms,open_ms,verify_ms,proof_bytes",
            )
            rows = bench.read_csv(path)
            self.assertEqual(len(rows), 1)
            self.assertEqual(rows[0]["commit_ms"], "2.000000000")
            self.assertEqual(rows[0]["open_ms"], "102.000000000")
            self.assertEqual(rows[0]["verify_ms"], "202.000000000")
            self.assertEqual(rows[0]["proof_bytes"], "12")

    def test_existing_rows_are_read_and_unselected_values_preserved(self):
        untouched = self.row(mu=22, commit="100.000000123")
        path = self.seed(rows=[untouched, self.row()])
        other = self.seed(field="goldilocks", code="rs")
        other_bytes = other.read_bytes()
        self.assertEqual(self.run_quiet(fields="m127", backends="qa"), 0)
        result = bench.read_csv(path)
        self.assertEqual([r["mu"] for r in result], ["20", "22"])
        self.assertEqual(result[0]["commit_ms"], "2.000000000")
        self.assertEqual(result[1], untouched)
        self.assertEqual(other.read_bytes(), other_bytes)

    def test_all_rounds_use_all_new_samples_not_old_or_fastest_value(self):
        path = self.seed()
        self.assertEqual(self.run_quiet(fields="m127", backends="qa", rounds=2), 0)
        result = bench.read_csv(path)[0]
        self.assertEqual(result["commit_ms"], "7.000000000")  # mean(1,3,11,13)
        self.assertEqual(result["open_ms"], "107.000000000")

    def test_later_round_failure_preserves_entire_old_file(self):
        path = self.seed(rows=[self.row(), self.row(mu=22)])
        before = path.read_bytes()
        os.environ["HF_TEST_FAIL_AT"] = "2"
        self.assertEqual(self.run_quiet(fields="m127", backends="qa", rounds=2), 1)
        self.assertEqual(path.read_bytes(), before)
        self.assertEqual((self.root / "counter").read_text(), "2")
        self.assertEqual(list(self.output.iterdir()), [path])

    def test_failed_new_configuration_is_not_added(self):
        os.environ["HF_TEST_FAIL_AT"] = "1"
        self.assertEqual(self.run_quiet(fields="m127", backends="qa"), 1)
        self.assertFalse(self.output.exists())

    def test_other_complete_configuration_updates_before_later_failure(self):
        first = self.seed(field="goldilocks", code="qa")
        second = self.seed(field="goldilocks", code="rs")
        before = second.read_bytes()
        os.environ["HF_TEST_FAIL_AT"] = "4"
        self.assertEqual(self.run_quiet(rounds=2), 1)
        self.assertEqual(bench.read_csv(first)[0]["commit_ms"], "7.000000000")
        self.assertEqual(second.read_bytes(), before)
        self.assertFalse((self.output / "m127-qa.csv").exists())

    def test_continue_after_failed_batch_keeps_it_and_updates_other_cases(self):
        path = self.seed(field="goldilocks", code="qa")
        before = path.read_bytes()
        os.environ["HF_TEST_FAIL_AT"] = "2"
        self.assertEqual(self.run_quiet(rounds=2, continue_on_failure=True), 1)
        self.assertEqual(path.read_bytes(), before)
        for name in ["goldilocks-rs.csv", "m127-qa.csv", "m127-rs.csv"]:
            self.assertEqual(
                bench.read_csv(self.output / name)[0]["commit_ms"], "7.000000000"
            )

    def test_incomplete_success_exit_and_interrupt_do_not_update(self):
        path = self.seed()
        before = path.read_bytes()
        os.environ["HF_TEST_SHORT_AT"] = "2"
        with self.assertRaises(ValueError):
            self.run_quiet(
                fields="m127", backends="qa", rounds=2, continue_on_failure=True
            )
        self.assertEqual(path.read_bytes(), before)
        os.environ["HF_TEST_SHORT_AT"] = "0"
        original = bench.run_process
        for error in [KeyboardInterrupt, RuntimeError]:
            calls = 0

            def stop_on_second_round(command):
                nonlocal calls
                calls += 1
                if calls == 2:
                    raise error()
                return original(command)

            with mock.patch.object(
                bench, "run_process", side_effect=stop_on_second_round
            ):
                with self.assertRaises(error):
                    self.run_quiet(
                        fields="m127", backends="qa", rounds=2, continue_on_failure=True
                    )
            self.assertEqual(calls, 2)
            self.assertEqual(path.read_bytes(), before)

    def test_smoke_and_synthetic_roles_never_update_formal_tables(self):
        path = self.seed()
        before = path.read_bytes()
        for role in ["functional_smoke", "synthetic_test"]:
            self.assertEqual(self.run_quiet(role=role), 0)
            self.assertEqual(path.read_bytes(), before)
            self.assertEqual(list(self.output.iterdir()), [path])
        new_output = self.root / "not-created"
        self.assertEqual(
            self.run_quiet(role="functional_smoke", output_dir=new_output), 0
        )
        self.assertFalse(new_output.exists())

    def test_atomic_write_error_preserves_old_table_and_cleans_temporary(self):
        path = self.seed()
        before = path.read_bytes()
        with mock.patch.object(
            bench.os, "replace", side_effect=OSError("write failed")
        ):
            with self.assertRaises(OSError):
                bench.update_result(path, self.row(commit="1.0"))
        self.assertEqual(path.read_bytes(), before)
        self.assertEqual(list(self.output.iterdir()), [path])

    def test_process_group_is_reaped_after_interrupt_and_exception(self):
        original_popen = subprocess.Popen
        for error in [KeyboardInterrupt, RuntimeError]:
            pid_path = self.root / error.__name__
            processes = []

            def launch(*a, **kwargs):
                proc = original_popen(*a, **kwargs)
                processes.append(proc)
                wait = proc.wait
                first = True

                def interrupted_wait(*a, **kwargs):
                    nonlocal first
                    if first:
                        first = False
                        deadline = time.monotonic() + 5
                        while not pid_path.exists() and time.monotonic() < deadline:
                            time.sleep(0.01)
                        if not pid_path.exists():
                            raise AssertionError("child did not start")
                        raise error()
                    return wait(*a, **kwargs)

                proc.wait = interrupted_wait
                return proc

            command = [
                sys.executable,
                "-c",
                "import os,pathlib,time;pathlib.Path("
                + repr(str(pid_path))
                + ").write_text(str(os.getpid()));time.sleep(120)",
            ]
            with mock.patch.object(bench.subprocess, "Popen", side_effect=launch):
                with self.assertRaises(error):
                    bench.run_process(command)
            self.assertIsInstance(processes[0].poll(), int)
            pid = int(pid_path.read_text())
            state = Path(f"/proc/{pid}/stat")
            self.assertTrue(not state.exists() or state.read_text().split()[2] == "Z")


if __name__ == "__main__":
    unittest.main()
