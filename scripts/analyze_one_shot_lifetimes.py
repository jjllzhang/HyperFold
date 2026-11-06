#!/usr/bin/env python3
"""Validate/ archive one-shot lifecycle pilots or actual largest-case attempts."""
import argparse
import csv
import hashlib
import json
import math
import pathlib
import re
import shutil
import statistics
import tarfile

METRICS = ('setup_ms', 'input_ms', 'commit_ms', 'forward_release_ms', 'open_ms',
           'witness_release_ms', 'first_tree_query_ms', 'verify_ms', 'serialization_ms',
           'diagnostics_ms', 'cleanup_ms', 'wall_ms', 'online_total_ms')
PHASES = ('setup_ms', 'input_ms', 'commit_ms', 'forward_release_ms', 'open_ms',
          'verify_ms', 'serialization_ms', 'diagnostics_ms', 'cleanup_ms')


def rows(path):
    with path.open() as stream:
        return list(csv.DictReader(stream))


def table(path, data, fields=None):
    with path.open('w', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=fields or list(data[0]))
        writer.writeheader()
        writer.writerows(data)


def near(a, b, tolerance=0.000003):
    assert abs(float(a)-float(b)) <= tolerance, (a, b)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', required=True, type=pathlib.Path)
    parser.add_argument('--parameters', required=True, type=pathlib.Path)
    parser.add_argument('--output', required=True, type=pathlib.Path)
    args = parser.parse_args()
    root, out = args.root, args.output
    manifest = json.loads((root / 'manifest.json').read_text())
    finished = json.loads((root / 'finished.json').read_text())
    jobs = [tuple(j) for j in manifest['jobs']]
    assert len(jobs) == manifest['process_rounds'] * 2 * len(manifest['modes']) == finished['processes']
    expected_jobs = [(iteration+1, code, mode) for iteration in range(manifest['process_rounds'])
                     for code in ('qa', 'rs') for mode in
                     (manifest['modes'] if iteration % 2 == 0 else manifest['modes'][::-1])]
    assert jobs == expected_jobs and len(set(jobs)) == len(jobs)
    assert manifest['setup_per_trial'] and finished['setup_per_trial'] and not manifest['memory_guard_enabled']
    assert not manifest['estimate_exclusions'] and not manifest['whole_tree_rebuild']
    assert manifest['first_tree_strategy'] == 'prehashed_recovered' and manifest['threads'] == 32
    parameters = {('qa' if r['backend'] == 'QA_units' else 'rs'): r for r in rows(args.parameters)
                  if r['mu'] == str(manifest['mu']) and r['r'] == '2' and r['c'] == '4'}
    assert set(parameters) == {'qa', 'rs'}
    outcomes = rows(root / 'outcomes.csv')
    assert len(outcomes) == len(jobs)
    all_raw, signatures = [], {}
    for outcome, (iteration, code, mode) in zip(outcomes, jobs):
        assert (int(outcome['trial_round']), outcome['backend'], outcome['mode']) == (iteration, code, mode)
        key = f"M127-{code}-{manifest['mu']}-r2-c4-round{iteration}-{mode}"
        case = root / 'cases' / key
        status = json.loads((case / 'status.json').read_text())
        contract = json.loads((case / 'execution-contract.json').read_text())
        command = json.loads((case / 'command.json').read_text())
        assert status['execution_contract'] == contract and contract['command'] == command
        assert contract['binary_sha256'] == manifest['binary_sha256']
        assert not status['memory_guard_enabled'] and not contract['memory_guard_enabled']
        assert status['stop_reason'] is None and int(outcome['exit_code']) == status['exit_code']
        for flag, value in [('--mu', manifest['mu']), ('--backend', code), ('--mode', mode),
                            ('--threads', 32), ('--trial-round', iteration), ('--repetitions', manifest['repetitions'])]:
            assert command[command.index(flag)+1] == str(value)
        current = rows(case / 'samples.csv') if (case / 'samples.csv').exists() else []
        assert int(outcome['recorded_samples']) == len(current)
        if status['exit_code'] == 0:
            assert outcome['status'] == 'measured' and len(current) == manifest['repetitions']
            assert [r['repetition'] for r in current] == [str(j) for j in range(1, len(current)+1)]
            for r in current:
                assert (r['field'], r['backend'].lower(), r['mode'].lower(), r['mu'], r['r'], r['c'], r['threads']) == (
                    'M127', code, mode, str(manifest['mu']), '2', '4', '32')
                assert r['trial_round'] == str(iteration) and r['setup_per_trial'] == 'true'
                assert r['mu0'] == parameters[code]['mu0'] and r['depth'] == parameters[code]['depth']
                assert r['queries'] == parameters[code]['queries']
                assert r['extension_degree'] == parameters[code]['extension_degree'] == '2'
                assert r['first_tree_strategy'] == 'prehashed_recovered'
                assert r['parameter_lifecycle'] == outcome['parameter_lifecycle']
                assert r['witness_lifecycle'] == outcome['witness_lifecycle']
                diagonal_elements = 4 * ((1 << manifest['mu']) - (1 << int(r['mu0'])))
                assert int(r['inverse_capacity_elements']) >= diagonal_elements
                if mode == 'release':
                    assert r['forward_capacity_elements'] == '0'
                    assert r['parameter_lifecycle'] == 'opening_only'
                    assert r['witness_lifecycle'] == 'consumed_after_first_round'
                    assert int(r['released_witness_elements']) == 1 << manifest['mu']
                    assert int(r['released_witness_capacity']) >= 1 << manifest['mu']
                    assert r['witness_release_level'] == r['depth'] and r['witness_release_subround'] == '0'
                else:
                    assert int(r['forward_capacity_elements']) >= diagonal_elements
                    assert r['parameter_lifecycle'] == 'reusable_both' and r['witness_lifecycle'] == 'borrowed_retained'
                    assert r['released_witness_elements'] == '0' and r['released_witness_capacity'] == '0'
                    assert r['witness_release_level'] == r['witness_release_subround'] == ''
                    assert float(r['forward_release_ms']) == float(r['witness_release_ms']) == 0
                assert all(math.isfinite(float(r[k])) and float(r[k]) >= 0 for k in METRICS)
                near(r['online_total_ms'], sum(float(r[k]) for k in ('commit_ms', 'forward_release_ms', 'open_ms', 'verify_ms')), 0.000005)
                assert float(r['wall_ms']) + 0.0001 >= sum(float(r[k]) for k in PHASES)
                assert float(r['open_ms']) + 0.0001 >= float(r['witness_release_ms']) + float(r['first_tree_query_ms'])
                assert int(r['commitment_bytes']) + int(r['eval_proof_bytes']) == int(r['proof_size_bytes'])
                signature = (r['fingerprint'], r['proof_size_bytes'])
                assert signatures.setdefault(code, signature) == signature
                assert signature == (outcome['fingerprint'], outcome['proof_size_bytes'])
            for metric in METRICS:
                values = [float(r[metric]) for r in current]
                near(outcome[f'{metric}_mean'], statistics.mean(values))
                if len(values) > 1:
                    near(outcome[f'{metric}_sample_sd'], statistics.stdev(values))
            all_raw.extend(current)
        else:
            assert outcome['status'] == 'runtime_failed' and outcome['fingerprint'] == outcome['proof_size_bytes'] == ''
            assert all(outcome[f'{k}_mean'] == outcome[f'{k}_sample_sd'] == '' for k in METRICS)
        resource_text = (case / 'resources.txt').read_text()
        near(outcome['gnu_peak_rss_gib'], int(re.search(r'Maximum resident set size \(kbytes\): (\d+)', resource_text)[1])/2**20)
        signal = re.search(r'Command terminated by signal (\d+)', resource_text)
        assert outcome['termination_signal'] == (signal[1] if signal else '')
        assert int(outcome['global_oom_kill_delta']) == status['system_after']['vmstat']['oom_kill']-status['system_before']['vmstat']['oom_kill']
    assert all_raw == rows(root / 'raw-samples.csv')
    assert finished['complete_raw_samples'] == len(all_raw)
    assert finished['measured'] == sum(r['status'] == 'measured' for r in outcomes)
    assert finished['runtime_failed'] == sum(r['status'] == 'runtime_failed' for r in outcomes)
    if (root / 'profile').exists():
        with (root / 'profile').open('rb') as stream:
            assert hashlib.file_digest(stream, 'sha256').hexdigest() == manifest['binary_sha256']
    aggregates = []
    for code in ('qa', 'rs'):
        for mode in manifest['modes']:
            group = [r for r in outcomes if r['backend'] == code and r['mode'] == mode]
            measured = [r for r in group if r['status'] == 'measured']
            row = dict(backend=code, mode=mode, measured_processes=len(measured), failed_processes=len(group)-len(measured),
                       max_process_rss_gib=f"{max(float(r['gnu_peak_rss_gib']) for r in group):.6f}")
            for metric in METRICS:
                values = [float(r[f'{metric}_mean']) for r in measured]
                row[f'{metric}_mean'] = f'{statistics.mean(values):.6f}' if values else ''
                row[f'{metric}_process_sd'] = f'{statistics.stdev(values):.6f}' if len(values) > 1 else ''
            aggregates.append(row)
    contrasts = []
    for iteration in range(1, manifest['process_rounds']+1):
        for code in ('qa', 'rs'):
            group = {r['mode']: r for r in outcomes if r['backend'] == code and int(r['trial_round']) == iteration and r['status'] == 'measured'}
            if set(group) != {'retained', 'release'}:
                continue
            for metric in METRICS:
                a, b = (float(group[m][f'{metric}_mean']) for m in ('retained', 'release'))
                contrasts.append(dict(backend=code, trial_round=iteration, metric=metric, retained_mean=f'{a:.6f}',
                                      release_mean=f'{b:.6f}', delta_ms=f'{b-a:.6f}',
                                      change_percent=f'{100*(b/a-1):.6f}' if a else ''))
    comparisons = []
    for code in ('qa', 'rs'):
        group = {r['mode']: r for r in aggregates if r['backend'] == code and int(r['measured_processes']) > 0}
        if set(group) != {'retained', 'release'}:
            continue
        for metric in METRICS:
            a, b = (float(group[m][f'{metric}_mean']) for m in ('retained', 'release'))
            paired = [r for r in contrasts if r['backend'] == code and r['metric'] == metric]
            deltas = [float(r['delta_ms']) for r in paired]
            percentages = [float(r['change_percent']) for r in paired if r['change_percent']]
            comparisons.append(dict(backend=code, metric=metric, matched_rounds=len(paired),
                                    retained_mean=f'{a:.6f}', release_mean=f'{b:.6f}', delta_ms=f'{b-a:.6f}',
                                    change_percent=f'{100*(b/a-1):.6f}' if a else '',
                                    round_delta_sd_ms=f'{statistics.stdev(deltas):.6f}' if len(deltas)>1 else '',
                                    min_round_percent=f'{min(percentages):.6f}' if percentages else '',
                                    max_round_percent=f'{max(percentages):.6f}' if percentages else ''))
    out.mkdir(parents=True, exist_ok=True)
    for name, data, fields in [('outcomes.csv', outcomes, None), ('raw-samples.csv', all_raw,
                               list(all_raw[0]) if all_raw else ['field', 'backend', 'mode', 'repetition']),
                              ('strategy-means.csv', aggregates, None), ('round-contrasts.csv', contrasts,
                               list(contrasts[0]) if contrasts else ['backend', 'trial_round', 'metric', 'delta_ms'])]:
        table(out / name, data, fields)
    table(out / 'comparisons.csv', comparisons, list(comparisons[0]) if comparisons else ['backend', 'metric', 'delta_ms'])
    for name in ('manifest.json', 'finished.json', 'policy.json'):
        shutil.copy2(root / name, out / name)
    for name in ('metadata.json', 'implementation-plan.md'):
        if (root.parent / name).exists():
            shutil.copy2(root.parent / name, out / name)
    shutil.copy2(args.parameters, out / 'm127-parameters.csv')
    shutil.copy2(__file__, out / 'analyze_one_shot_lifetimes.py')
    with tarfile.open(out / 'raw-evidence.tar.gz', 'w:gz') as archive:
        for p in sorted(root.rglob('*')):
            if p.is_file() and p.name != 'profile':
                archive.add(p, arcname=pathlib.Path('data') / p.relative_to(root))
        for name in ('metadata.json', 'implementation-plan.md', 'run.sh', 'exit-status', 'finished-utc.txt', 'stdout.log', 'stderr.log'):
            if (root.parent / name).exists():
                archive.add(root.parent / name, arcname=name)
    report = f"# One-shot lifetime experiment: M127 μ={manifest['mu']}, r=2, c=4\n\n"
    report += (f"Completed recording {finished['processes']} processes: {finished['measured']} measured, "
               f"{finished['runtime_failed']} runtime failures, {len(all_raw)} complete measured trials. "
               "All modes use prehashed queried-row recovery, 32 threads and a fresh Setup for EVERY trial, including warmup.\n\n")
    report += '| Code | Mode | Processes measured | Commit ms | Open ms | Online total ms | Full wall ms | Max process RSS GiB |\n|---|---|---:|---:|---:|---:|---:|---:|\n'
    for r in aggregates:
        report += '| ' + ' | '.join(str(r[k]) for k in ('backend', 'mode', 'measured_processes', 'commit_ms_mean', 'open_ms_mean', 'online_total_ms_mean', 'wall_ms_mean', 'max_process_rss_gib')) + ' |\n'
    report += '\n| Code | Mode | Setup ms | Input ms | Forward release ms | Witness release ms (within open) | Serialization ms | Cleanup ms |\n|---|---|---:|---:|---:|---:|---:|---:|\n'
    for r in aggregates:
        report += '| ' + ' | '.join(str(r[k]) for k in ('backend', 'mode', 'setup_ms_mean', 'input_ms_mean', 'forward_release_ms_mean', 'witness_release_ms_mean', 'serialization_ms_mean', 'cleanup_ms_mean')) + ' |\n'
    if comparisons:
        report += '\n| Code | Metric | Release−retained ms | Change | Round difference SD ms | Round percent range |\n|---|---|---:|---:|---:|---:|\n'
        for r in comparisons:
            if r['metric'] not in ('open_ms', 'online_total_ms', 'wall_ms'):
                continue
            report += (f"| {r['backend']} | {r['metric']} | {r['delta_ms']} | {r['change_percent']}% | "
                       f"{r['round_delta_sd_ms'] or 'n/a'} | {r['min_round_percent']}% to {r['max_round_percent']}% |\n")
    report += '''
`retained` is the constructor-matched control: it keeps forward/inverse parameters and the borrowed original witness. `release` transitions to OpeningOnlyPcs after commit, drops forward storage without inversion, consumes only OwnedWitness and frees its allocation after the first complete lifted fold. Caller-owned top oracle survives verify/serialization and is destroyed in cleanup in both modes. No proof/transcript change or full-table clone is used. Actual capacity and release-round markers are checked against each record.

## Cost boundaries and repetition structure

Each process executes one warmup plus the recorded repetitions; each trial creates fresh Setup and fully cleans its oracle/parameters before the next Setup. The two-mode pilot alternates AB/BA in independent processes. Means within a process are not additional independent processes: `outcomes.csv` gives each process mean/sample SD, `strategy-means.csv` the mean/SD across successful process means, and `round-contrasts.csv` every matched round difference. Small differences comparable with variation are descriptive and do not establish zero overhead or formal statistical significance.

Full wall includes Setup, input generation/claimed-value evaluation, commit, forward release, open, verify, serialization, transcript/fingerprint diagnostics and cleanup, excluding CSV output. Online total is exactly commit+forward_release+open+verify. Witness-release and first-tree-query clocks are subparts of open; they are not added twice. Fresh Setup costs are reported, not amortized away or hidden outside the comparison. First-tree query includes recovery/path extraction/compression and its existing cache/order effects. Process peak RSS spans Setup/warmup/all trials and cleanup, not only opening.

Every complete trial verifies, has matching prover/verifier post-challenges, and shares commitment/proof/challenge fingerprints and wire size with its matched backend across modes/rounds. Size is commitment plus evaluation proof bytes. Incomplete cases retain partial raw evidence but have blank timing/size aggregates. Exit/signal/global OOM observations are kept separately; signal9/global counters alone do not prove PID-specific OOM attribution. No active available-memory guard or estimate exclusion is enabled, and host swap/kernel/cgroup settings are unchanged.

The previous official142/426, whole-tree diagnostic24, queried-row diagnostic54 and failedμ30 trials remain immutable separate datasets. This new outer Setup-per-trial lifecycle must not be silently treated as the old cached-Setup lifecycle. There is no successful nativeμ30 baseline for an exact same-size overhead percentage. Payload estimates (80GiB potential extra release atμ30) are not measured RSS bounds or success guarantees. Tests/engineering reviews are not cryptographic security proofs.

## Reproduction

Hardware/build/source, seeds42/27/28, native binary hash, exact case commands/contracts, memory traces and actual exit/resource records are preserved. Parameters are the unchanged certified uniform M127 quadratic rows; every recorded query count/depth/degree is checked. One shared host, no affinity binding. For the largest-case release-only follow-up there is no matched retained control in this dataset.

From the recorded source with its pinned local Plonky3 checkout:

```sh
RUSTFLAGS='-C target-cpu=native' cargo build --offline --locked --release --example one_shot_lifetimes
python3 scripts/run_one_shot_lifetimes.py --binary target/release/examples/one_shot_lifetimes --root /tmp/new-empty-pilot
```

The separate largest follow-up adds `--mu 30 --modes release --rounds 1 --continue-on-failure`.
Archive-only numeric reproduction, with no executable required:

```sh
mkdir evidence
tar -xzf raw-evidence.tar.gz -C evidence
python3 analyze_one_shot_lifetimes.py --root evidence/data --parameters m127-parameters.csv --output regenerated
```

The archive omits the large executable but retains its SHA-256. SHA256SUMS covers every other snapshot file.
'''
    if manifest['mu'] not in (26, 30):
        report = '**SMOKE ONLY, not the μ26 pilot or largest-case result.**\n\n' + report
    report += f"\nSource: `{manifest['source_commit']}`. Binary SHA-256: `{manifest['binary_sha256']}`.\n"
    (out / 'REPORT.md').write_text(report)
    (out / 'SHA256SUMS').write_text(''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n' for p in sorted(out.iterdir()) if p.is_file() and p.name != 'SHA256SUMS'))
    print(json.dumps(finished))


if __name__ == '__main__':
    main()
