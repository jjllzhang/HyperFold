#!/usr/bin/env python3
"""Validate/report two formal queried-row attempts without mixing baseline rows."""
import argparse
import csv
import hashlib
import json
import pathlib
import re
import shutil
import statistics
import tarfile

METRICS = ['commit_time_ms', 'open_time_ms', 'verify_time_ms', 'commitment_bytes',
           'eval_proof_bytes', 'proof_size_bytes', 'proof_size_kb', 'diagonal_transition_ms', 'fingerprint']


def rows(path):
    with path.open() as stream:
        return list(csv.DictReader(stream))


def table(path, data, fields=None):
    fields = fields or list(data[0])
    with path.open('w', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        writer.writerows(data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', required=True, type=pathlib.Path)
    parser.add_argument('--parameters', required=True, type=pathlib.Path)
    parser.add_argument('--output', required=True, type=pathlib.Path)
    args = parser.parse_args()
    root = args.root
    manifest = json.loads((root / 'manifest.json').read_text())
    finished = json.loads((root / 'finished.json').read_text())
    assert manifest['first_tree_strategy'] == finished['first_tree_strategy'] == 'prehashed_recovered'
    assert manifest['diagonal_storage'] == finished['diagonal_storage'] == 'retained_both'
    assert not manifest['memory_guard_enabled'] and not finished['memory_guard_enabled']
    assert not manifest['whole_tree_rebuild'] and not manifest['estimate_exclusions']
    assert manifest['fresh_commit_every_repetition'] and manifest['repetitions'] == 3 and manifest['warmup'] == 1
    assert manifest['threads'] == 32
    parameters = {('qa' if r['backend'] == 'QA_units' else 'rs'): r for r in rows(args.parameters)
                  if r['mu'] == str(manifest['mu']) and r['r'] == '2' and r['c'] == '4'}
    assert set(parameters) == {'qa', 'rs'}
    outcomes = rows(root / 'outcomes.csv')
    assert len(outcomes) == 2 and {r['backend'].lower() for r in outcomes} == {'qa', 'rs'}
    complete, resources = [], []
    for outcome in outcomes:
        code = outcome['backend'].lower()
        case = root / 'cases' / code
        status = json.loads((case / 'status.json').read_text())
        contract = json.loads((case / 'execution-contract.json').read_text())
        command = json.loads((case / 'command.json').read_text())
        assert command == contract['command'] == status['execution_contract']['command']
        assert contract == status['execution_contract']
        assert contract['binary_sha256'] == manifest['binary_sha256'] and not contract['memory_guard_enabled']
        assert not status['memory_guard_enabled'] and status['stop_reason'] is None
        for flag, value in [('--mu', manifest['mu']), ('--fields', 'm127'), ('--r', 2), ('--c', 4),
                            ('--threads', 32), ('--memory-mode', 'retained'), ('--first-tree-strategy', 'recovered'),
                            ('--repetitions', 3), ('--backends', code)]:
            assert command[command.index(flag)+1] == str(value)
        assert outcome['mu0'] == parameters[code]['mu0']
        assert outcome['queries'] == parameters[code]['queries']
        assert outcome['extension_degree'] == parameters[code]['extension_degree'] == '2'
        assert outcome['first_tree_strategy'] == 'prehashed_recovered' and outcome['diagonal_storage'] == 'retained_both'
        assert outcome['low_memory'] == 'false' and outcome['estimated_gib'] == ''
        samples = rows(case / 'results.raw.csv') if (case / 'results.raw.csv').exists() else []
        assert int(outcome.get('recorded_raw_samples', outcome.get('partial_raw_samples'))) == len(samples)
        assert int(outcome['exit_code']) == status['exit_code']
        if status['exit_code'] == 0:
            assert outcome['status'] == 'measured' and len(samples) == 3
            assert [r['repetition'] for r in samples] == ['1', '2', '3']
            for sample in samples:
                assert sample['first_tree_strategy'] == outcome['first_tree_strategy']
                assert sample['diagonal_storage'] == 'retained_both' and sample['low_memory'] == 'false'
                assert sample['fingerprint'] == outcome['fingerprint']
                assert sample['queries'] == parameters[code]['queries']
            for metric in ('commit_time_ms', 'open_time_ms', 'verify_time_ms', 'diagonal_transition_ms'):
                assert abs(float(outcome[metric])-statistics.mean(float(s[metric]) for s in samples)) < 0.000002
            assert all(int(s['commitment_bytes'])+int(s['eval_proof_bytes']) == int(outcome['proof_size_bytes']) for s in samples)
            complete.extend(samples)
        else:
            assert outcome['status'] == 'runtime_failed' and all(outcome[k] == '' for k in METRICS)
        text = (case / 'resources.txt').read_text()
        rss = int(re.search(r'Maximum resident set size \(kbytes\): (\d+)', text)[1]) / 2**20
        signal = re.search(r'Command terminated by signal (\d+)', text)
        delta = status['system_after']['vmstat']['oom_kill']-status['system_before']['vmstat']['oom_kill']
        assert delta == int(outcome['global_oom_kill_delta'])
        resources.append(dict(backend=code, started_utc=status['started_utc'], finished_utc=status['finished_utc'],
                              exit_code=status['exit_code'], termination_signal=signal[1] if signal else '',
                              gnu_peak_rss_gib=f'{rss:.6f}', sampled_hwm_gib=f"{status['observed_peak_hwm_kib']/2**20:.6f}",
                              minimum_available_gib=f"{status['minimum_host_available_bytes']/2**30:.6f}",
                              global_oom_kill_delta=delta,
                              swapin_pages_delta=status['system_after']['vmstat']['pswpin']-status['system_before']['vmstat']['pswpin'],
                              swapout_pages_delta=status['system_after']['vmstat']['pswpout']-status['system_before']['vmstat']['pswpout']))
    assert complete == rows(root / 'completed-raw.csv')
    assert finished['processed'] == 2 and finished['complete_raw_samples'] == len(complete)
    assert finished['measured'] == sum(r['status'] == 'measured' for r in outcomes)
    assert finished['runtime_failed'] == sum(r['status'] == 'runtime_failed' for r in outcomes)
    if (root / 'bench').exists():
        with (root / 'bench').open('rb') as stream:
            assert hashlib.file_digest(stream, 'sha256').hexdigest() == manifest['binary_sha256']
    out = args.output
    out.mkdir(parents=True, exist_ok=True)
    table(out / 'outcomes.csv', outcomes)
    table(out / 'resources.csv', resources)
    # With no completed repetitions, preserve an explicit empty table instead of fabricated data.
    table(out / 'raw-samples.csv', complete, list(complete[0]) if complete else ['field', 'backend', 'repetition', 'commit_time_ms', 'open_time_ms', 'verify_time_ms', 'fingerprint'])
    for name in ('manifest.json', 'finished.json', 'policy.json', 'selected-cases.csv'):
        shutil.copy2(root / name, out / name)
    for name in ('metadata.json', 'implementation-plan.md'):
        if (root.parent / name).exists():
            shutil.copy2(root.parent / name, out / name)
    shutil.copy2(args.parameters, out / 'm127-parameters.csv')
    shutil.copy2(__file__, out / 'analyze_query_extremes.py')
    with tarfile.open(out / 'raw-evidence.tar.gz', 'w:gz') as archive:
        for p in sorted(root.rglob('*')):
            if p.is_file() and p.name != 'bench':
                archive.add(p, arcname=pathlib.Path('data') / p.relative_to(root))
        for name in ('metadata.json', 'implementation-plan.md', 'run.sh', 'exit-status', 'finished-utc.txt', 'stdout.log', 'stderr.log'):
            if (root.parent / name).exists():
                archive.add(root.parent / name, arcname=name)
    report = f"# Formal queried-row attempts at μ={manifest['mu']}\n\n"
    report += (f"Actually attempted two M127/r=2/c=4 configurations: {finished['measured']} completed, "
               f"{finished['runtime_failed']} runtime failures, {len(complete)} complete measured repetitions. "
               "Strategy: **prehashed_recovered**, Setup: **retained_both**, threads: **32**. "
               "No active available-memory termination or estimate exclusion was used.\n\n")
    report += '| Code | Status | Exit | Signal | GNU time peak RSS (GiB) | Minimum host available (GiB) | Global OOM-count delta |\n|---|---|---:|---:|---:|---:|---:|\n'
    for outcome, r in zip(outcomes, resources):
        report += f"| {r['backend'].upper()} | {outcome['status']} | {r['exit_code']} | {r['termination_signal']} | {r['gnu_peak_rss_gib']} | {r['minimum_available_gib']} | {r['global_oom_kill_delta']} |\n"
    report += '''
Each process prepares Setup once, then performs one warmup and three measured repetitions, with a fresh commitment in each repetition. Setup/input generation/claimed-value evaluation are outside commit/open/verify timers. Opening includes prehashing, codeword release, query-row recovery and native multiproof work. Both diagonal directions remain live; no forward-table release or whole-tree rebuilding is used. Proof size counts encoded commitment plus evaluation proof. Only complete verified repetitions with matching prover/verifier post-challenges and deterministic fingerprints enter raw records. Aggregate timing and size fields remain blank for every incomplete case, even if a partial measurement exists.

The processes were actually launched under a pinned binary and immutable execution contracts; no system swap/cgroup/kernel settings were changed. RSS spans the entire process, including Setup/warmup. Exit code and GNU time signal are reported independently. SIGKILL plus a global OOM-counter increase does not establish kernel-confirmed attribution to a particular PID. Memory traces and swap-counter changes are preserved; the old 498 GiB retained-path stop was a partial observation, not a full-peak baseline.

The configured queried-row path can release a 64 GiB first codeword at μ30 after its successor is folded. A failed attempt does not by itself show which allocation/lifecycle stage was reached; no source-inferred release point is presented as an observed event. The plan's staged payload estimates are static accounting, not measured RSS bounds. This run does not include the separate candidate of releasing forward diagonal tables.

These are new strategy-specific outcomes. They do not overwrite the original 142 complete retained-mode configurations / 426 samples, the prior failures, or the μ26 constructor-control study. They do not establish an exact μ30 time-overhead percentage relative to a completed native baseline. Correctness checks and experiments are not security proofs.

## Evidence and reproduction

Exact source, executable SHA-256, seeds42/27/28, certificate, commands, strategy/table/guard policy, exit/resource records and partial raw files are retained. `raw-evidence.tar.gz` omits the executable but records its hash; `resources.csv` includes swap/global OOM observations. Full hardware/build context is in metadata (AMD EPYC9754, about503GiB RAM, Rust1.95.0, native CPU build; no affinity pinning, shared host).

To reproduce actual attempts from the recorded source and pinned local Plonky3 checkout:

```sh
RUSTFLAGS='-C target-cpu=native' cargo build --offline --locked --release --bin bench
python3 scripts/attempt_query_extremes.py --binary target/release/bench --root /tmp/recovered-extremes-new-empty
```

To reproduce numeric analysis from this archive without an executable:

```sh
mkdir evidence
tar -xzf raw-evidence.tar.gz -C evidence
python3 analyze_query_extremes.py --root evidence/data --parameters m127-parameters.csv --output regenerated
```

The original launched binary preceded the later CLI output-file guard fix; it ran only in freshly created case directories. That fix protects future manual output paths and does not change the measured proving code. Prior evidence remains immutable. SHA256SUMS covers every other snapshot file.
'''
    report += f"\nSource: `{manifest['source_commit']}`. Binary SHA-256: `{manifest['binary_sha256']}`.\n"
    if manifest['mu'] != 30:
        report = '**SMOKE DATA, not the μ30 actual-attempt result.**\n\n' + report
    (out / 'REPORT.md').write_text(report)
    (out / 'SHA256SUMS').write_text(''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n'
                                          for p in sorted(out.iterdir()) if p.is_file() and p.name != 'SHA256SUMS'))
    print(json.dumps(finished))


if __name__ == '__main__':
    main()
