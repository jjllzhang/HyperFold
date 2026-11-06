#!/usr/bin/env python3
"""Run independent 32-thread PCS cases, or summarize v2 raw CSV without running them."""

import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import statistics
import subprocess
import tempfile
from datetime import datetime, timezone

SCHEMA = "hyperfold-pcs-bench-v2"
METRICS = (
    "setup_ms",
    "input_ms",
    "commit_ms",
    "forward_release_ms",
    "open_ms",
    "verify_ms",
    "serialization_ms",
    "cleanup_ms",
    "wall_ms",
    "online_total_ms",
)


def read_csv(path):
    with Path(path).open(newline="") as f:
        return list(csv.DictReader(f))


def write_csv(path, records):
    keys = list(dict.fromkeys(k for row in records for k in row))
    with Path(path).open("w", newline="") as f:
        if keys:
            writer = csv.DictWriter(f, keys)
            writer.writeheader()
            writer.writerows(records)


def summarize(records):
    """Group complete raw repetitions by case; failed processes have no aggregates."""
    groups = {}
    for row in records:
        if row["schema_version"] != SCHEMA:
            raise ValueError("expected v2 benchmark CSV")
        if "process_status" not in row:
            raise ValueError(
                "use the runner's combined raw CSV with process exit-status labels"
            )
        if row["process_status"] != "measured":
            continue
        identity = {
            k: v
            for k, v in row.items()
            if k not in METRICS
            and k
            not in (
                "repetition",
                "process_status",
                "commitment_bytes",
                "eval_proof_bytes",
                "proof_size_bytes",
            )
        }
        groups.setdefault(tuple(identity.items()), []).append(row)
    output = []
    for key, rows in groups.items():
        result = dict(key, status="measured", measured_repetitions=len(rows))
        for metric in METRICS:
            values = [float(row[metric]) for row in rows]
            result[metric + "_mean"] = f"{statistics.fmean(values):.6f}"
            result[metric + "_sample_sd"] = (
                f"{statistics.stdev(values) if len(values) > 1 else 0:.6f}"
            )
        for field in ("commitment_bytes", "eval_proof_bytes", "proof_size_bytes"):
            result[field] = rows[-1][field]
        output.append(result)
    return output


def run_process(command, directory, observation=None):
    """GNU time observes RSS; all interruption paths reap our own process group."""
    resource = directory / "resources.txt"
    proc = None
    try:
        with (directory / "stdout.log").open("w") as stdout, (
            directory / "stderr.log"
        ).open("w") as stderr:
            proc = subprocess.Popen(
                [
                    "/usr/bin/time",
                    "-f",
                    "peak_rss_kib=%M",
                    "-o",
                    str(resource),
                    *command,
                ],
                stdout=stdout,
                stderr=stderr,
                start_new_session=True,
            )
            returncode = proc.wait()
    finally:
        if proc is not None:
            # A nonzero leader exit can leave descendants alive as well.
            try:
                os.killpg(proc.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
                proc.wait()
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        text = resource.read_text() if resource.exists() else ""
        match = re.search(r"^peak_rss_kib=(\d+)$", text, re.MULTILINE)
        rss = int(match[1]) if match else ""
        if observation is not None and proc is not None:
            observation.update(exit_code=proc.returncode, peak_rss_kib=rss)
    return returncode, rss


def run(args):
    root = args.output_dir.resolve()
    root.mkdir(parents=True)  # An existing directory is never reused.
    checkout = Path(__file__).resolve().parents[1]
    physical = {}
    for cpu in sorted(os.sched_getaffinity(0)):
        topology = Path(f"/sys/devices/system/cpu/cpu{cpu}/topology")
        core = (
            (topology / "physical_package_id").read_text(),
            (topology / "core_id").read_text(),
        )
        physical.setdefault(core, cpu)
    os.sched_setaffinity(0, list(physical.values())[:32])
    metadata = {
        "schema_version": SCHEMA,
        "role": args.role,
        "started_utc": datetime.now(timezone.utc).isoformat(),
        "source_commit": args.source_commit,
        "build_flags": args.build_flags,
        "compiler": args.compiler,
        "machine": platform.platform(),
        "cpu_model": next(
            (
                line.split(":", 1)[1].strip()
                for line in Path("/proc/cpuinfo").read_text().splitlines()
                if line.startswith("model name")
            ),
            "unknown",
        ),
        "cpu_affinity": sorted(os.sched_getaffinity(0)),
        "exclusive_host": False,
        "threads": 32,
        "warmup": 1,
        "repetitions": args.repetitions,
        "rounds": args.rounds,
        "seeds": {"setup": 42, "message": 27, "point": 28},
        "certificate": "hfc-tight-total100-gold-m127-v3-2026-11-05",
        "setup": "fresh_per_trial",
        "parameters": "forward_released_after_commit",
        "witness": "consumed_after_first_lifted_fold",
        "first_tree": "query_row_recovery",
        "proof_size": "commitment_plus_evaluation_proof",
        "online_scope": "commit+forward_release+open+verify",
        "wall_scope": "fresh Setup/input/commit/release/open/verify/serialization/cleanup; CSV IO excluded",
        "parameter_sha256": {
            p.name: hashlib.sha256(p.read_bytes()).hexdigest()
            for p in (checkout / "data").glob("*.csv")
        },
        "processes": [],
        "planned_processes": 0,
        "termination_reason": None,
    }
    summaries, raw_all, jobs = [], [], []
    try:
        with tempfile.TemporaryDirectory(prefix="hyperfold-benchmark-") as temporary:
            binary = Path(temporary) / "bench"
            shutil.copy2(args.binary.resolve(), binary)
            metadata["binary_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
            selection = [
                "--threads",
                "32",
                "--fields",
                args.fields,
                "--backends",
                args.backends,
                "--mu",
                args.mu,
                "--r",
                args.r,
                "--c",
                args.c,
            ]
            listing = subprocess.check_output(
                [str(binary), "--list", *selection], text=True
            )
            (root / "selected.csv").write_text(listing)
            selected = read_csv(root / "selected.csv")
            jobs = [
                dict(row, trial_round=str(n + 1))
                for n in range(args.rounds)
                for row in selected
            ]
            metadata["planned_processes"] = len(jobs)
            for job in jobs:
                directory = (
                    root
                    / "cases"
                    / f'{job["field"]}-{job["backend"]}-mu{job["mu"]}-r{job["r"]}-c{job["c"]}-round{job["trial_round"]}'
                )
                directory.mkdir(parents=True)
                output = directory / "results.csv"
                command = [
                    str(binary),
                    "--fields",
                    job["field"].lower(),
                    "--backends",
                    job["backend"].lower(),
                    "--mu",
                    job["mu"],
                    "--r",
                    job["r"],
                    "--c",
                    job["c"],
                    "--threads",
                    "32",
                    "--trial-round",
                    job["trial_round"],
                    "--repetitions",
                    str(args.repetitions),
                    "--output",
                    str(output),
                ]
                entry = dict(
                    job,
                    command=command,
                    status="runtime_failed",
                    exit_code="",
                    peak_rss_kib="",
                )
                metadata["processes"].append(entry)
                case_raw = []
                raw = output.with_suffix(".raw.csv")
                try:
                    code, rss = run_process(command, directory, entry)
                    entry.update(exit_code=code, peak_rss_kib=rss)
                    case_raw = read_csv(raw) if raw.exists() else []
                    if code == 0:
                        if len(case_raw) != args.repetitions:
                            raise ValueError(
                                "incomplete raw repetitions after successful exit"
                            )
                        summaries.extend(
                            summarize(
                                [
                                    dict(row, process_status="measured")
                                    for row in case_raw
                                ]
                            )
                        )
                        entry["status"] = "measured"
                finally:
                    if not case_raw and raw.exists():
                        case_raw = read_csv(raw)
                    raw_all.extend(
                        dict(row, process_status=entry["status"]) for row in case_raw
                    )
                    if entry["status"] != "measured":
                        failed = dict(
                            job, status="runtime_failed", measured_repetitions=0
                        )
                        failed.update(
                            {
                                f"{m}_{stat}": ""
                                for m in METRICS
                                for stat in ("mean", "sample_sd")
                            }
                        )
                        failed.update(
                            commitment_bytes="",
                            eval_proof_bytes="",
                            proof_size_bytes="",
                        )
                        summaries.append(failed)
                if entry["status"] != "measured" and not args.continue_on_failure:
                    metadata["termination_reason"] = "process_failed"
                    break
    except BaseException as error:
        metadata["termination_reason"] = type(error).__name__ + ": " + str(error)
        raise
    finally:
        metadata["attempted_processes"] = len(metadata["processes"])
        metadata["unattempted_processes"] = len(jobs) - len(metadata["processes"])
        metadata["finished_utc"] = datetime.now(timezone.utc).isoformat()
        write_csv(root / "results.csv", summaries)
        write_csv(root / "raw-samples.csv", raw_all)
        (root / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    return 1 if any(p["status"] != "measured" for p in metadata["processes"]) else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--summarize",
        type=Path,
        help="read the runner combined raw CSV and print process summaries; no workload",
    )
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--fields", default="goldilocks,m127")
    parser.add_argument("--backends", default="qa,rs")
    parser.add_argument("--mu", default="20..=30")
    parser.add_argument("--r", default="2,4,8")
    parser.add_argument("--c", default="2,4")
    parser.add_argument("--repetitions", type=int, default=3)
    parser.add_argument("--rounds", type=int, default=1)
    parser.add_argument("--continue-on-failure", action="store_true")
    parser.add_argument("--role", default="protocol_performance")
    parser.add_argument(
        "--source-commit",
        default="unknown",
        help="source of the supplied binary, not inferred from its path",
    )
    parser.add_argument("--build-flags", default="unknown")
    parser.add_argument(
        "--compiler",
        default="unknown",
        help="compiler used to build the supplied binary",
    )
    args = parser.parse_args()
    if args.summarize:
        import sys

        rows = summarize(read_csv(args.summarize))
        if rows:
            writer = csv.DictWriter(sys.stdout, list(rows[0]))
            writer.writeheader()
            writer.writerows(rows)
        return 0
    if (
        not args.binary
        or not args.output_dir
        or args.repetitions < 1
        or args.rounds < 1
    ):
        parser.error(
            "binary, new output directory and positive repetitions/rounds required"
        )
    return run(args)


if __name__ == "__main__":
    raise SystemExit(main())
