#!/usr/bin/env python3
"""Update four PCS result tables after complete, independent 32-thread batches."""

import argparse
import csv
import io
import math
import os
from pathlib import Path
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile

SCHEMA = "hyperfold-pcs-bench-v2"  # Internal Rust CLI input, not the result table.
COLUMNS = ["mu", "r", "c", "commit_ms", "open_ms", "verify_ms", "proof_bytes"]
METRICS = COLUMNS[3:6]
ROOT = Path(__file__).resolve().parents[1]


def read_csv(path, columns=None):
    with Path(path).open(newline="") as stream:
        reader = csv.DictReader(stream)
        if columns is not None and reader.fieldnames != columns:
            raise ValueError(f"unexpected result header: {path}")
        return list(reader)


def config_key(row):
    return tuple(int(row[key]) for key in COLUMNS[:3])


def result_path(root, job):
    field, code = job["field"].lower(), job["backend"].lower()
    if field not in ("goldilocks", "m127") or code not in ("qa", "rs"):
        raise ValueError("unknown field/code in benchmark selection")
    return root / f"{field}-{code}.csv"


def update_result(path, result):
    """Replace one configuration atomically, preserving all other value strings."""
    records = {}
    if path.exists():
        for row in read_csv(path, COLUMNS):
            key = config_key(row)
            if key in records:
                raise ValueError(f"duplicate configuration in {path}: {key}")
            records[key] = row
    records[config_key(result)] = result
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w", newline="", dir=path.parent, prefix=f".{path.name}.", delete=False
        ) as stream:
            temporary = Path(stream.name)
            writer = csv.DictWriter(stream, COLUMNS, lineterminator="\n")
            writer.writeheader()
            writer.writerows(records[key] for key in sorted(records))
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def run_process(command):
    """Reap this workload's process group on completion, failure or interruption."""
    proc = subprocess.Popen(command, start_new_session=True)
    try:
        return proc.wait()
    finally:
        try:
            os.killpg(proc.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait()
        finally:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass


def read_round(path, job, round_number, repetitions):
    rows = read_csv(path)
    if len(rows) != repetitions or {int(row["repetition"]) for row in rows} != set(
        range(1, repetitions + 1)
    ):
        raise ValueError("incomplete formal repetitions")
    for row in rows:
        if (
            row["schema_version"] != SCHEMA
            or config_key(row) != config_key(job)
            or row["field"].lower() != job["field"].lower()
            or row["backend"].lower() != job["backend"].lower()
            or int(row["trial_round"]) != round_number
        ):
            raise ValueError("unexpected benchmark case or schema")
    return rows


def batch_result(job, samples):
    result = {key: str(int(job[key])) for key in COLUMNS[:3]}
    for metric in METRICS:
        values = [float(row[metric]) for row in samples]
        if any(not math.isfinite(value) or value < 0 for value in values):
            raise ValueError("invalid measured time")
        result[metric] = f"{statistics.fmean(values):.9f}"
    sizes = {int(row["proof_size_bytes"]) for row in samples}
    if len(sizes) != 1 or next(iter(sizes)) <= 0:
        raise ValueError("expected one positive proof size for this fixed-input batch")
    result["proof_bytes"] = str(next(iter(sizes)))
    return result


def run(args):
    physical = {}
    for cpu in sorted(os.sched_getaffinity(0)):
        topology = Path(f"/sys/devices/system/cpu/cpu{cpu}/topology")
        core = (
            (topology / "physical_package_id").read_text(),
            (topology / "core_id").read_text(),
        )
        physical.setdefault(core, cpu)
    os.sched_setaffinity(0, list(physical.values())[:32])
    failed = False
    with tempfile.TemporaryDirectory(prefix="hyperfold-benchmark-") as temporary:
        root = Path(temporary)
        binary = root / "bench"
        shutil.copy2(args.binary.resolve(), binary)
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
        jobs = list(csv.DictReader(io.StringIO(listing)))
        for index, job in enumerate(jobs):
            path = result_path(args.output_dir, job)
            samples = []
            for round_number in range(1, args.rounds + 1):
                output = root / f"case-{index}-round-{round_number}.csv"
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
                    str(round_number),
                    "--repetitions",
                    str(args.repetitions),
                    "--output",
                    str(output),
                ]
                code = run_process(command)
                if code:
                    print(
                        f"benchmark failed: {path.name} {config_key(job)}, round {round_number}, exit {code}",
                        file=sys.stderr,
                    )
                    failed = True
                    break
                samples.extend(
                    read_round(
                        output.with_suffix(".raw.csv"),
                        job,
                        round_number,
                        args.repetitions,
                    )
                )
            else:
                result = batch_result(job, samples)
                if args.role == "protocol_performance":
                    update_result(path, result)
                    print(f"updated {path.name}: {config_key(job)}")
                continue
            if not args.continue_on_failure:
                break
    return 1 if failed else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/bench")
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "results",
        help="read/update the four field/code CSVs here",
    )
    parser.add_argument("--fields", default="goldilocks,m127")
    parser.add_argument("--backends", default="qa,rs")
    parser.add_argument("--mu", default="20..=30")
    parser.add_argument("--r", default="2,4,8")
    parser.add_argument("--c", default="2,4")
    parser.add_argument("--repetitions", type=int, default=3)
    parser.add_argument("--rounds", type=int, default=1)
    parser.add_argument("--continue-on-failure", action="store_true")
    parser.add_argument(
        "--role",
        choices=["protocol_performance", "functional_smoke", "synthetic_test"],
        default="protocol_performance",
        help="only protocol_performance updates result files",
    )
    args = parser.parse_args()
    if args.repetitions < 1 or args.rounds < 1:
        parser.error("positive repetitions and rounds required")
    try:
        return run(args)
    except KeyboardInterrupt:
        print("benchmark interrupted", file=sys.stderr)
        return 130
    except Exception as error:
        print(f"benchmark error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
