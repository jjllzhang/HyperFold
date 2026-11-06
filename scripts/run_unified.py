#!/usr/bin/env python3
"""Run uniform one-shot cases sequentially with fixed binary and observed resources.

Every case is an independent32-thread process,1warmup+Nfresh-Setup measured trials.
Use new directories; no resume, estimates, memory switches or automatic fallback.
"""

import argparse, json, os, pathlib, re, shutil, subprocess
from benchmark_evidence import (
    monitor_command,
    prepare_attempt,
    rows,
    sha,
    utc,
    write_table,
    command_output,
)

METRICS = (
    "setup_ms",
    "input_ms",
    "commit_ms",
    "forward_release_ms",
    "open_ms",
    "witness_release_ms",
    "first_tree_query_ms",
    "verify_ms",
    "serialization_ms",
    "diagnostics_ms",
    "cleanup_ms",
    "wall_ms",
    "online_total_ms",
)
SCHEMA = "hyperfold-one-shot-bench-v1"


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary", required=True, type=pathlib.Path)
    p.add_argument("--output-dir", required=True, type=pathlib.Path)
    p.add_argument("--fields", default="m127")
    p.add_argument("--backends", default="qa,rs")
    p.add_argument("--mu", default="20..=30")
    p.add_argument("--r", default="2,4,8")
    p.add_argument("--c", default="2,4")
    p.add_argument("--repetitions", type=int, default=3)
    p.add_argument("--rounds", type=int, default=1)
    p.add_argument("--continue-on-failure", action="store_true")
    p.add_argument(
        "--smoke",
        action="store_true",
        help="label a functional smoke; requires mu20, one round and one measured repetition",
    )
    a = p.parse_args()
    if a.repetitions < 1 or a.rounds < 1:
        p.error("positive repetitions and rounds required")
    if a.smoke and (a.mu != "20" or a.rounds != 1 or a.repetitions != 1):
        p.error("smoke requires mu20/one round/one measured repetition")
    root = a.output_dir.resolve()
    root.mkdir(parents=True, exist_ok=True)
    if any(root.iterdir()):
        raise RuntimeError("new empty evidence directory required")
    allowed = sorted(os.sched_getaffinity(0))
    physical = {}
    for cpu in allowed:
        t = pathlib.Path(f"/sys/devices/system/cpu/cpu{cpu}/topology")
        physical.setdefault(
            (
                (t / "physical_package_id").read_text().strip(),
                (t / "core_id").read_text().strip(),
            ),
            cpu,
        )
    cpus = list(physical.values())[:32]
    if len(cpus) != 32:
        raise RuntimeError("need32distinct allowed physical cores")
    os.sched_setaffinity(0, cpus)
    binary = root / "profile"
    shutil.copy2(a.binary.resolve(), binary)
    selection = [
        "--fields",
        a.fields,
        "--backends",
        a.backends,
        "--mu",
        a.mu,
        "--r",
        a.r,
        "--c",
        a.c,
        "--threads",
        "32",
    ]
    listing = subprocess.check_output([str(binary), "--list", *selection], text=True)
    (root / "selected.csv").write_text(listing)
    selected = rows(root / "selected.csv")
    if not selected or any(s["schema_version"] != SCHEMA for s in selected):
        raise RuntimeError("unexpected benchmark schema")
    jobs = [
        dict(s, trial_round=iteration + 1)
        for iteration in range(a.rounds)
        for s in selected
    ]
    repo = pathlib.Path(__file__).resolve().parents[1]
    status = command_output(["git", "-C", str(repo), "status", "--short"])
    if status:
        raise RuntimeError("commit verified sources before timing")
    manifest = dict(
        schema_version=SCHEMA,
        experiment_role="functional_smoke" if a.smoke else "uniform_benchmark",
        started_utc=utc(),
        source_commit=command_output(["git", "-C", str(repo), "rev-parse", "HEAD"]),
        source_status=status,
        binary_sha256=sha(binary),
        jobs=jobs,
        threads=32,
        repetitions=a.repetitions,
        warmup=1,
        process_rounds=a.rounds,
        setup_per_trial=True,
        first_tree_strategy="prehashed_recovered",
        parameter_lifecycle="opening_only",
        witness_lifecycle="consumed_after_first_round",
        memory_guard_enabled=False,
        estimate_exclusions=False,
        whole_tree_rebuild=False,
        cpu_affinity=cpus,
        initial_affinity=allowed,
        exclusive_host=False,
        setup_seed_byte=42,
        message_seed_byte=27,
        point_seed_byte=28,
        hardware=command_output(["lscpu"]),
        rustc=command_output(["rustc", "-Vv"]),
        build_flags="RUSTFLAGS=-C target-cpu=native",
        root_lock_sha256=sha(repo / "Cargo.lock"),
        field_source_sha256=sha(repo / "src/field_m127.rs"),
        certificate="hfc-tight-total100-gold-m127-v3-2026-11-05",
        online_scope="commit+forward_release+open+verify",
        wall_scope="fresh Setup/input+evaluation/commit/release/open/verify/wire/diagnostics/cleanup; CSV IO excluded",
    )
    manifest["parameters_sha256"] = {
        p.name: sha(p) for p in (repo / "data").glob("*.csv")
    }
    vendor = "/home/zjl/BinaryField-Bench/vendor/Plonky3"
    manifest["plonky3_commit"] = command_output(
        ["git", "-C", vendor, "rev-parse", "HEAD"]
    )
    manifest["plonky3_status"] = command_output(
        ["git", "-C", vendor, "status", "--short"]
    )
    if manifest["plonky3_status"]:
        raise RuntimeError("shared Plonky3 must be unchanged")
    (root / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    shutil.copytree(repo / "data", root / "parameters")
    shutil.copy2(repo / "Cargo.lock", root / "root-Cargo.lock")
    source = root / "source"
    source.mkdir()
    for f in ["run_unified.py", "benchmark_evidence.py", "analyze_unified.py"]:
        shutil.copy2(repo / "scripts" / f, source / f)
    policy = dict(
        enabled=True,
        binary_sha256=manifest["binary_sha256"],
        schema_version=SCHEMA,
        memory_guard_enabled=False,
    )
    policy_path = root / "policy.json"
    policy_path.write_text(json.dumps(policy, indent=2) + "\n")

    def enabled():
        return json.loads(policy_path.read_text()) == policy

    outcomes = []
    raw_all = []
    signatures = {}

    def finish(termination_reason=None, error=None):
        failed = sum(r["status"] == "runtime_failed" for r in outcomes)
        result = dict(
            finished_utc=utc(),
            schema_version=SCHEMA,
            processes=len(outcomes),
            planned_processes=len(jobs),
            attempted_processes=len(outcomes),
            unattempted_processes=len(jobs) - len(outcomes),
            measured=sum(r["status"] == "measured" for r in outcomes),
            runtime_failed=failed,
            complete_raw_samples=len(raw_all),
            run_status="stopped" if termination_reason else "completed",
            termination_reason=termination_reason,
            error=error,
        )
        (root / "finished.json").write_text(json.dumps(result, indent=2) + "\n")
        (root / "progress.json").write_text(
            json.dumps(dict(phase=result["run_status"], **result)) + "\n"
        )

    for job in jobs:
        key = "{}-{}-{}-r{}-c{}-round{}".format(
            job["field"],
            job["backend"],
            job["mu"],
            job["r"],
            job["c"],
            job["trial_round"],
        )
        case = root / "cases" / key
        case.mkdir(parents=True)
        command = [
            "/usr/bin/time",
            "-v",
            "-o",
            str(case / "resources.txt"),
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
            "--repetitions",
            str(a.repetitions),
            "--trial-round",
            str(job["trial_round"]),
            "--output",
            str(case / "results.csv"),
        ]
        contract = dict(
            schema_version=SCHEMA,
            command=command,
            binary_sha256=manifest["binary_sha256"],
            job=job,
            repetitions=a.repetitions,
            warmup=1,
            setup_per_trial=True,
            memory_guard_enabled=False,
        )
        if prepare_attempt(case, command, contract) is not None:
            raise RuntimeError("new case directory required")
        (root / "progress.json").write_text(
            json.dumps(
                dict(
                    completed=len(outcomes),
                    total=len(jobs),
                    current=key,
                    updated_utc=utc(),
                )
            )
            + "\n"
        )
        try:
            allowed = enabled()
        except Exception as error:
            finish(
                "policy_unreadable", dict(type=type(error).__name__, message=str(error))
            )
            raise RuntimeError(
                "execution policy unreadable; no further work launched"
            ) from error
        if not allowed:
            finish("policy_revoked")
            raise RuntimeError("execution policy revoked")
        if sha(binary) != manifest["binary_sha256"]:
            finish("binary_changed")
            raise RuntimeError("binary changed")
        observed = monitor_command(command, case, enabled)
        observed["execution_contract"] = contract
        (case / "status.json").write_text(json.dumps(observed, indent=2) + "\n")
        current = (
            rows(case / "results.raw.csv")
            if (case / "results.raw.csv").exists()
            else []
        )
        summary = rows(case / "results.csv") if (case / "results.csv").exists() else []
        complete = (
            observed["exit_code"] == 0
            and observed["stop_reason"] is None
            and len(summary) == 1
            and len(current) == a.repetitions
        )
        entry = dict(
            **job,
            status="measured" if complete else "runtime_failed",
            exit_code=observed["exit_code"],
            recorded_samples=len(current),
            complete_repetitions=a.repetitions if complete else 0,
        )
        for m in METRICS:
            for k in ["mean", "sample_sd"]:
                entry[f"{m}_{k}"] = summary[0][f"{m}_{k}"] if complete else ""
        for k in [
            "commitment_bytes",
            "eval_proof_bytes",
            "proof_size_bytes",
            "fingerprint",
        ]:
            entry[k] = summary[0][k] if complete else ""
        resource = (case / "resources.txt").read_text()
        peak = re.search(r"Maximum resident set size \(kbytes\): (\d+)", resource)
        entry["peak_rss_gib"] = f"{int(peak[1])/2**20:.9f}" if peak else ""
        if complete:
            for row in current:
                for k in [
                    "schema_version",
                    "field",
                    "backend",
                    "mu",
                    "r",
                    "c",
                    "extension_degree",
                    "queries",
                    "threads",
                    "first_tree_strategy",
                    "parameter_lifecycle",
                    "witness_lifecycle",
                    "setup_per_trial",
                ]:
                    assert row[k] == str(job[k])
                assert (
                    int(row["trial_round"]) == job["trial_round"]
                    and row["forward_capacity_elements"] == "0"
                    and int(row["released_witness_elements"]) == 1 << int(job["mu"])
                )
                sig = tuple(
                    row[k]
                    for k in [
                        "fingerprint",
                        "proof_size_bytes",
                        "parameters_id",
                        "commitment_root",
                        "y0_hash",
                        "proof_hash",
                        "challenge_hash",
                    ]
                )
                sigkey = tuple(row[k] for k in ["field", "backend", "mu", "r", "c"])
                assert signatures.setdefault(sigkey, sig) == sig
            raw_all.extend(current)
        outcomes.append(entry)
        write_table(root / "results.csv", outcomes)
        write_table(root / "raw-samples.csv", raw_all)
        if not complete and (
            not a.continue_on_failure or observed["stop_reason"] is not None
        ):
            finish(
                observed["stop_reason"] or "worker_failed",
                observed.get("monitor_error"),
            )
            raise RuntimeError(
                f"{key} incomplete; evidence preserved and aggregates blank"
            )
    finish()


if __name__ == "__main__":
    main()
