#!/usr/bin/env python3
"""Validate the three-strategy queried-row study and build a portable report."""
import argparse
import csv
import hashlib
import json
import pathlib
import re
import shutil
import statistics
import tarfile


def rows(path):
    with path.open() as stream:
        return list(csv.DictReader(stream))


def write(path, data):
    with path.open('w', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=list(data[0]))
        writer.writeheader()
        writer.writerows(data)


def near(a, b, tolerance=0.000002):
    assert abs(float(a)-float(b)) <= tolerance, (a, b)


def summarize(root, parameters, allow_smoke=False):
    manifest = json.loads((root / 'manifest.json').read_text())
    finished = json.loads((root / 'finished.json').read_text())
    count = manifest['process_rounds']
    reps = manifest['repetitions']
    if not allow_smoke:
        assert (manifest['mu'], count, reps) == (26, 3, 3)
    assert manifest['source_status'] == '' or allow_smoke
    assert (finished['profiles'], finished['raw_samples']) == (count * 6, count * 6 * reps)
    assert manifest['threads'] == 32 and manifest['r'] == 2 and manifest['c'] == 4
    expected_order = [(iteration + 1, backend, strategy)
                      for iteration, order in enumerate((('native', 'prehashed', 'recovered'),
                                                         ('prehashed', 'recovered', 'native'),
                                                         ('recovered', 'native', 'prehashed'))[:count])
                      for backend in ('qa', 'rs') for strategy in order]
    assert [tuple(j) for j in manifest['jobs']] == expected_order
    settings = {('qa' if r['backend'] == 'QA_units' else 'rs'): r
                for r in rows(parameters) if r['mu'] == str(manifest['mu']) and r['r'] == '2' and r['c'] == '4'}
    assert set(settings) == {'qa', 'rs'}
    assert all(r['target_bits'] == '100' and r['certificate_version'] == 'hfc-tight-total100-gold-m127-v3-2026-11-05' for r in settings.values())
    means = rows(root / 'process-means.csv')
    assert len(means) == count * 6
    signatures, canonical_raw = {}, []
    metric_names = ('open_time_ms', 'first_tree_query_ms', 'verify_time_ms')
    for m, (iteration, code, strategy) in zip(means, expected_order):
        assert (int(m['process_round']), m['backend'], m['strategy']) == (iteration, code, strategy)
        case = root / 'cases' / f"M127-{code}-{manifest['mu']}-r2-c4-round{iteration}-{strategy}"
        status = json.loads((case / 'status.json').read_text())
        assert status['exit_code'] == 0
        command = json.loads((case / 'command.json').read_text())
        for flag, value in [('--mu', manifest['mu']), ('--backend', code), ('--strategy', strategy),
                            ('--threads', 32), ('--round', iteration), ('--repetitions', reps)]:
            assert command[command.index(flag)+1] == str(value)
        current = rows(case / 'samples.csv')
        assert len(current) == reps
        assert [r['repetition'] for r in current] == [str(j) for j in range(1, reps+1)]
        for r in current:
            assert (r['field'], r['backend'].lower(), int(r['mu']), r['r'], r['c'], r['threads'],
                    r['strategy'].lower(), int(r['process_round'])) == (
                    'M127', code, manifest['mu'], '2', '4', '32', strategy, iteration)
            assert r['extension_degree'] == settings[code]['extension_degree'] == '2'
            assert r['queries'] == settings[code]['queries']
            assert (r['fingerprint'], r['proof_size_bytes']) == (m['fingerprint'], m['proof_size_bytes'])
            assert signatures.setdefault(code, (r['fingerprint'], r['proof_size_bytes'])) == (
                r['fingerprint'], r['proof_size_bytes'])
            assert 0 <= float(r['first_tree_query_ms']) <= float(r['open_time_ms'])
        for metric in metric_names:
            values = [float(r[metric]) for r in current]
            near(m[f'{metric}_mean'], statistics.mean(values))
            if reps > 1:
                near(m[f'{metric}_sample_sd'], statistics.stdev(values))
        rss = int(re.search(r'Maximum resident set size \(kbytes\): (\d+)',
                            (case / 'resources.txt').read_text())[1]) / 2**20
        near(m['peak_process_rss_gib'], rss)
        canonical_raw.extend(current)
    assert canonical_raw == rows(root / 'raw-samples.csv')
    contrasts = []
    for iteration in range(1, count+1):
        for code in ('qa', 'rs'):
            group = {m['strategy']: m for m in means if int(m['process_round']) == iteration and m['backend'] == code}
            for left, right in [('native', 'recovered'), ('native', 'prehashed'), ('prehashed', 'recovered')]:
                a, b = [float(group[s]['open_time_ms_mean']) for s in (left, right)]
                contrasts.append(dict(backend=code, process_round=str(iteration), baseline=left, compared=right,
                                      baseline_open_ms=f'{a:.6f}', compared_open_ms=f'{b:.6f}',
                                      delta_ms=f'{b-a:.6f}', change_percent=f'{100*(b/a-1):.6f}'))
    assert contrasts == rows(root / 'round-contrasts.csv')
    aggregate = []
    for code in ('qa', 'rs'):
        for strategy in ('native', 'prehashed', 'recovered'):
            group = [m for m in means if m['backend'] == code and m['strategy'] == strategy]
            values = [float(m['open_time_ms_mean']) for m in group]
            row = dict(backend=code, strategy=strategy, independent_processes=count,
                       repetitions_per_process=reps, open_mean_ms=f'{statistics.mean(values):.6f}',
                       process_mean_sd_ms=f'{statistics.stdev(values):.6f}' if count > 1 else '',
                       first_tree_query_mean_ms=f"{statistics.mean(float(m['first_tree_query_ms_mean']) for m in group):.6f}",
                       max_process_rss_gib=f"{max(float(m['peak_process_rss_gib']) for m in group):.6f}")
            for index, value in enumerate(values, 1):
                row[f'round{index}_open_mean_ms'] = f'{value:.6f}'
            aggregate.append(row)
    comparison = []
    for code in ('qa', 'rs'):
        group = {r['strategy']: r for r in aggregate if r['backend'] == code}
        for left, right in [('native', 'recovered'), ('native', 'prehashed'), ('prehashed', 'recovered')]:
            a, b = [float(group[s]['open_mean_ms']) for s in (left, right)]
            matched = [r for r in contrasts if r['backend'] == code and r['baseline'] == left and r['compared'] == right]
            deltas = [float(r['delta_ms']) for r in matched]
            percentages = [float(r['change_percent']) for r in matched]
            comparison.append(dict(backend=code, baseline=left, compared=right,
                                   delta_ms=f'{b-a:.6f}', change_percent=f'{100*(b/a-1):.6f}',
                                   round_delta_sd_ms=f'{statistics.stdev(deltas):.6f}' if count > 1 else '',
                                   min_round_percent=f'{min(percentages):.6f}', max_round_percent=f'{max(percentages):.6f}'))
    if (root / 'profile').exists():
        with (root / 'profile').open('rb') as stream:
            assert hashlib.file_digest(stream, 'sha256').hexdigest() == manifest['binary_sha256']
    return manifest, means, canonical_raw, contrasts, aggregate, comparison


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', required=True, type=pathlib.Path)
    parser.add_argument('--parameters', required=True, type=pathlib.Path)
    parser.add_argument('--output', required=True, type=pathlib.Path)
    parser.add_argument('--allow-smoke', action='store_true')
    args = parser.parse_args()
    manifest, means, raw, contrasts, aggregate, comparisons = summarize(args.root, args.parameters, args.allow_smoke)
    out = args.output
    out.mkdir(parents=True, exist_ok=True)
    for name, data in [('process-means.csv', means), ('raw-samples.csv', raw), ('round-contrasts.csv', contrasts),
                       ('strategy-means.csv', aggregate), ('comparisons.csv', comparisons)]:
        write(out / name, data)
    shutil.copy2(args.root / 'manifest.json', out / 'manifest.json')
    shutil.copy2(args.root / 'finished.json', out / 'finished.json')
    shutil.copy2(args.parameters, out / 'm127-parameters.csv')
    shutil.copy2(__file__, out / 'analyze_query_rows.py')
    for name in ('metadata.json', 'implementation-plan.md'):
        if (args.root.parent / name).exists():
            shutil.copy2(args.root.parent / name, out / name)
    with tarfile.open(out / 'raw-evidence.tar.gz', 'w:gz') as archive:
        for p in sorted(args.root.rglob('*')):
            if p.is_file() and p.name != 'profile':
                archive.add(p, arcname=pathlib.Path('data') / p.relative_to(args.root))
        for name in ('run.sh', 'metadata.json', 'implementation-plan.md', 'exit-status', 'finished-utc.txt', 'stdout.log', 'stderr.log'):
            if (args.root.parent / name).exists():
                archive.add(args.root.parent / name, arcname=name)
    report = '# Queried-row recovery: constructor-controlled opening comparison\n\n'
    if args.allow_smoke:
        report += '**SMOKE ONLY: not the planned mu26/three-round experiment.**\n\n'
    report += (f"M127 μ={manifest['mu']}, r=2, c=4, quadratic extension, QA/RS, 32 Rayon threads. "
               'A = original native retention; B = prehashed retention; C = prehashed queried-row recovery. '
               'B/C share one leaf-hashing function and the same pinned native tree constructor.\n\n')
    report += '| Code | Strategy | Open mean (s) | SD across process means (s) | First-tree query (ms) | Max process RSS (GiB) |\n|---|---|---:|---:|---:|---:|\n'
    for r in aggregate:
        sd = f"{float(r['process_mean_sd_ms'])/1000:.4f}" if r['process_mean_sd_ms'] else 'n/a'
        report += f"| {r['backend'].upper()} | {r['strategy']} | {float(r['open_mean_ms'])/1000:.4f} | {sd} | {float(r['first_tree_query_mean_ms']):.4f} | {float(r['max_process_rss_gib']):.3f} |\n"
    report += '\n| Code | Contrast | Added open (ms) | Net change | Range across rounds | SD of round differences (ms) |\n|---|---|---:|---:|---:|---:|\n'
    for r in comparisons:
        report += f"| {r['backend'].upper()} | {r['compared']} − {r['baseline']} | {r['delta_ms']} | {float(r['change_percent']):+.3f}% | {float(r['min_round_percent']):+.3f}% to {float(r['max_round_percent']):+.3f}% | {r['round_delta_sd_ms'] or 'n/a'} |\n"
    report += '''
C−A is the net change relative to normal operation. B−A identifies changes from the prehashed construction entry. C−B compares retention and recovery with that entry held fixed; it includes the changed working set and allocation lifetime, rather than isolating pure arithmetic.

## Measurement structure and limits

Each backend uses three independent process rounds in ABC/BCA/CAB order. Each process uses one common commitment, one warmup, then three measured openings. Thus each strategy/backend has three independent process means and nine measured openings; the nine openings are not nine independent processes. The tables preserve each process mean, within-process sample SD and cross-process variation. Small changes comparable to these variations do not establish a stable increase or zero cost. No percentage is extrapolated to μ30.

Both Setup diagonal directions stay alive throughout all modes. Setup, commit, claimed-value evaluation, verification, final serialization and fingerprint calculation are outside open. All tree construction, dense-codeword release, recovery and multiproof work stay inside open. The optional first-tree query clock measures row access/recovery, native path extraction and cross-layer compression. A/B open the first tree before other intermediate trees; C opens it after the others have been opened and released. Its sub-timing therefore also includes ordering/cache-state differences and is not a pure recovery arithmetic timer. Peak RSS spans Setup, commit and all openings, not just query time.

The first dense intermediate codeword has a 4 GiB payload at μ26. C retains its hashes, borrows only the top codeword/diagonals/challenge in its Matrix descriptor, and releases the dense matrix after its successor is folded, before committing that successor. The measured process peak need not fall by exactly 4 GiB. This study does not demonstrate whether μ30 fits.

Every generated proof was verified, with matching prover/verifier post-challenges. Commitment, proof and post-challenge fingerprints and wire sizes match across strategies and rounds per backend. The size column counts commitment plus encoded evaluation proof. Correctness/byte equality and tampering tests also passed at both base fields and r=2/4/8, c=2/4, depths1/2/3/4, including 1/4/32-thread comparisons and a test-only counter showing no lazy-row access during tree construction. Tests are not a cryptographic security proof.

## Reproduction

Source, native executable hash, exact commands/status, thread count and seeds are in manifest and the raw evidence archive. Seeds are 42/27/28 for Setup/message/point. Hardware is AMD EPYC 9754, 128 physical / 256 logical CPUs, four NUMA nodes, about 503 GiB RAM, Rust 1.95.0. Native CPU release flags; no CPU pinning and a shared host. The same certified uniform-profile M127 rows and unchanged Plonky3 pin are used; the two relevant parameter rows are checked against each raw record. The official 142 configurations and 426 samples remain unchanged.

From the source commit recorded in manifest, with the existing pinned local Plonky3 checkout:

```sh
RUSTFLAGS='-C target-cpu=native' cargo build --offline --locked --release --example query_row_overhead
python3 scripts/profile_query_rows.py --binary target/release/examples/query_row_overhead --output-dir /tmp/query-row-new-empty
```

To reproduce all numeric analysis without the executable, from this report folder:

```sh
mkdir evidence
tar -xzf raw-evidence.tar.gz -C evidence
python3 analyze_query_rows.py --root evidence/data --parameters m127-parameters.csv --output regenerated
```

Raw archive includes every case command, zero-exit status, GNU time resource record and measured CSV; its large executable is omitted but SHA-256 retained. The analyzer checks process order, parameter queries, repetitions, fingerprints, resource peaks, means/SDs and contrasts against raw evidence. SHA256SUMS covers all other snapshot files.
'''
    report += f"\nSource commit: `{manifest['source_commit']}`. Executable SHA-256: `{manifest['binary_sha256']}`.\n"
    (out / 'REPORT.md').write_text(report)
    (out / 'SHA256SUMS').write_text(''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n'
                                          for p in sorted(out.iterdir()) if p.is_file() and p.name != 'SHA256SUMS'))
    print(json.dumps(dict(profiles=len(means), raw_samples=len(raw), strategies=6, output=str(out))))


if __name__ == '__main__':
    main()
