"""Lightweight process/CSV regressions; no PCS timings are run here."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock
import benchmark_pcs as bench


class RunnerTests(unittest.TestCase):
    def setUp(self):
        initial_affinity = os.sched_getaffinity(0)
        self.addCleanup(os.sched_setaffinity, 0, initial_affinity)
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.binary = self.root / "fake-cli"
        self.binary.write_text("""#!/usr/bin/env python3
import csv,os,pathlib,sys
args=dict(zip(sys.argv[1::2],sys.argv[2::2])) if '--list' not in sys.argv else {}
if '--list' in sys.argv:
 print('schema_version,field,backend,mu,r,c,trial_round')
 for field,code in [('Goldilocks','Qa'),('M127','Qa'),('M127','Rs')]:
  print('hyperfold-pcs-bench-v2,'+field+','+code+',20,2,4,1')
 sys.exit(0)
count=pathlib.Path(os.environ['HF_TEST_COUNTER'])
n=int(count.read_text())+1 if count.exists() else 1
count.write_text(str(n))
fail=n==int(os.environ.get('HF_TEST_FAIL_AT','0'))
p=pathlib.Path(args['--output']).with_suffix('.raw.csv')
metrics=['setup_ms','input_ms','commit_ms','forward_release_ms','open_ms','verify_ms','serialization_ms','cleanup_ms','wall_ms','online_total_ms']
with p.open('w',newline='') as f:
 w=csv.writer(f);w.writerow(['schema_version','field','backend','mu','r','c','trial_round','repetition',*metrics,'commitment_bytes','eval_proof_bytes','proof_size_bytes'])
 for i in range(1 if fail else int(args['--repetitions'])):
  w.writerow(['hyperfold-pcs-bench-v2',args['--fields'],args['--backends'],20,2,4,args['--trial-round'],i+1,*([1+2*i]*10),5,7,12])
sys.exit(7 if fail else 0)
""")
        self.binary.chmod(0o755)
        self.environment = mock.patch.dict(
            os.environ,
            {"HF_TEST_COUNTER": str(self.root / "counter"), "HF_TEST_FAIL_AT": "0"},
        )
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def args(self, **changes):
        args = dict(
            binary=self.binary,
            output_dir=self.root / "results",
            role="synthetic_test",
            source_commit="synthetic",
            build_flags="not_applicable",
            compiler="not_applicable",
            fields="goldilocks,m127",
            backends="qa,rs",
            mu="20",
            r="2",
            c="4",
            repetitions=2,
            rounds=1,
            continue_on_failure=False,
        )
        args.update(changes)
        return argparse.Namespace(**args)

    def test_success_and_read_only_statistics(self):
        self.assertEqual(bench.run(self.args()), 0)
        root = self.root / "results"
        rows = bench.read_csv(root / "results.csv")
        self.assertEqual(len(rows), 3)
        self.assertEqual(rows[0]["commit_ms_mean"], "2.000000")
        self.assertAlmostEqual(float(rows[0]["commit_ms_sample_sd"]), 2**0.5, places=5)
        self.assertEqual(
            rows,
            [
                {k: str(v) for k, v in r.items()}
                for r in bench.summarize(bench.read_csv(root / "raw-samples.csv"))
            ],
        )
        metadata = json.loads((root / "metadata.json").read_text())
        self.assertEqual(metadata["attempted_processes"], 3)
        self.assertEqual(metadata["unattempted_processes"], 0)
        self.assertTrue(all(p["exit_code"] == 0 for p in metadata["processes"]))

    def test_first_and_middle_failures_keep_partial_raw_and_blank_metrics(self):
        for failed_at in [1, 2]:
            with self.subTest(failed_at=failed_at):
                os.environ["HF_TEST_FAIL_AT"] = str(failed_at)
                (self.root / "counter").unlink(missing_ok=True)
                output = self.root / f"failure-{failed_at}"
                self.assertEqual(bench.run(self.args(output_dir=output)), 1)
                metadata = json.loads((output / "metadata.json").read_text())
                self.assertEqual(metadata["attempted_processes"], failed_at)
                self.assertEqual(metadata["unattempted_processes"], 3 - failed_at)
                self.assertEqual(metadata["processes"][-1]["exit_code"], 7)
                last = bench.read_csv(output / "results.csv")[-1]
                self.assertEqual(last["commit_ms_mean"], "")
                self.assertEqual(last["proof_size_bytes"], "")
                self.assertEqual(
                    bench.read_csv(output / "raw-samples.csv")[-1]["process_status"],
                    "runtime_failed",
                )
                partial = next(output.glob("cases/*/results.raw.csv"))
                with self.assertRaises(ValueError):
                    bench.summarize(bench.read_csv(partial))

    def test_existing_output_is_untouched(self):
        output = self.root / "results"
        output.mkdir()
        (output / "keep").write_bytes(b"old evidence")
        with self.assertRaises(FileExistsError):
            bench.run(self.args())
        self.assertEqual((output / "keep").read_bytes(), b"old evidence")
        self.assertFalse((self.root / "counter").exists())

    def test_interrupt_does_not_continue_even_when_requested(self):
        for error in [KeyboardInterrupt, RuntimeError]:
            output = self.root / ("stop-" + error.__name__)
            with mock.patch.object(bench, "run_process", side_effect=error):
                with self.assertRaises(error):
                    bench.run(self.args(output_dir=output, continue_on_failure=True))
            metadata = json.loads((output / "metadata.json").read_text())
            self.assertEqual(metadata["attempted_processes"], 1)
            self.assertEqual(metadata["unattempted_processes"], 2)
            self.assertTrue(metadata["termination_reason"].startswith(error.__name__))

    def test_interruption_and_exception_reap_owned_children(self):
        original_popen = subprocess.Popen
        for error in [KeyboardInterrupt, RuntimeError]:
            with self.subTest(error=error):
                directory = self.root / error.__name__
                directory.mkdir()
                pid_path = directory / "pid"

                def launch(*a, **kwargs):
                    proc = original_popen(*a, **kwargs)
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
                observation = {}
                with mock.patch.object(bench.subprocess, "Popen", side_effect=launch):
                    with self.assertRaises(error):
                        bench.run_process(command, directory, observation)
                self.assertIsInstance(observation["exit_code"], int)
                pid = int(pid_path.read_text())
                state = Path(f"/proc/{pid}/stat")
                self.assertTrue(
                    not state.exists() or state.read_text().split()[2] == "Z"
                )

    def test_missing_gnu_resource_file_is_blank(self):
        directory = self.root / "no-rss"
        directory.mkdir()
        exists = Path.exists
        with mock.patch.object(
            Path, "exists", lambda p: False if p.name == "resources.txt" else exists(p)
        ):
            code, rss = bench.run_process([sys.executable, "-c", "pass"], directory)
        self.assertEqual(code, 0)
        self.assertEqual(rss, "")


if __name__ == "__main__":
    unittest.main()
