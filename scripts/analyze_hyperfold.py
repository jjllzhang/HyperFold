#!/usr/bin/env python3
"""Validate final retained HyperFold evidence and produce tables/standalone figures.
Run only after BOTH the base traversal and actual-attempt queue have finished.
"""
import argparse
import csv
import hashlib
import io
import json
import math
import pathlib
import re
import shutil
import statistics
import subprocess
import tarfile

KEY_FIELDS = ('field', 'backend', 'mu', 'r', 'c')
TIMES = ('commit_time_ms', 'open_time_ms', 'verify_time_ms', 'diagonal_transition_ms')
MEASUREMENTS = TIMES + ('commitment_bytes', 'eval_proof_bytes', 'proof_size_bytes', 'proof_size_kb')


def read_csv(path):
    with path.open(newline='') as stream:
        return list(csv.DictReader(stream))


def key(row):
    return tuple(row[k] for k in KEY_FIELDS)


def write_csv(path, data):
    with path.open('w', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=list(data[0]))
        writer.writeheader()
        writer.writerows(data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', required=True, type=pathlib.Path)
    parser.add_argument('--output', required=True, type=pathlib.Path)
    parser.add_argument('--repo', type=pathlib.Path, default=pathlib.Path('/home/zjl/HyperFold'))
    args = parser.parse_args()
    root, out = args.root.resolve(), args.output.resolve()
    if not (root / 'final-finished.json').exists():
        raise SystemExit('Not ready: actual-attempt final-finished.json is absent.')
    assert (root / 'exit-status').read_text().strip() == '0'
    assert (root / 'actual-attempt-exit-status').read_text().strip() == '0'
    summary = json.loads((root / 'final-finished.json').read_text())
    manifest = json.loads((root / 'manifest.json').read_text())
    with (root / 'bench').open('rb') as stream:
        assert hashlib.file_digest(stream, 'sha256').hexdigest() == manifest['contract']['binary_sha256']
    expected = list(csv.DictReader(io.StringIO(subprocess.check_output([
        str(root / 'bench'), '--list', '--memory-mode', 'retained', '--max-estimated-gib', '1024'
    ], text=True))))
    expected = {key(r): r for r in expected}
    means = read_csv(root / 'final-results.csv')
    raw = read_csv(root / 'final-results.raw.csv')
    assert len(means) == len(expected) == 144
    assert len({key(r) for r in means}) == 144 and set(map(key, means)) == set(expected)
    assert len(raw) == summary['raw_samples'] == 3 * summary['measured']
    groups = {}
    for row in raw:
        groups.setdefault(key(row), []).append(row)
    enriched = []
    resource_rows = []
    stopped = []
    for row in means:
        assert row['status'] in ('measured', 'memory_guard_stopped')
        assert row['low_memory'] == 'false' and row['threads'] == '32' and row['repetitions'] == '3'
        spec = expected[key(row)]
        assert all(row[k] == spec[k] for k in ('mu0', 'extension_degree', 'queries'))
        name = '-'.join(key(row))
        attempt = root / 'actual-attempts' / name
        directory = attempt if (attempt / 'status.json').exists() else root / 'cases' / name
        state = json.loads((directory / 'status.json').read_text())
        samples = groups.get(key(row), [])
        if row['status'] == 'measured':
            assert state['exit_code'] == 0 and not state.get('memory_guard', False)
            assert len(samples) == 3 and {s['repetition'] for s in samples} == {'1', '2', '3'}
            assert all(s['low_memory'] == 'false' and s['certified_iop'] == 'true' for s in samples)
            assert all(all(s[k] == row[k] for k in KEY_FIELDS + ('mu0', 'extension_degree', 'queries')) for s in samples)
            for metric in TIMES:
                assert abs(float(row[metric]) - statistics.mean(float(s[metric]) for s in samples)) < 2e-6
            assert int(row['proof_size_bytes']) == int(row['commitment_bytes']) + int(row['eval_proof_bytes'])
            assert all(int(s['commitment_bytes']) == int(row['commitment_bytes']) and
                       int(s['eval_proof_bytes']) == int(row['eval_proof_bytes']) for s in samples)
        else:
            assert not samples and all(not row[k] for k in MEASUREMENTS)
            assert state['memory_guard'] and state['exit_code'] != 0
            stopped.append(name)
        resource = (directory / 'resources.txt').read_text()
        match = re.search(r'Maximum resident set size \(kbytes\): (\d+)', resource)
        rss_kib = int(match[1]) if match else int(state['observed_peak_hwm_kib'])
        command = json.loads((directory / 'command.json').read_text())
        assert command[command.index('--memory-mode') + 1] in ('retained', 'auto')
        # Reused auto-selected cases are proven retained by all their recorded flags.
        record = dict(row, peak_rss_gib=f'{rss_kib / 2**20:.6f}',
                      rss_source='gnu_time' if match else 'sampled_hwm',
                      evidence_directory=str(directory.relative_to(root)),
                      explicit_actual_attempt=str(directory == attempt).lower())
        for metric in TIMES[:3]:
            record[metric.replace('_ms', '_sample_sd_ms')] = (
                f'{statistics.stdev(float(s[metric]) for s in samples):.6f}' if samples else '')
        enriched.append(record)
        resource_rows.append({k: record[k] for k in KEY_FIELDS +
                              ('status', 'peak_rss_gib', 'rss_source', 'evidence_directory', 'explicit_actual_attempt')})
    assert len(stopped) == summary['memory_guard_stopped']
    out.mkdir(parents=True, exist_ok=True)
    write_csv(out / 'results.csv', enriched)
    write_csv(out / 'raw-samples.csv', raw)
    write_csv(out / 'resources.csv', resource_rows)
    for name in ('manifest.json', 'reused-source-manifest.json', 'reuse-validation.json',
                 'actual-attempt-policy.json', 'final-finished.json', 'policy-directive.json'):
        shutil.copy2(root / name, out / name)
    parameter_dir = out / 'parameters'
    parameter_dir.mkdir(exist_ok=True)
    for name in ('goldilocks-parameters.csv', 'm127-parameters.csv', 'SHA256SUMS'):
        shutil.copy2(args.repo / 'data' / name, parameter_dir / name)
    shutil.copy2(pathlib.Path(__file__), out / 'analyze_hyperfold.py')
    indexed = {key(r): r for r in enriched}
    paired = []
    for q in enriched:
        if q['backend'] != 'Qa' or q['status'] != 'measured':
            continue
        r = indexed[(q['field'], 'Rs', q['mu'], q['r'], q['c'])]
        if r['status'] != 'measured':
            continue
        ratio = {k: q[k] for k in ('field','mu','r','c')}
        for metric in TIMES[:3] + ('proof_size_bytes',):
            ratio[metric + '_qa_over_rs'] = f"{float(q[metric])/float(r[metric]):.9f}"
        paired.append(ratio)
    write_csv(out / 'paired-qa-rs-ratios.csv', paired)
    # Import matplotlib only after all integrity checks; no plots run during timings.
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    by_key = {key(r): r for r in enriched}
    metrics = [('commit_time_ms', 'Commit (ms)', 1), ('open_time_ms', 'Open (ms)', 1),
               ('verify_time_ms', 'Verify (ms)', 1), ('proof_size_bytes', 'Commitment + proof (KiB)', 1024)]
    for metric, label, divisor in metrics:
        fig, axes = plt.subplots(2, 3, figsize=(12, 6.6), sharex=True)
        for field_index, field in enumerate(('Goldilocks', 'M127')):
            for arity_index, arity in enumerate(('2', '4', '8')):
                ax = axes[field_index, arity_index]
                for backend, color in (('Qa', '#0072B2'), ('Rs', '#D55E00')):
                    for rate, style in (('2', '-'), ('4', '--')):
                        ys, errors = [], []
                        for mu in (20, 22, 24, 26, 28, 30):
                            row = by_key[(field, backend, str(mu), arity, rate)]
                            ys.append(float(row[metric]) / divisor if row['status'] == 'measured' else math.nan)
                            samples = groups.get(key(row), [])
                            errors.append(statistics.stdev(float(x[metric]) for x in samples) / divisor
                                          if samples and metric in TIMES else 0)
                        if metric in TIMES:
                            ax.errorbar((20,22,24,26,28,30), ys, yerr=errors, color=color, linestyle=style,
                                        marker='o', markersize=3, capsize=2, elinewidth=.7,
                                        label=f'{backend.upper()}, c={rate}')
                        else:
                            ax.plot((20,22,24,26,28,30), ys, color=color, linestyle=style,
                                    marker='o', markersize=3, label=f'{backend.upper()}, c={rate}')
                ax.set_yscale('log')
                ax.set_title(f'{field}, r={arity}', fontsize=10)
                ax.grid(True, which='major', alpha=.25)
                ax.set_xticks((20,22,24,26,28,30))
                if arity_index == 0: ax.set_ylabel(label)
                if field_index == 1: ax.set_xlabel('Multilinear variables μ')
        handles, labels = axes[0,0].get_legend_handles_labels()
        fig.legend(handles, labels, loc='upper center', bbox_to_anchor=(.5,.955), ncol=4, frameon=False)
        fig.suptitle('HyperFold · 32 threads · retained trees · 100-bit IOP parameters', y=.995, fontsize=12)
        fig.text(.5, .015, 'Mean ± sample SD (3 repetitions); gaps are unmeasured. Goldilocks cubic / M127 quadratic. '
                 'One fixed Setup seed.', ha='center', fontsize=8)
        fig.subplots_adjust(left=.08, right=.99, bottom=.13, top=.85, hspace=.25, wspace=.20)
        stem = metric.replace('_time_ms', '').replace('_size_bytes', '_size')
        for extension in ('svg', 'png'):
            fig.savefig(out / f'{stem}.{extension}', dpi=180, bbox_inches='tight')
        plt.close(fig)
    peak = max(enriched, key=lambda r: float(r['peak_rss_gib']))
    report = [
        '# HyperFold retained-memory experiment report', '',
        f"Validated {summary['processed']} configurations: {summary['measured']} measured, "
        f"{summary['memory_guard_stopped']} stopped by the live available-memory guard; "
        f"{summary['raw_samples']} completed raw samples. No recomputation was used.", '',
        'Each measured configuration used 32 Rayon threads, one warmup and three measured repetitions. '
        'Commit/open/verify means exclude Setup and final serialization. Proof size includes the '
        'commitment and evaluation proof, including wire framing. `raw-samples.csv` contains every '
        'completed repetition; `results.csv` adds sample standard deviations and observed peak RSS.', '',
        'The two largest M127 configurations were actually attempted with retained trees, bypassing '
        'the estimate ceiling. A guard stop means host available memory crossed the 12 GiB reserve. '
        'It is not an operating-system OOM event or proof that every memory policy must fail. '
        'Partial attempt data remains in the evidence archive; incomplete aggregate measurements are blank.', '',
        f"Maximum recorded RSS: {float(peak['peak_rss_gib']):.3f} GiB at {' / '.join(key(peak))}. "
        'Resource usage covers the full case process, including Setup and warmup.', '',
        '## Reproduction and limits', '',
        f"Pinned executable SHA-256: `{manifest['contract']['binary_sha256']}`. "
        f"Source at policy migration: `{manifest['source_commit']}`; the original binary-build "
        'provenance is preserved in `reused-source-manifest.json`. Plonky3 commit '
        '`0d803737b681774186cd283b1fd2d8e493655c82`; AMD EPYC 9754, 503 GiB host RAM, '
        'Rust/Cargo 1.95.0, native CPU flags. Commands, seeds, hardware and per-case raw/resource '
        'records are in the manifest and evidence archive.', '',
        'Parameters use the 144 official uniform-profile rows, Goldilocks cubic and M127 quadratic, '
        'certificate `hfc-tight-total100-gold-m127-v3-2026-11-05`. The stated 100-bit bound concerns '
        'the specified interactive IOP/commit-binding budget. Correctness tests and these measurements '
        'are not a security proof of the implementation, Fiat–Shamir, Merkle or seeded Setup.', '',
        'This is one machine and one Setup seed, with three repetitions, no CPU pinning and shared '
        'host/NUMA scheduling. Encoder contracts differ: Goldilocks RS uses coefficient FFT encoding; '
        'M127 RS uses systematic Lagrange encoding. Compare QA/RS at matched field, μ, r and c.', '',
        '## Figures', '',
    ]
    for stem in ('commit','open','verify','proof_size'):
        report += [f'![{stem}]({stem}.png)', '']
    report += ['## Matched QA/RS comparisons', '',
               'The table gives the median QA/RS ratio across matched field/μ/r/c settings. '
               'Below 1 means less time or fewer bytes for QA. These are descriptive ratios over '
               'this experiment matrix, not population estimates. Query counts differ by the '
               'certified base-code distance bounds; verification and proof-size ratios include that effect.', '',
               '| Field | Matched pairs | Commit | Open | Verify | Total bytes |',
               '|---|---:|---:|---:|---:|---:|']
    for field in ('Goldilocks','M127'):
        subset = [r for r in paired if r['field'] == field]
        values = [statistics.median(float(r[m + '_qa_over_rs']) for r in subset)
                  for m in TIMES[:3] + ('proof_size_bytes',)]
        report.append('| ' + field + ' | ' + str(len(subset)) + ' | '
                      + ' | '.join(f'{v:.3f}' for v in values) + ' |')
    report += ['']
    report += ['## Actual extreme-case attempts', '',
               '| Case | Start UTC | Finish UTC | Sampled HWM GiB | Minimum host available GiB | Stop reason |',
               '|---|---|---|---:|---:|---|']
    for name in ('M127-Qa-30-2-4', 'M127-Rs-30-2-4'):
        state = json.loads((root / 'actual-attempts' / name / 'status.json').read_text())
        report.append(f"| {name} | {state['started_utc']} | {state['finished_utc']} | "
                      f"{state['observed_peak_hwm_kib']/2**20:.6f} | "
                      f"{state['minimum_host_available_bytes']/2**30:.6f} | {state['stop_reason']} |")
    report += ['']
    report += ['## μ=30 configurations', '',
               '| Field | Backend | r | c | Status | Commit ms | Open ms | Verify ms | Total bytes | Peak GiB |',
               '|---|---|---:|---:|---|---:|---:|---:|---:|---:|']
    for row in enriched:
        if row['mu'] == '30':
            values = [row[k] for k in ('field','backend','r','c','status','commit_time_ms','open_time_ms',
                                      'verify_time_ms','proof_size_bytes','peak_rss_gib')]
            report.append('| ' + ' | '.join(values) + ' |')
    (out / 'REPORT.md').write_text('\n'.join(report) + '\n')
    with tarfile.open(out / 'raw-evidence.tar.gz', 'w:gz') as archive:
        for path in sorted(root.rglob('*')):
            if path.is_file() and path != root / 'bench' and not path.name.endswith('.lock'):
                archive.add(path, arcname=str(path.relative_to(root)), recursive=False)
    hashes = []
    for path in sorted(out.rglob('*')):
        if path.is_file() and path.name != 'SHA256SUMS':
            with path.open('rb') as stream:
                hashes.append(f"{hashlib.file_digest(stream, 'sha256').hexdigest()}  {path.relative_to(out)}")
    (out / 'SHA256SUMS').write_text('\n'.join(hashes) + '\n')
    print(json.dumps(dict(summary, report_directory=str(out)), indent=2))


if __name__ == '__main__':
    main()
