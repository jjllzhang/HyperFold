#!/usr/bin/env python3
"""Update four PCS result tables after complete, independent 32-thread batches."""

import argparse
import csv
from contextlib import ExitStack
from datetime import datetime, timezone
import hashlib
import io
import math
import json
import os
from pathlib import Path
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile

SCHEMA = "hyperfold-pcs-bench-v3"  # Internal Rust CLI input, not the result table.
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
    if job.get("matrix", "specialized") == "general":
        root = root / "general"
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


def utc_now():
    return datetime.now(timezone.utc).isoformat()


def event(root, kind, **fields):
    """Append an archive record when persistent evidence is requested."""
    if root is not None:
        with (root / "events.jsonl").open("a") as stream:
            stream.write(json.dumps(dict(event=kind, utc=utc_now(), **fields)) + "\n")


def memory_snapshot():
    keys = {"MemTotal", "MemAvailable", "SwapTotal", "SwapFree"}
    return {
        key: value.strip()
        for line in Path("/proc/meminfo").read_text().splitlines()
        for key, value in [line.split(":", 1)]
        if key in keys
    }


def reap_group(proc):
    """Reap only the new session created for this workload."""
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


def run_process(command, *, root, archive, prefix, stdout_name=None):
    """Capture process evidence and always clean up its session."""
    with ExitStack() as stack:
        stdout = stack.enter_context((root / (stdout_name or f"{prefix}.stdout.log")).open("x"))
        stderr = stack.enter_context((root / f"{prefix}.stderr.log").open("x"))
        event(archive, "process_start", prefix=prefix, command=command)
        proc = subprocess.Popen(command, stdout=stdout, stderr=stderr, start_new_session=True)
        code = None
        try:
            code = proc.wait()
        finally:
            reap_group(proc)
            event(archive, "process_end", prefix=prefix, returncode=code)
        return code


def read_round(path, job, round_number, repetitions):
    rows = read_csv(path)
    if len(rows) != repetitions or {int(row["repetition"]) for row in rows} != set(
        range(1, repetitions + 1)
    ):
        raise ValueError("incomplete formal repetitions")
    for row in rows:
        if (
            row["schema_version"] != SCHEMA
            or row["matrix"] != job["matrix"]
            or config_key(row) != config_key(job)
            or row["field"].lower() != job["field"].lower()
            or row["backend"].lower() != job["backend"].lower()
            or int(row["trial_round"]) != round_number
        ):
            raise ValueError("unexpected benchmark case or schema")
        for key in ("mu0", "depth", "extension_degree", "queries", "threads",
                    "first_tree_strategy", "parameter_lifecycle", "witness_lifecycle",
                    "setup_per_trial"):
            if key in job and row.get(key) != job[key]:
                raise ValueError(f"unexpected benchmark identity: {key}")
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


def execute(args, root, archive):
    physical = {}
    for cpu in sorted(os.sched_getaffinity(0)):
        topology = Path(f"/sys/devices/system/cpu/cpu{cpu}/topology")
        core = (
            (topology / "physical_package_id").read_text().strip(),
            (topology / "core_id").read_text().strip(),
        )
        physical.setdefault(core, cpu)
    if len(physical) < 32:
        raise ValueError("benchmark requires at least32 distinct physical cores")
    selected = list(physical.values())[:32]
    os.sched_setaffinity(0, selected)
    if set(os.sched_getaffinity(0)) != set(selected):
        raise ValueError("benchmark affinity was not applied")
    binary = root / "bench"
    source_sha = hashlib.sha256(args.binary.resolve().read_bytes()).hexdigest()
    shutil.copy2(args.binary.resolve(), binary)
    binary_sha = hashlib.sha256(binary.read_bytes()).hexdigest()
    event(archive, "binary_copied", source=str(args.binary.resolve()),
          source_sha256=source_sha, binary_sha256=binary_sha)
    if binary_sha != source_sha:
        raise ValueError("copied binary identity mismatch")
    if archive is not None:
        metadata = dict(
            schema="hyperfold-pcs-run-v1", start_utc=utc_now(),
            binary_source=str(args.binary.resolve()),
            binary_sha256=binary_sha,
            binary_bytes=binary.stat().st_size,
            runner_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            affinity=selected,
            physical_cores=[dict(socket=socket, core=core, cpu=cpu)
                            for (socket, core), cpu in physical.items() if cpu in selected],
            output_dir=str(args.output_dir.resolve()), role=args.role,
            rounds=args.rounds, repetitions=args.repetitions, warmups_per_process=1,
            memory=memory_snapshot(),
            build_provenance="binary SHA recorded; source-to-build provenance not automatically verified",
        )
        (root / "run.json").write_text(json.dumps(metadata, indent=2) + "\n")
    selection = [
        "--threads", "32", "--fields", args.fields, "--backends", args.backends,
        "--mu", args.mu, "--r", args.r, "--c", args.c,
        "--matrix", args.matrix,
    ]
    code = run_process([str(binary), "--list", *selection], root=root,
                       archive=archive, prefix="selection", stdout_name="selection.csv")
    if code != 0:
        raise ValueError(f"selection process failed: exit{code}")
    listing = (root / "selection.csv").read_text()
    jobs = list(csv.DictReader(io.StringIO(listing)))
    jobs.sort(key=lambda j: (int(j["mu"]), j["field"].lower(), int(j["r"]),
                            int(j["c"]), j["backend"].lower()))
    keys = [(j["field"].lower(), j["backend"].lower(), *config_key(j)) for j in jobs]
    if not jobs or len(set(keys)) != len(jobs):
        raise ValueError("empty or duplicate configuration selection")
    failed = False
    for index, job in enumerate(jobs):
        path = result_path(args.output_dir, job)
        samples = []
        for round_number in range(1, args.rounds + 1):
            prefix = f"case-{index}-round-{round_number}"
            output = root / f"{prefix}.raw.csv"
            command = [
                str(binary), "--fields", job["field"].lower(),
                "--backends", job["backend"].lower(), "--mu", job["mu"],
                "--r", job["r"], "--c", job["c"], "--matrix", job["matrix"], "--threads", "32",
                "--trial-round", str(round_number),
                "--repetitions", str(args.repetitions), "--output", str(output),
            ]
            code = run_process(command, root=root, archive=archive, prefix=prefix)
            if code != 0:
                print(f"benchmark failed: {path.name} {config_key(job)}, "
                      f"round{round_number}, exit{code}", file=sys.stderr)
                failed = True
                break
            samples.extend(read_round(output, job,
                                      round_number, args.repetitions))
        else:
            result = batch_result(job, samples)
            if args.role == "protocol_performance":
                update_result(path, result)
                print(f"updated {path.name}: {config_key(job)}", flush=True)
            continue
        if not args.continue_on_failure:
            break
    return 1 if failed else 0


def run(args):
    raw_dir = getattr(args, "raw_dir", None)
    with ExitStack() as stack:
        if raw_dir is None:
            root = Path(stack.enter_context(tempfile.TemporaryDirectory(
                prefix="hyperfold-benchmark-")))
            archive = None
        else:
            provided = Path(raw_dir)
            if provided.exists() or provided.is_symlink():
                raise FileExistsError(f"raw directory must be new: {provided}")
            root = provided.resolve()
            root.mkdir(parents=True, exist_ok=False)
            archive = root
        return execute(args, root, archive)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/bench")
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "results",
        help="read/update the four field/code CSVs here",
    )
    parser.add_argument(
        "--raw-dir", type=Path,
        help="new, previously absent directory retaining copied binary, samples and logs",
    )
    parser.add_argument("--matrix", choices=["specialized", "general"], default="specialized")
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
    except (OSError, ValueError) as error:
        print(f"benchmark error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
