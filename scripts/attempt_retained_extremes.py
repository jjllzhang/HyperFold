#!/usr/bin/env python3
"""Queue actual retained-only attempts after a base sweep, preserving failures.

The estimate ceiling is bypassed only for the two explicit M127 cases. Actual
available memory is monitored; guard stops produce blank aggregate measurements.
Base results stay immutable. Read final-results*.csv and final-finished.json after
this supplemental phase, rather than treating estimated exclusions as outcomes.
"""
import argparse
import csv
import json
import os
import pathlib
import signal
import subprocess
import time

from run_sweep import available_bytes, rows, sha, utc, write_table

KEYS = ["M127-Qa-30-2-4", "M127-Rs-30-2-4"]
MEASUREMENT_FIELDS = [
    "commit_time_ms", "open_time_ms", "verify_time_ms", "commitment_bytes",
    "eval_proof_bytes", "proof_size_bytes", "proof_size_kb", "diagonal_transition_ms",
]


def process_memory(pid):
    try:
        fields = dict(line.split(":", 1) for line in
                      pathlib.Path(f"/proc/{pid}/status").read_text().splitlines() if ":" in line)
        return int(fields.get("VmRSS", "0 kB").split()[0]), int(fields.get("VmHWM", "0 kB").split()[0])
    except (FileNotFoundError, ProcessLookupError):
        return 0, 0


def children(pid):
    try:
        return [int(p) for p in pathlib.Path(f"/proc/{pid}/task/{pid}/children").read_text().split()]
    except FileNotFoundError:
        return []


def stop_workload(proc):
    # Let GNU time survive to record the child's termination and resource usage.
    child_pids = children(proc.pid)
    if not child_pids:
        try:
            proc.wait(timeout=0.2)
            return
        except subprocess.TimeoutExpired:
            child_pids = children(proc.pid)
    for pid in child_pids:
        try:
            os.kill(pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    if not child_pids:
        proc.terminate()
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        proc.wait()


def monitor_command(command, directory, guard_gib, policy_check):
    started = utc()
    minimum_available = available_bytes()
    peak_rss_kib = 0
    peak_hwm_kib = 0
    reason = None
    with open(directory / "stdout.log", "w") as stdout, open(directory / "stderr.log", "w") as stderr, \
            open(directory / "memory-trace.csv", "w") as trace:
        trace.write("utc,available_bytes,bench_rss_kib,bench_hwm_kib\n")
        proc = subprocess.Popen(command, stdout=stdout, stderr=stderr, start_new_session=True)
        previous_log = 0.0
        while proc.poll() is None:
            available = available_bytes()
            minimum_available = min(minimum_available, available)
            rss, hwm = (0, 0)
            for pid in children(proc.pid):
                r, h = process_memory(pid)
                rss += r
                hwm = max(hwm, h)
            peak_rss_kib = max(peak_rss_kib, rss)
            peak_hwm_kib = max(peak_hwm_kib, hwm)
            now = time.monotonic()
            if now - previous_log >= 1:
                trace.write(f"{utc()},{available},{rss},{hwm}\n")
                trace.flush()
                previous_log = now
            if not policy_check():
                reason = "policy_revoked"
            elif available < guard_gib * 2**30:
                reason = "available_memory_guard"
            if reason:
                stop_workload(proc)
                break
            time.sleep(0.1)
        rc = proc.wait()
    return {"started_utc": started, "finished_utc": utc(), "exit_code": rc,
            "stop_reason": reason, "memory_guard": reason == "available_memory_guard",
            "minimum_host_available_bytes": minimum_available,
            "observed_peak_rss_kib": peak_rss_kib, "observed_peak_hwm_kib": peak_hwm_kib}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=pathlib.Path, required=True)
    parser.add_argument("--estimate-bypass-gib", type=float, default=1024)
    args = parser.parse_args()
    root = args.root.resolve()
    policy_path = root / "actual-attempt-policy.json"

    def enabled():
        policy = json.loads(policy_path.read_text())
        return policy["enabled"] and policy["memory_mode"] == "retained"

    manifest = json.loads((root / "manifest.json").read_text())
    assert manifest["contract"]["memory_mode"] == "retained"
    assert sha(root / "bench") == manifest["contract"]["binary_sha256"]
    repetitions = manifest["contract"]["repetitions"]
    guard = manifest["contract"]["min_available_gib"]
    progress_path = root / "actual-attempt-progress.json"
    progress_path.write_text(json.dumps({"phase": "waiting_for_base", "queued": KEYS,
                                         "updated_utc": utc()}) + "\n")
    while not (root / "finished.json").exists():
        if not enabled():
            raise RuntimeError("actual-attempt policy revoked")
        if (root / "exit-status").exists() and (root / "exit-status").read_text().strip() != "0":
            raise RuntimeError("base sweep failed; repair it before supplemental attempts")
        time.sleep(5)
    base = rows(root / "results.csv")
    base_raw = rows(root / "results.raw.csv")
    assert len(base) == 144
    index = {"-".join(row[k] for k in ["field", "backend", "mu", "r", "c"]): i
             for i, row in enumerate(base)}
    assert len(index) == 144
    supplemental_raw = []
    for key in KEYS:
        if not enabled():
            raise RuntimeError("actual-attempt policy revoked")
        old = base[index[key]]
        if old["status"] == "measured":
            continue
        assert old["status"] == "skipped_memory"
        directory = root / "actual-attempts" / key
        directory.mkdir(parents=True, exist_ok=True)
        result = directory / "results.csv"
        status_path = directory / "status.json"
        command = ["/usr/bin/time", "-v", "-o", str(directory / "resources.txt"), str(root / "bench"),
                   "--threads", "32", "--mu", "30", "--fields", "m127", "--backends",
                   old["backend"].lower(), "--r", "2", "--c", "4", "--repetitions", str(repetitions),
                   "--memory-mode", "retained", "--max-estimated-gib", str(args.estimate_bypass_gib),
                   "--output", str(result)]
        (directory / "command.json").write_text(json.dumps(command, indent=2) + "\n")
        progress_path.write_text(json.dumps({"phase": "attempting", "current": key,
                                             "updated_utc": utc()}) + "\n")
        if status_path.exists():
            status = json.loads(status_path.read_text())
        else:
            if available_bytes() < guard * 2**30:
                raise RuntimeError("insufficient available memory before actual attempt")
            status = monitor_command(command, directory, guard, enabled)
            status_path.write_text(json.dumps(status, indent=2) + "\n")
        if status["stop_reason"] == "policy_revoked":
            raise RuntimeError("actual attempt stopped because policy changed")
        if status["exit_code"] == 0:
            measured = rows(result)
            samples = rows(result.with_suffix(".raw.csv"))
            assert len(measured) == 1 and measured[0]["status"] == "measured"
            assert measured[0]["low_memory"] == "false" and len(samples) == repetitions
            assert all(sample["low_memory"] == "false" for sample in samples)
            base[index[key]] = measured[0]
            supplemental_raw.extend(samples)
        else:
            # Preserve every partial sample and failure log in actual-attempts/. A
            # complete three-repetition measurement is required for aggregate timings.
            replacement = dict(old)
            replacement["status"] = "memory_guard_stopped" if status["memory_guard"] else "runtime_failed"
            for field in MEASUREMENT_FIELDS:
                replacement[field] = ""
            base[index[key]] = replacement
            if not status["memory_guard"]:
                write_table(root / "final-results.csv", base)
                raise RuntimeError(f"unexpected runtime failure in {key}; inspect actual-attempts")
        write_table(root / "final-results.csv", base)
        write_table(root / "final-results.raw.csv", base_raw + supplemental_raw)
    summary = {"finished_utc": utc(), "processed": len(base),
               "measured": sum(row["status"] == "measured" for row in base),
               "memory_guard_stopped": sum(row["status"] == "memory_guard_stopped" for row in base),
               "estimate_only_exclusions": sum(row["status"] == "skipped_memory" for row in base),
               "raw_samples": len(base_raw) + len(supplemental_raw), "recomputation_used": False}
    assert summary["estimate_only_exclusions"] == 0
    (root / "final-finished.json").write_text(json.dumps(summary, indent=2) + "\n")
    progress_path.write_text(json.dumps({"phase": "finished", **summary}) + "\n")


if __name__ == "__main__":
    main()
