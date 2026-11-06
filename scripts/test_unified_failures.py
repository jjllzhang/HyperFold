"""Synthetic lifecycle/error evidence regressions; no PCS computation or timing study."""

import csv, io, json, os, pathlib, signal, subprocess, sys, tarfile, tempfile, time, unittest
from unittest import mock
import benchmark_evidence as evidence
import run_unified
import analyze_unified


class MonitorPolicyTests(unittest.TestCase):
    def test_unreadable_policy_stops_and_reaps_the_owned_workload(self):
        for invalid in [False, True]:
            with self.subTest(invalid_json=invalid), tempfile.TemporaryDirectory(
                prefix="policy-test-"
            ) as d:
                root = pathlib.Path(d)
                pidfile = root / "worker.pid"
                policy = root / "policy.json"
                if invalid:
                    policy.write_text("{invalid")
                code = f"import os,time,pathlib;pathlib.Path({str(pidfile)!r}).write_text(str(os.getpid()));time.sleep(60)"
                command = [
                    "/usr/bin/time",
                    "-v",
                    "-o",
                    str(root / "resources.txt"),
                    sys.executable,
                    "-c",
                    code,
                ]

                def allowed():
                    if not pidfile.exists():
                        return True
                    return json.loads(policy.read_text())["enabled"]

                start = time.monotonic()
                status = evidence.monitor_command(command, root, allowed)
                self.assertLess(time.monotonic() - start, 5)
                self.assertEqual(status["stop_reason"], "policy_unreadable")
                self.assertEqual(
                    status["monitor_error"]["type"],
                    "JSONDecodeError" if invalid else "FileNotFoundError",
                )
                self.assertNotEqual(status["exit_code"], 0)
                worker = int(pidfile.read_text())
                with self.assertRaises(ProcessLookupError):
                    os.kill(worker, 0)

    def test_unexpected_monitor_exception_also_reaps_the_session(self):
        with tempfile.TemporaryDirectory(prefix="monitor-error-") as d:
            root = pathlib.Path(d)
            with mock.patch.object(
                evidence,
                "available_bytes",
                side_effect=[2**40, RuntimeError("synthetic observation error")],
            ):
                status = evidence.monitor_command(
                    [sys.executable, "-c", "import time;time.sleep(60)"],
                    root,
                    lambda: True,
                )
            self.assertEqual(status["stop_reason"], "monitor_error")
            self.assertIsNotNone(status["exit_code"])
            with self.assertRaises(ProcessLookupError):
                os.kill(status["workload_pid"], 0)


class StoppedRunArchiveTests(unittest.TestCase):
    def test_default_failure_has_analyzable_attempted_prefix(self):
        repo = pathlib.Path(__file__).resolve().parents[1]
        with (repo / "data/m127-parameters.csv").open() as source:
            params = list(csv.DictReader(source))
        header = "schema_version,field,backend,mu,r,c,mu0,depth,extension_degree,queries,threads,trial_round,first_tree_strategy,parameter_lifecycle,witness_lifecycle,setup_per_trial"
        lines = [header]
        for code in ["QA_units", "RS"]:
            r = next(
                x
                for x in params
                if x["backend"] == code
                and x["mu"] == "20"
                and x["r"] == "2"
                and x["c"] == "4"
            )
            lines.append(
                ",".join(
                    [
                        run_unified.SCHEMA,
                        "M127",
                        "Qa" if code == "QA_units" else "Rs",
                        "20",
                        "2",
                        "4",
                        r["mu0"],
                        r["depth"],
                        "2",
                        r["queries"],
                        "32",
                        "1",
                        "prehashed_recovered",
                        "opening_only",
                        "consumed_after_first_round",
                        "true",
                    ]
                )
            )
        listing = "\n".join(lines) + "\n"
        with tempfile.TemporaryDirectory(prefix="synthetic-stopped-run-") as d:
            root = pathlib.Path(d)
            binary = root / "fake-bench"
            binary.write_text(
                "#!"
                + sys.executable
                + '\nimport sys\nif "--list" in sys.argv:print('
                + repr(listing)
                + ',end="")\nelse:sys.exit(7)\n'
            )
            binary.chmod(0o755)
            evidence_root = root / "data"

            def command_output(cmd):
                if "status" in cmd:
                    return ""
                if "rev-parse" in cmd:
                    return "f" * 40
                return "synthetic unit fixture; not performance data"

            argv = [
                "run_unified",
                "--binary",
                str(binary),
                "--output-dir",
                str(evidence_root),
                "--fields",
                "m127",
                "--backends",
                "qa,rs",
                "--mu",
                "20",
                "--r",
                "2",
                "--c",
                "4",
                "--repetitions",
                "3",
            ]
            with mock.patch.object(sys, "argv", argv), mock.patch.object(
                run_unified, "command_output", side_effect=command_output
            ), mock.patch.object(run_unified.os, "sched_setaffinity"):
                with self.assertRaisesRegex(RuntimeError, "incomplete"):
                    run_unified.main()
            finish = json.loads((evidence_root / "finished.json").read_text())
            self.assertEqual(
                (
                    finish["planned_processes"],
                    finish["attempted_processes"],
                    finish["unattempted_processes"],
                ),
                (2, 1, 1),
            )
            self.assertEqual(finish["run_status"], "stopped")
            self.assertEqual(finish["termination_reason"], "worker_failed")
            outcomes = evidence.rows(evidence_root / "results.csv")
            self.assertEqual(len(outcomes), 1)
            self.assertEqual(outcomes[0]["status"], "runtime_failed")
            self.assertEqual(outcomes[0]["proof_size_bytes"], "")
            for name in run_unified.METRICS:
                self.assertEqual(outcomes[0][name + "_mean"], "")
                self.assertEqual(outcomes[0][name + "_sample_sd"], "")
            report = root / "report"
            rebuilt = root / "rebuilt"
            with mock.patch.object(
                sys,
                "argv",
                [
                    "analyze_unified",
                    "--root",
                    str(evidence_root),
                    "--output",
                    str(report),
                ],
            ), mock.patch("sys.stdout", new_callable=io.StringIO):
                analyze_unified.main()
            with mock.patch.object(
                sys,
                "argv",
                [
                    "analyze_unified",
                    "--archive",
                    str(report / "raw-evidence.tar.gz"),
                    "--output",
                    str(rebuilt),
                ],
            ), mock.patch("sys.stdout", new_callable=io.StringIO):
                analyze_unified.main()
            for name in ["results.csv", "raw-samples.csv"]:
                self.assertEqual(
                    (report / name).read_bytes(), (rebuilt / name).read_bytes()
                )
            self.assertIn("1 unattempted", (report / "REPORT.md").read_text())


if __name__ == "__main__":
    unittest.main()
