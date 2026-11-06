#!/usr/bin/env python3
"""Profile existing first-tree rebuild strategy against retained state at fitting sizes.
Two-direction diagonals and one common commitment per process; no switch timings.
"""
import argparse
import csv
import hashlib
import json
import pathlib
import re
import shutil
import statistics
import subprocess

from run_sweep import rows, utc, write_table


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=pathlib.Path)
    parser.add_argument('--output-dir', required=True, type=pathlib.Path)
    args = parser.parse_args()
    root = args.output_dir.resolve()
    root.mkdir(parents=True, exist_ok=True)
    if any(root.iterdir()):
        raise RuntimeError('profile evidence directory must be empty')
    shutil.copy2(args.binary.resolve(), root / 'profile')
    with (root / 'profile').open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    repo = pathlib.Path(__file__).resolve().parents[1]
    jobs = []
    for index, mu in enumerate((24, 26)):
        for backend_index, backend in enumerate(('qa', 'rs')):
            strategies = ['retained', 'rebuild']
            if (index + backend_index) % 2:
                strategies.reverse()
            jobs.extend((mu, backend, strategy) for strategy in strategies)
    manifest = {'started_utc': utc(), 'binary_sha256': digest,
                'source_commit': subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip(),
                'source_status': subprocess.check_output(['git', '-C', str(repo), 'status', '--short'], text=True).strip(),
                'field': 'M127', 'r': 2, 'c': 4, 'threads': 32, 'warmup': 1, 'repetitions': 3,
                'diagonals': 'both directions retained; no conversions',
                'commitments': 'one common commitment per process, outside open timer',
                'setup_seed_byte': 42, 'message_seed_byte': 27, 'point_seed_byte': 28,
                'strategy_order': jobs, 'build_flags': '-C target-cpu=native',
                'scope': 'diagnostic opening-strategy comparison; official sweep data unchanged'}
    (root / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    samples, means = [], []
    for index, (mu, backend, strategy) in enumerate(jobs):
        key = f'M127-{backend}-{mu}-r2-c4-{strategy}'
        directory = root / 'cases' / key
        directory.mkdir(parents=True)
        command = ['/usr/bin/time', '-v', '-o', str(directory / 'resources.txt'), str(root / 'profile'),
                   '--mu', str(mu), '--backend', backend, '--strategy', strategy, '--threads', '32',
                   '--repetitions', '3', '--output', str(directory / 'samples.csv')]
        (directory / 'command.json').write_text(json.dumps(command, indent=2) + '\n')
        (root / 'progress.json').write_text(json.dumps({'completed': index, 'total': len(jobs),
                                                       'current': key, 'updated_utc': utc()}) + '\n')
        started = utc()
        with (directory / 'stdout.log').open('w') as stdout, (directory / 'stderr.log').open('w') as stderr:
            process = subprocess.run(command, stdout=stdout, stderr=stderr)
        (directory / 'status.json').write_text(json.dumps({'started_utc': started, 'finished_utc': utc(),
                                                         'exit_code': process.returncode}) + '\n')
        if process.returncode:
            raise RuntimeError(f'profile {key} failed; preserve evidence')
        current = rows(directory / 'samples.csv')
        assert len(current) == 3 and {r['repetition'] for r in current} == {'1', '2', '3'}
        assert all(r['mu'] == str(mu) and r['backend'].lower() == backend and r['strategy'].lower() == strategy
                   and r['threads'] == '32' and r['extension_degree'] == '2' for r in current)
        assert len({r['fingerprint'] for r in current}) == 1
        rss = int(re.search(r'Maximum resident set size \(kbytes\): (\d+)',
                            (directory / 'resources.txt').read_text())[1]) / 2**20
        row = dict(field='M127', backend=backend, mu=str(mu), r='2', c='4', strategy=strategy,
                   threads='32', repetitions='3',
                   open_mean_ms=f"{statistics.mean(float(r['open_time_ms']) for r in current):.6f}",
                   open_sample_sd_ms=f"{statistics.stdev(float(r['open_time_ms']) for r in current):.6f}",
                   verify_mean_ms=f"{statistics.mean(float(r['verify_time_ms']) for r in current):.6f}",
                   proof_size_bytes=current[0]['proof_size_bytes'], fingerprint=current[0]['fingerprint'],
                   peak_process_rss_gib=f'{rss:.6f}')
        means.append(row)
        samples.extend(current)
        write_table(root / 'means.csv', means)
        write_table(root / 'raw-samples.csv', samples)
        paired = []
        for size in (24, 26):
            for code in ('qa', 'rs'):
                matched = {r['strategy']: r for r in means if r['mu'] == str(size) and r['backend'] == code}
                if len(matched) != 2:
                    continue
                a, b = matched['retained'], matched['rebuild']
                assert a['fingerprint'] == b['fingerprint'] and a['proof_size_bytes'] == b['proof_size_bytes']
                normal, rebuilt = float(a['open_mean_ms']), float(b['open_mean_ms'])
                paired.append(dict(field='M127', backend=code, mu=size, r=2, c=4,
                                   retained_open_ms=a['open_mean_ms'], rebuild_open_ms=b['open_mean_ms'],
                                   delta_ms=f'{rebuilt-normal:.6f}', overhead_percent=f'{100*(rebuilt/normal-1):.3f}',
                                   retained_sample_sd_ms=a['open_sample_sd_ms'],
                                   rebuild_sample_sd_ms=b['open_sample_sd_ms'], fingerprint=a['fingerprint']))
        write_table(root / 'paired.csv', paired)
    (root / 'finished.json').write_text(json.dumps({'finished_utc': utc(), 'profiles': len(means),
                                                   'paired_configs': len(paired), 'raw_samples': len(samples)}) + '\n')
    (root / 'progress.json').write_text(json.dumps({'completed': len(jobs), 'total': len(jobs),
                                                   'updated_utc': utc(), 'phase': 'finished'}) + '\n')


if __name__ == '__main__':
    main()
