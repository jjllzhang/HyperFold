#!/usr/bin/env python3
"""Neutral immutable-command/CSV/process evidence helpers. No memory guard or estimate policy."""

import csv, datetime, hashlib, json, os, pathlib, signal, subprocess, time


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
    raise RuntimeError("MemAvailable is required for host memory observations")


def write_table(path, entries):
    if not entries:
        return
    temp = path.with_suffix(".tmp")
    with open(temp, "w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(entries[0]))
        writer.writeheader()
        writer.writerows(entries)
    temp.replace(path)


def process_memory(pid):
    try:
        fields = dict(
            line.split(":", 1)
            for line in pathlib.Path(f"/proc/{pid}/status").read_text().splitlines()
            if ":" in line
        )
        return int(fields.get("VmRSS", "0 kB").split()[0]), int(
            fields.get("VmHWM", "0 kB").split()[0]
        )
    except (FileNotFoundError, ProcessLookupError):
        return 0, 0


def children(pid):
    try:
        return [
            int(p)
            for p in pathlib.Path(f"/proc/{pid}/task/{pid}/children")
            .read_text()
            .split()
        ]
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


def system_memory_snapshot():
    meminfo = {}
    for line in pathlib.Path("/proc/meminfo").read_text().splitlines():
        name, value = line.split(":", 1)
        if name in (
            "MemTotal",
            "MemAvailable",
            "SwapTotal",
            "SwapFree",
            "Committed_AS",
            "CommitLimit",
        ):
            meminfo[name + "_kib"] = int(value.split()[0])
    vmstat = dict(
        line.split() for line in pathlib.Path("/proc/vmstat").read_text().splitlines()
    )
    return {
        "meminfo": meminfo,
        "vmstat": {
            key: int(vmstat[key])
            for key in ("oom_kill", "pswpin", "pswpout")
            if key in vmstat
        },
        "overcommit_memory": pathlib.Path("/proc/sys/vm/overcommit_memory")
        .read_text()
        .strip(),
    }


def monitor_command(command, directory, policy_check):
    started = utc()
    system_before = system_memory_snapshot()
    minimum_available = available_bytes()
    peak_rss_kib = 0
    peak_hwm_kib = 0
    reason = None
    with open(directory / "stdout.log", "w") as stdout, open(
        directory / "stderr.log", "w"
    ) as stderr, open(directory / "memory-trace.csv", "w") as trace:
        trace.write("utc,available_bytes,bench_rss_kib,bench_hwm_kib\n")
        proc = subprocess.Popen(
            command, stdout=stdout, stderr=stderr, start_new_session=True
        )
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
            if reason:
                stop_workload(proc)
                break
            time.sleep(0.1)
        rc = proc.wait()
    return {
        "started_utc": started,
        "finished_utc": utc(),
        "exit_code": rc,
        "stop_reason": reason,
        "memory_guard": False,
        "minimum_host_available_bytes": minimum_available,
        "observed_peak_rss_kib": peak_rss_kib,
        "observed_peak_hwm_kib": peak_hwm_kib,
        "memory_guard_enabled": False,
        "system_before": system_before,
        "system_after": system_memory_snapshot(),
    }


def prepare_attempt(directory, command, contract):
    """Bind immutable evidence before launch; refuse incompatible or partial reuse."""
    status_path = directory / "status.json"
    contract_path = directory / "execution-contract.json"
    command_path = directory / "command.json"
    if status_path.exists():
        if not contract_path.exists() or not command_path.exists():
            raise RuntimeError(
                "existing attempt has no immutable contract; use a new directory"
            )
        if json.loads(contract_path.read_text()) != contract:
            raise RuntimeError("attempt contract changed; use a new directory")
        if json.loads(command_path.read_text()) != command:
            raise RuntimeError(
                "recorded command differs from contract; use a new directory"
            )
        status = json.loads(status_path.read_text())
        if status.get("memory_guard_enabled", True) != contract["memory_guard_enabled"]:
            raise RuntimeError(
                "recorded status guard differs from contract; use a new directory"
            )
        return status
    if any(directory.iterdir()):
        raise RuntimeError(
            "incomplete attempt evidence exists; preserve it and use a new directory"
        )
    contract_path.write_text(json.dumps(contract, indent=2) + "\n")
    command_path.write_text(json.dumps(command, indent=2) + "\n")
    return None
