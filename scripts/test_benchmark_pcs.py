"""Synthetic CLI and temporary CSV tests; these never measure PCS performance."""

import argparse
import csv
import io
import json
import hashlib
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
 print('schema_version,matrix,field,backend,mu,r,c')
 for field in ['Goldilocks','M127']:
  for code in ['Qa','Rs']:
   if field.lower() in a['--fields'].split(',') and code.lower() in a['--backends'].split(','):
    print('hyperfold-pcs-bench-v3,'+a['--matrix']+','+field+','+code+',20,2,4')
 sys.exit(0)
count=pathlib.Path(os.environ['HF_TEST_COUNTER'])
n=int(count.read_text())+1 if count.exists() else 1
count.write_text(str(n))
fail=n==int(os.environ.get('HF_TEST_FAIL_AT','0'))
short=n==int(os.environ.get('HF_TEST_SHORT_AT','0'))
p=pathlib.Path(a['--output'])
with p.open('w',newline='') as f:
 w=csv.writer(f)
 w.writerow(['schema_version','matrix','field','backend','mu','r','c','trial_round','repetition','commit_ms','open_ms','verify_ms','proof_size_bytes'])
 for i in range(1 if fail or short else int(a['--repetitions'])):
  value=1+2*i+10*(int(a['--trial-round'])-1)
  w.writerow(['hyperfold-pcs-bench-v3',a['--matrix'],a['--fields'],a['--backends'],20,2,4,a['--trial-round'],i+1,value,value+100,value+200,12])
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
            matrix="specialized",
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

    def test_general_results_are_separate(self):
        self.assertEqual(self.run_quiet(matrix="general", fields="m127", backends="qa"), 0)
        self.assertTrue((self.output / "general/m127-qa.csv").exists())
        self.assertFalse((self.output / "m127-qa.csv").exists())

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

            def stop_on_second_round(command, **kwargs):
                nonlocal calls
                if "--list" not in command:
                    calls += 1
                    if calls == 2:
                        raise error()
                return original(command, **kwargs)

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
                    bench.run_process(command, root=self.root, archive=None, prefix=error.__name__)
            self.assertIsInstance(processes[0].poll(), int)
            pid = int(pid_path.read_text())
            state = Path(f"/proc/{pid}/stat")
            self.assertTrue(not state.exists() or state.read_text().split()[2] == "Z")


    def events(self, root):
        return [json.loads(line) for line in (root / "events.jsonl").read_text().splitlines()]

    def test_persistent_success_retains_identity_selection_logs_and_samples(self):
        root = self.root / "archive"
        os.environ["HF_TEST_PRIVATE_VALUE"] = "must-not-be-dumped"
        self.assertEqual(self.run_quiet(raw_dir=root), 0)
        self.assertEqual((root / "bench").read_bytes(), self.binary.read_bytes())
        meta = json.loads((root / "run.json").read_text())
        self.assertEqual(meta["binary_sha256"], hashlib.sha256(self.binary.read_bytes()).hexdigest())
        self.assertEqual(len(meta["affinity"]), 32)
        self.assertEqual(len({(x["socket"], x["core"]) for x in meta["physical_cores"]}), 32)
        self.assertNotIn("must-not-be-dumped", (root / "run.json").read_text())
        selection = bench.read_csv(root / "selection.csv")
        self.assertEqual(len(selection), 4)
        self.assertFalse((root / "selection.stdout.log").exists())
        self.assertTrue((root / "selection.stderr.log").exists())
        events = self.events(root)
        self.assertTrue(any(e["event"] == "process_start" and e["prefix"] == "selection"
                            and "--list" in e["command"] for e in events))
        ends = [e for e in events if e["event"] == "process_end"]
        self.assertEqual(len(ends), 5)  # list-only child is separately recorded
        self.assertTrue(all(e["returncode"] == 0 for e in ends))
        for index in range(4):
            prefix = f"case-{index}-round-1"
            self.assertEqual(len(bench.read_csv(root / (prefix + ".raw.csv"))), 2)
            self.assertFalse((root / (prefix + ".csv")).exists())
            self.assertTrue((root / (prefix + ".stdout.log")).exists())
            self.assertTrue((root / (prefix + ".stderr.log")).exists())
            start = next(e for e in events if e["event"] == "process_start" and e["prefix"] == prefix)
            self.assertIn("--trial-round", start["command"])
            self.assertIn("--output", start["command"])
        del os.environ["HF_TEST_PRIVATE_VALUE"]

    def test_existing_raw_directory_and_dangling_symlink_are_rejected(self):
        root = self.root / "archive"
        root.mkdir()
        (root / "sentinel").write_text("unchanged")
        with self.assertRaises(FileExistsError):
            self.run_quiet(raw_dir=root)
        self.assertEqual(list(root.iterdir()), [root / "sentinel"])
        link = self.root / "link"
        link.symlink_to(self.root / "absent")
        with self.assertRaises(FileExistsError):
            self.run_quiet(raw_dir=link)
        self.assertFalse((self.root / "absent").exists())

    def test_default_still_removes_temporary_binary_and_raw_directory(self):
        targets = []
        original = bench.shutil.copy2
        def copy(source, target):
            targets.append(Path(target))
            return original(source, target)
        with mock.patch.object(bench.shutil, "copy2", side_effect=copy):
            self.assertEqual(self.run_quiet(fields="m127", backends="qa"), 0)
        self.assertEqual(len(targets), 1)
        self.assertFalse(targets[0].parent.exists())

    def test_persistent_later_round_failure_retains_partial_and_old_table(self):
        path = self.seed()
        before = path.read_bytes()
        root = self.root / "archive"
        os.environ["HF_TEST_FAIL_AT"] = "2"
        self.assertEqual(self.run_quiet(fields="m127", backends="qa", rounds=2, raw_dir=root), 1)
        self.assertEqual(path.read_bytes(), before)
        self.assertTrue((root / "case-0-round-2.raw.csv").exists())
        events = self.events(root)
        self.assertTrue(any(e["event"] == "process_end" and e["returncode"] == 7 for e in events))
        self.assertFalse(any(e["event"] == "case_validated" for e in events))

    def test_persistent_short_sample_schema_and_parse_errors_leave_evidence(self):
        original = bench.read_round
        for mode in ("short", "schema", "parse"):
            root = self.root / mode
            self.output = self.root / (mode + "-results")
            path = self.seed()
            before = path.read_bytes()
            def read(path, job, rd, repetitions):
                if mode == "schema":
                    text = path.read_text().replace(bench.SCHEMA, "bad-schema")
                    path.write_text(text)
                if mode == "parse":
                    raise ValueError("synthetic parse error")
                return original(path, job, rd, repetitions)
            os.environ["HF_TEST_SHORT_AT"] = "1" if mode == "short" else "0"
            (self.root / "counter").unlink(missing_ok=True)
            with mock.patch.object(bench, "read_round", side_effect=read):
                with self.assertRaises(ValueError):
                    self.run_quiet(fields="m127", backends="qa", raw_dir=root)
            self.assertEqual(path.read_bytes(), before)
            self.assertTrue((root / "case-0-round-1.raw.csv").exists())

    def test_persistent_unknown_exit_never_validates_even_with_complete_csv(self):
        root = self.root / "archive"
        original = subprocess.Popen
        def launch(command, **kwargs):
            proc = original(command, **kwargs)
            if "--list" not in command:
                wait = proc.wait
                first = True
                def unknown(*a, **kw):
                    nonlocal first
                    code = wait(*a, **kw)
                    if first:
                        first = False
                        return None
                    return code
                proc.wait = unknown
            return proc
        with mock.patch.object(bench.subprocess, "Popen", side_effect=launch):
            self.assertEqual(self.run_quiet(fields="m127", backends="qa", raw_dir=root), 1)
        self.assertEqual(len(bench.read_csv(root / "case-0-round-1.raw.csv")), 2)
        events = self.events(root)
        self.assertTrue(any(e["event"] == "process_end" and e["returncode"] is None for e in events))
        self.assertFalse(any(e["event"] == "case_validated" for e in events))
        self.assertFalse(self.output.exists())

    def test_persistent_interrupt_reaps_and_records_non_success(self):
        root = self.root / "archive"
        original = subprocess.Popen
        children = []
        def launch(command, **kwargs):
            proc = original(command, **kwargs)
            if "--list" not in command:
                children.append(proc)
                wait = proc.wait
                first = True
                def interrupt(*a, **kw):
                    nonlocal first
                    if first:
                        first = False
                        raise KeyboardInterrupt()
                    return wait(*a, **kw)
                proc.wait = interrupt
            return proc
        with mock.patch.object(bench.subprocess, "Popen", side_effect=launch):
            with self.assertRaises(KeyboardInterrupt):
                self.run_quiet(fields="m127", backends="qa", raw_dir=root)
        self.assertIsInstance(children[0].poll(), int)
        events = self.events(root)
        end = next(e for e in events if e["event"] == "process_end" and e["prefix"] == "case-0-round-1")
        self.assertIsNone(end["returncode"])
        self.assertFalse(self.output.exists())

    def test_persistent_launch_failure_is_not_an_unknown_success(self):
        root = self.root / "archive"
        original = subprocess.Popen
        def launch(command, **kwargs):
            if "--list" not in command:
                raise OSError("synthetic launch failure")
            return original(command, **kwargs)
        with mock.patch.object(bench.subprocess, "Popen", side_effect=launch):
            with self.assertRaises(OSError):
                self.run_quiet(fields="m127", backends="qa", raw_dir=root)
        events = self.events(root)
        self.assertTrue(any(e["event"] == "process_start" and e["prefix"] == "case-0-round-1" for e in events))
        self.assertFalse(any(e["event"] == "process_end" and e["prefix"] == "case-0-round-1" for e in events))
        self.assertFalse(self.output.exists())

    def test_archive_write_failure_does_not_fall_back_or_publish(self):
        root = self.root / "archive"
        original = bench.event
        def fail(root, kind, **fields):
            if kind == "process_end" and fields.get("prefix") == "case-0-round-1":
                raise OSError("synthetic archive failure")
            return original(root, kind, **fields)
        with mock.patch.object(bench, "event", side_effect=fail):
            with self.assertRaises(OSError):
                self.run_quiet(fields="m127", backends="qa", raw_dir=root)
        self.assertTrue((root / "case-0-round-1.raw.csv").exists())
        self.assertFalse(self.output.exists())

    def test_insufficient_cores_and_binary_copy_mismatch_fail_before_workload(self):
        root = self.root / "few-cores"
        with mock.patch.object(bench.os, "sched_getaffinity", return_value={0}):
            with self.assertRaises(ValueError):
                self.run_quiet(raw_dir=root)
        original = bench.shutil.copy2
        def corrupt(source, target):
            original(source, target)
            with Path(target).open("ab") as f:
                f.write(b"\n")
        root = self.root / "bad-copy"
        with mock.patch.object(bench.shutil, "copy2", side_effect=corrupt):
            with self.assertRaisesRegex(ValueError, "identity mismatch"):
                self.run_quiet(raw_dir=root)
        self.assertFalse((root / "selection.csv").exists())
        self.assertFalse(self.output.exists())

    def test_optional_certified_identity_is_checked_when_listed(self):
        self.run_quiet(fields="m127", backends="qa", raw_dir=self.root / "archive")
        raw = self.root / "archive/case-0-round-1.raw.csv"
        job = dict(schema_version=bench.SCHEMA, matrix="specialized", field="M127", backend="Qa", mu="20",r="2",c="4",threads="32")
        with self.assertRaisesRegex(ValueError, "threads"):
            bench.read_round(raw, job, 1, 2)


if __name__ == "__main__":
    unittest.main()
