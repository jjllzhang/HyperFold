#!/usr/bin/env python3
"""Run each certified case in isolation and preserve commands, RSS, and raw samples.

Linux, Python 3.11+, GNU time. Sequential by design. Stop on failed cases.
Memory exclusions require --allow-memory-skips and are never reported as measured.
Resume only in the same directory with the same binary hash and arguments.
"""
import argparse
import csv
import datetime
import hashlib
import io
import json
import os
import pathlib
import platform
import shutil
import signal
import subprocess
import time


def utc():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def sha(path):
    with open(path, "rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def rows(path):
    with open(path, newline="") as stream:
        return list(csv.DictReader(stream))


def command_output(command):
    return subprocess.check_output(command, text=True).strip()


def available_bytes():
    for line in pathlib.Path("/proc/meminfo").read_text().splitlines():
        if line.startswith("MemAvailable:"):
            return int(line.split()[1]) * 1024
    raise RuntimeError("MemAvailable is required for the live memory guard")


def write_table(path, entries):
    if not entries:
        return
    temp = path.with_suffix(".tmp")
    with open(temp, "w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(entries[0]))
        writer.writeheader()
        writer.writerows(entries)
    temp.replace(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--max-estimated-gib", type=float, default=64)
    parser.add_argument("--min-available-gib", type=float, default=12)
    parser.add_argument("--repetitions", type=int, default=3)
    parser.add_argument("--memory-mode", choices=["auto", "retained", "low"], default="auto")
    parser.add_argument("--allow-memory-skips", action="store_true")
    parser.add_argument("--reuse-completed-from", type=pathlib.Path,
                        help="copy compatible completed cases into a NEW output directory")
    args = parser.parse_args()
    root = pathlib.Path(__file__).resolve().parents[1]
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    binary = args.binary.resolve()
    contract = {"binary_sha256": sha(binary), "threads": 32,
                "repetitions": args.repetitions, "max_estimated_gib": args.max_estimated_gib,
                "min_available_gib": args.min_available_gib, "memory_mode": args.memory_mode,
                "allow_memory_skips": args.allow_memory_skips}
    manifest = output / "manifest.json"
    if manifest.exists():
        existing = json.loads(manifest.read_text())
        if existing["contract"] != contract:
            raise RuntimeError("resume contract differs; use a new output directory")
    else:
        shutil.copy2(binary, output / "bench")
        info = {"contract": contract, "started_utc": utc(), "platform": platform.platform(),
                "source_commit": command_output(["git", "-C", str(root), "rev-parse", "HEAD"]),
                "source_status": command_output(["git", "-C", str(root), "status", "--short"]),
                "plonky3_commit": "0d803737b681774186cd283b1fd2d8e493655c82",
                "cargo_lock_sha256": sha(root / "Cargo.lock"),
                "rustc": command_output(["rustc", "-Vv"]),
                "cargo": command_output(["cargo", "-V"]),
                "lscpu": command_output(["lscpu"]),
                "meminfo": pathlib.Path("/proc/meminfo").read_text(),
                "rustflags": os.environ.get("RUSTFLAGS", ""),
                "build_command": "RUSTFLAGS='-C target-cpu=native' cargo build --offline --locked --release --bin bench",
                "setup_seed_byte": 42, "message_seed_byte": 27, "point_seed_byte": 28,
                "script_argv": __import__("sys").argv}
        if args.reuse_completed_from:
            prior = args.reuse_completed_from.resolve()
            old = json.loads((prior / "manifest.json").read_text())
            for key in ["binary_sha256", "threads", "repetitions", "max_estimated_gib", "min_available_gib"]:
                if old["contract"][key] != contract[key]:
                    raise RuntimeError(f"incompatible reused contract: {key}")
            if args.memory_mode != "retained":
                raise RuntimeError("policy migration currently supports retained-only destination")
            imported = []
            for status in sorted((prior / "cases").glob("*/status.json")):
                state = json.loads(status.read_text())
                if state.get("exit_code") != 0 or state.get("memory_guard"):
                    continue
                source = status.parent
                means = rows(source / "results.csv")
                samples = rows(source / "results.raw.csv")
                if len(means) != 1 or means[0]["status"] != "measured":
                    continue
                if len(samples) != args.repetitions or means[0]["low_memory"] != "false":
                    raise RuntimeError(f"cannot reuse non-retained or incomplete case {source.name}")
                if any(sample["low_memory"] != "false" for sample in samples):
                    raise RuntimeError(f"non-retained raw sample {source.name}")
                destination = output / "cases" / source.name
                shutil.copytree(source, destination)
                (destination / "reuse.json").write_text(json.dumps({
                    "source_directory": str(source), "imported_utc": utc(),
                    "actual_low_memory": False,
                    "reason": "same binary and measurement contract; auto previously selected retained"},
                    indent=2) + "\n")
                imported.append(source.name)
            info["reused_from"] = str(prior)
            info["reused_retained_cases"] = imported
        manifest.write_text(json.dumps(info, indent=2) + "\n")
    pinned = output / "bench"
    if sha(pinned) != contract["binary_sha256"]:
        raise RuntimeError("pinned binary differs")
    cases = list(csv.DictReader(io.StringIO(command_output([
        str(pinned), "--list", "--memory-mode", args.memory_mode, "--max-estimated-gib",
        str(args.max_estimated_gib)]))))
    assert len(cases) == 144
    # Increasing size, with field and arity/rate kept adjacent for inspection.
    cases.sort(key=lambda x: (int(x["mu"]), x["field"], int(x["r"]), int(x["c"]), x["backend"]))
    all_means, all_raw = [], []
    progress = output / "progress.json"
    for index, case in enumerate(cases):
        key = "-".join(case[k] for k in ["field", "backend", "mu", "r", "c"])
        directory = output / "cases" / key
        directory.mkdir(parents=True, exist_ok=True)
        result = directory / "results.csv"
        status = directory / "status.json"
        completed = status.exists() and json.loads(status.read_text()).get("exit_code") == 0
        if not completed:
            if available_bytes() < args.min_available_gib * 2**30:
                raise RuntimeError("insufficient available memory before next case")
            command = ["/usr/bin/time", "-v", "-o", str(directory / "resources.txt"), str(pinned),
                       "--threads", "32", "--mu", case["mu"], "--fields", case["field"].lower(),
                       "--backends", case["backend"].lower(), "--r", case["r"], "--c", case["c"],
                       "--repetitions", str(args.repetitions), "--memory-mode", args.memory_mode,
                       "--max-estimated-gib", str(args.max_estimated_gib), "--output", str(result)]
            (directory / "command.json").write_text(json.dumps(command, indent=2) + "\n")
            started = utc()
            progress.write_text(json.dumps({"completed": sum(m["status"] == "measured" for m in all_means),
                                           "processed": len(all_means), "total": 144,
                                           "current": key, "started_utc": started}) + "\n")
            print(f"{started} {index+1}/144 {key}", flush=True)
            memory_guard = False
            minimum_available = available_bytes()
            with open(directory / "stdout.log", "w") as stdout, open(directory / "stderr.log", "w") as stderr:
                proc = subprocess.Popen(command, stdout=stdout, stderr=stderr, start_new_session=True)
                while proc.poll() is None:
                    available = available_bytes()
                    minimum_available = min(available, minimum_available)
                    if available < args.min_available_gib * 2**30:
                        memory_guard = True
                        os.killpg(proc.pid, signal.SIGTERM)
                        try:
                            proc.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            os.killpg(proc.pid, signal.SIGKILL)
                        break
                    time.sleep(2)
                rc = proc.wait()
            status.write_text(json.dumps({"started_utc": started, "finished_utc": utc(),
                                          "exit_code": rc, "memory_guard": memory_guard,
                                          "minimum_host_available_bytes": minimum_available}, indent=2) + "\n")
            if rc != 0:
                raise RuntimeError(f"case {key} failed rc={rc}; memory_guard={memory_guard}")
        means = rows(result)
        raw = rows(result.with_suffix(".raw.csv"))
        measured = len(means) == 1 and means[0]["status"] == "measured" and len(raw) == args.repetitions
        excluded = (args.allow_memory_skips and len(means) == 1
                    and means[0]["status"] == "skipped_memory" and not raw)
        if not measured and not excluded:
            raise RuntimeError(f"case {key} missing complete measurements or explicit memory exclusion")
        if args.memory_mode == "retained" and any(r["low_memory"] != "false" for r in means + raw):
            raise RuntimeError(f"case {key} violated retained-only policy")
        all_means.extend(means)
        all_raw.extend(raw)
        write_table(output / "results.csv", all_means)
        write_table(output / "results.raw.csv", all_raw)
        progress.write_text(json.dumps({"completed": sum(m["status"] == "measured" for m in all_means),
                                           "processed": len(all_means), "total": 144,
                                       "last_completed": key, "updated_utc": utc()}) + "\n")
    (output / "finished.json").write_text(json.dumps({"finished_utc": utc(), "processed": len(all_means),
                                                    "measured": sum(m["status"] == "measured" for m in all_means),
                                                    "skipped_memory": sum(m["status"] == "skipped_memory" for m in all_means),
                                                    "raw_samples": len(all_raw)}) + "\n")


if __name__ == "__main__":
    main()
