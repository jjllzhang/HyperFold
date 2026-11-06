#!/usr/bin/env python3
"""Validate uniform one-shot raw/summary/resource evidence and build a portable archive."""

import argparse, csv, hashlib, json, math, pathlib, re, shutil, statistics, tarfile, tempfile

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


def rows(p):
    if not p.exists():
        return []
    with p.open(newline="") as f:
        return list(csv.DictReader(f))


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def table(p, data):
    if not data:
        p.write_text("")
        return
    with p.open("w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(data[0]), lineterminator="\n")
        w.writeheader()
        w.writerows(data)


def analyze(root, out):
    manifest = json.loads((root / "manifest.json").read_text())
    finish = json.loads((root / "finished.json").read_text())
    out.mkdir(parents=True, exist_ok=True)
    if any(out.iterdir()):
        raise RuntimeError("new report directory required")
    assert (
        manifest["schema_version"] == finish["schema_version"] == SCHEMA
        and manifest["threads"] == 32
        and manifest["setup_per_trial"]
    )
    assert (
        not manifest["memory_guard_enabled"]
        and not manifest["whole_tree_rebuild"]
        and not manifest["estimate_exclusions"]
    )
    recorded = rows(root / "results.csv")
    assert len(recorded) == len(manifest["jobs"]) == finish["processes"]
    certificate_rows = {}
    for name, digest in manifest["parameters_sha256"].items():
        p = root / "parameters" / name
        assert sha(p) == digest
        for c in rows(p):
            key = (
                c["base_field"],
                "Qa" if c["backend"] == "QA_units" else "Rs",
                c["mu"],
                c["r"],
                c["c"],
            )
            certificate_rows[key] = c
    assert sha(root / "root-Cargo.lock") == manifest["root_lock_sha256"]
    all_raw = []
    signatures = {}
    measured = 0
    failed = 0
    for job, entry in zip(manifest["jobs"], recorded):
        key = "{}-{}-{}-r{}-c{}-round{}".format(
            job["field"],
            job["backend"],
            job["mu"],
            job["r"],
            job["c"],
            job["trial_round"],
        )
        case = root / "cases" / key
        contract = json.loads((case / "execution-contract.json").read_text())
        status = json.loads((case / "status.json").read_text())
        command = json.loads((case / "command.json").read_text())
        assert (
            contract == status["execution_contract"]
            and command == contract["command"]
            and contract["job"] == job
        )
        assert (
            contract["binary_sha256"] == manifest["binary_sha256"]
            and contract["schema_version"] == SCHEMA
            and not contract["memory_guard_enabled"]
            and not status["memory_guard_enabled"]
        )
        assert (
            int(entry["exit_code"]) == status["exit_code"]
            and int(entry["trial_round"]) == job["trial_round"]
        )
        for flag, value in [
            ("--mu", job["mu"]),
            ("--r", job["r"]),
            ("--c", job["c"]),
            ("--fields", job["field"].lower()),
            ("--backends", job["backend"].lower()),
            ("--threads", 32),
            ("--trial-round", job["trial_round"]),
            ("--repetitions", manifest["repetitions"]),
        ]:
            assert command[command.index(flag) + 1] == str(value)
        cert = certificate_rows[
            tuple(str(job[k]) for k in ["field", "backend", "mu", "r", "c"])
        ]
        for k in ["mu0", "depth", "extension_degree"]:
            assert job[k] == cert[k]
        assert job["queries"] == cert["queries"]
        current = rows(case / "results.raw.csv")
        summary = rows(case / "results.csv")
        assert len(current) == int(entry["recorded_samples"])
        if entry["status"] == "measured":
            assert (
                status["exit_code"] == 0
                and status["stop_reason"] is None
                and len(current) == manifest["repetitions"]
                and len(summary) == 1
            )
            assert [int(r["repetition"]) for r in current] == list(
                range(1, len(current) + 1)
            )
            assert (
                summary[0]["status"] == "measured"
                and int(summary[0]["measured_repetitions"]) == manifest["repetitions"]
            )
            measured += 1
            for r in current:
                for k in [
                    "schema_version",
                    "field",
                    "backend",
                    "mu",
                    "r",
                    "c",
                    "mu0",
                    "depth",
                    "extension_degree",
                    "queries",
                    "threads",
                    "trial_round",
                    "first_tree_strategy",
                    "parameter_lifecycle",
                    "witness_lifecycle",
                    "setup_per_trial",
                ]:
                    assert r[k] == str(job[k])
                assert (
                    r["forward_capacity_elements"] == "0"
                    and int(r["inverse_capacity_elements"]) > 0
                )
                assert int(r["released_witness_elements"]) == 1 << int(
                    job["mu"]
                ) and int(r["released_witness_capacity"]) >= int(
                    r["released_witness_elements"]
                )
                assert (
                    r["witness_release_level"] == r["depth"]
                    and r["witness_release_subround"] == "0"
                )
                for m in METRICS:
                    assert math.isfinite(float(r[m])) and float(r[m]) >= 0
                online = sum(
                    float(r[m])
                    for m in ["commit_ms", "forward_release_ms", "open_ms", "verify_ms"]
                )
                assert abs(online - float(r["online_total_ms"])) < 0.00001
                assert int(r["proof_size_bytes"]) == int(r["commitment_bytes"]) + int(
                    r["eval_proof_bytes"]
                )
                sig = tuple(
                    r[k]
                    for k in [
                        "fingerprint",
                        "parameters_id",
                        "commitment_root",
                        "y0_hash",
                        "proof_hash",
                        "challenge_hash",
                        "proof_size_bytes",
                    ]
                )
                sigkey = tuple(r[k] for k in ["field", "backend", "mu", "r", "c"])
                assert signatures.setdefault(sigkey, sig) == sig
            for m in METRICS:
                values = [float(r[m]) for r in current]
                mean = statistics.mean(values)
                sd = statistics.stdev(values) if len(values) > 1 else 0.0
                for name, value in [("mean", mean), ("sample_sd", sd)]:
                    k = f"{m}_{name}"
                    assert (
                        abs(float(entry[k]) - value) < 0.00001
                        and abs(float(summary[0][k]) - value) < 0.00001
                    )
            for k in [
                "fingerprint",
                "proof_size_bytes",
                "commitment_bytes",
                "eval_proof_bytes",
            ]:
                assert summary[0][k] == entry[k] == current[0][k]
            all_raw.extend(current)
        else:
            assert entry["status"] == "runtime_failed"
            failed += 1
            for m in METRICS:
                for k in ["mean", "sample_sd"]:
                    assert entry[f"{m}_{k}"] == ""
            for k in [
                "proof_size_bytes",
                "commitment_bytes",
                "eval_proof_bytes",
                "fingerprint",
            ]:
                assert entry[k] == ""
        resource = (case / "resources.txt").read_text()
        peak = re.search(r"Maximum resident set size \(kbytes\): (\d+)", resource)
        if peak:
            assert abs(float(entry["peak_rss_gib"]) - int(peak[1]) / 2**20) < 0.00000001
    assert (
        measured == finish["measured"]
        and failed == finish["runtime_failed"]
        and len(all_raw) == finish["complete_raw_samples"]
    )
    assert rows(root / "raw-samples.csv") == all_raw
    table(out / "results.csv", recorded)
    table(out / "raw-samples.csv", all_raw)
    (out / "REPORT.md").write_text(
        f'# Uniform one-shot run\n\nValidated {len(recorded)} processes: {measured} complete, {failed} incomplete. {len(all_raw)} completed raw measurements. All trials use fresh Setup,32threads,forward/witness release and queried-row recovery. Incomplete aggregate timing/size is blank; partial raw evidence remains archived.\n\nOnline total includes commit+forward release+open+verify; wall includes Setup/input/wire/diagnostics/cleanup. Peak RSS spans the whole process and warmup. This report does not mix historical cached lifecycles or infer process-specific OOM attribution.\n\nExecution source `{manifest["source_commit"]}`, binary SHA256 `{manifest["binary_sha256"]}`. Tests and measurements do not constitute a security proof.\n'
    )
    (out / "manifest.json").write_bytes((root / "manifest.json").read_bytes())
    return dict(
        processes=len(recorded),
        measured=measured,
        runtime_failed=failed,
        complete_raw_samples=len(all_raw),
        analysis_sha256=sha(pathlib.Path(__file__)),
    )


def main():
    p = argparse.ArgumentParser(description=__doc__)
    g = p.add_mutually_exclusive_group(required=True)
    g.add_argument("--root", type=pathlib.Path)
    g.add_argument("--archive", type=pathlib.Path)
    p.add_argument("--output", required=True, type=pathlib.Path)
    a = p.parse_args()
    if a.archive:
        with tempfile.TemporaryDirectory() as d:
            with tarfile.open(a.archive) as t:
                t.extractall(d, filter="data")
            result = analyze(pathlib.Path(d) / "data", a.output)
    else:
        result = analyze(a.root, a.output)
        with tarfile.open(a.output / "raw-evidence.tar.gz", "w:gz") as t:
            for f in sorted(a.root.rglob("*")):
                if f.is_file() and f.name != "profile":
                    t.add(
                        f, arcname="data/" + str(f.relative_to(a.root)), recursive=False
                    )
        shutil.copy2(__file__, a.output / "analyze_unified.py")
    (a.output / "validation.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
