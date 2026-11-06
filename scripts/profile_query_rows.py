#!/usr/bin/env python3
"""Three-constructor-controlled rounds of isolated M127 queried-row openings.
A/native, B/prehashed-retained, C/prehashed-recovered; official sweep unchanged.
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

from run_sweep import utc, write_table


def read_rows(path):
    with path.open() as stream:
        return list(csv.DictReader(stream))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=pathlib.Path)
    parser.add_argument('--output-dir', required=True, type=pathlib.Path)
    parser.add_argument('--mu', type=int, default=26)
    parser.add_argument('--rounds', type=int, default=3, choices=(1, 2, 3))
    parser.add_argument('--repetitions', type=int, default=3)
    args = parser.parse_args()
    if args.repetitions < 1:
        parser.error('positive repetitions required')
    root = args.output_dir.resolve()
    root.mkdir(parents=True, exist_ok=True)
    if any(root.iterdir()):
        raise RuntimeError('evidence directory must be empty')
    shutil.copy2(args.binary.resolve(), root / 'profile')
    with (root / 'profile').open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    repo = pathlib.Path(__file__).resolve().parents[1]
    orders = [('native', 'prehashed', 'recovered'), ('prehashed', 'recovered', 'native'),
              ('recovered', 'native', 'prehashed')]
    jobs = [(iteration + 1, backend, strategy)
            for iteration, order in enumerate(orders[:args.rounds])
            for backend in ('qa', 'rs') for strategy in order]
    manifest = dict(started_utc=utc(), binary_sha256=digest,
                    source_commit=subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip(),
                    source_status=subprocess.check_output(['git', '-C', str(repo), 'status', '--short'], text=True).strip(),
                    field='M127', mu=args.mu, r=2, c=4, threads=32,
                    warmup=1, repetitions=args.repetitions, process_rounds=args.rounds,
                    diagonal_tables='both directions held live; no conversions or releases',
                    one_common_commitment_per_process=True, setup_seed_byte=42,
                    message_seed_byte=27, point_seed_byte=28, jobs=jobs,
                    build_flags='-C target-cpu=native',
                    first_tree_query_scope='row access/recovery, multiproof extraction and cross-layer compression; in open total',
                    plan='/home/zjl/zkp-research/notes/impl-plan-hyperfold-query-row-recovery.md',
                    directive='fd3f83db-4b9a-4b58-9302-ba5f2acf4c45',
                    scope='separate diagnostic; never overwrite official 142 retained cases')
    (root / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    means, samples, fingerprints = [], [], {}
    for index, (iteration, backend, strategy) in enumerate(jobs):
        key = f'M127-{backend}-{args.mu}-r2-c4-round{iteration}-{strategy}'
        directory = root / 'cases' / key
        directory.mkdir(parents=True)
        command = ['/usr/bin/time', '-v', '-o', str(directory / 'resources.txt'), str(root / 'profile'),
                   '--mu', str(args.mu), '--backend', backend, '--strategy', strategy, '--threads', '32',
                   '--round', str(iteration), '--repetitions', str(args.repetitions),
                   '--output', str(directory / 'samples.csv')]
        (directory / 'command.json').write_text(json.dumps(command, indent=2) + '\n')
        (root / 'progress.json').write_text(json.dumps(dict(completed=index, total=len(jobs), current=key,
                                                         updated_utc=utc())) + '\n')
        started = utc()
        with (directory / 'stdout.log').open('w') as stdout, (directory / 'stderr.log').open('w') as stderr:
            result = subprocess.run(command, stdout=stdout, stderr=stderr)
        (directory / 'status.json').write_text(json.dumps(dict(started_utc=started, finished_utc=utc(),
                                                             exit_code=result.returncode)) + '\n')
        if result.returncode:
            raise RuntimeError(f'failed {key}; evidence preserved')
        current = read_rows(directory / 'samples.csv')
        assert len(current) == args.repetitions
        assert {r['repetition'] for r in current} == {str(j) for j in range(1, args.repetitions + 1)}
        assert all(r['field'] == 'M127' and r['mu'] == str(args.mu) and r['r'] == '2' and r['c'] == '4'
                   and r['backend'].lower() == backend and r['strategy'].lower() == strategy
                   and r['threads'] == '32' and r['extension_degree'] == '2'
                   and r['process_round'] == str(iteration) for r in current)
        signatures = {(r['fingerprint'], r['proof_size_bytes']) for r in current}
        assert len(signatures) == 1
        signature = signatures.pop()
        assert fingerprints.setdefault(backend, signature) == signature
        row = dict(field='M127', backend=backend, mu=args.mu, r=2, c=4, strategy=strategy,
                   process_round=iteration, threads=32, repetitions=args.repetitions,
                   fingerprint=signature[0], proof_size_bytes=signature[1])
        for metric in ('open_time_ms', 'first_tree_query_ms', 'verify_time_ms'):
            values = [float(r[metric]) for r in current]
            row[f'{metric}_mean'] = f'{statistics.mean(values):.6f}'
            row[f'{metric}_sample_sd'] = f'{statistics.stdev(values):.6f}' if len(values) > 1 else ''
        row['peak_process_rss_gib'] = f"{int(re.search(r'Maximum resident set size \(kbytes\): (\d+)', (directory / 'resources.txt').read_text())[1]) / 2**20:.6f}"
        means.append(row)
        samples.extend(current)
        write_table(root / 'process-means.csv', means)
        write_table(root / 'raw-samples.csv', samples)
        contrasts = []
        for run in range(1, args.rounds + 1):
            for code in ('qa', 'rs'):
                matched = {r['strategy']: r for r in means if r['process_round'] == run and r['backend'] == code}
                if len(matched) != 3:
                    continue
                for left, right in [('native', 'recovered'), ('native', 'prehashed'), ('prehashed', 'recovered')]:
                    a, b = (float(matched[s]['open_time_ms_mean']) for s in (left, right))
                    contrasts.append(dict(backend=code, process_round=run, baseline=left, compared=right,
                                          baseline_open_ms=f'{a:.6f}', compared_open_ms=f'{b:.6f}',
                                          delta_ms=f'{b-a:.6f}', change_percent=f'{100*(b/a-1):.6f}'))
        write_table(root / 'round-contrasts.csv', contrasts)
    (root / 'finished.json').write_text(json.dumps(dict(finished_utc=utc(), profiles=len(means),
                                                       raw_samples=len(samples), process_rounds=args.rounds)) + '\n')
    (root / 'progress.json').write_text(json.dumps(dict(completed=len(jobs), total=len(jobs),
                                                       phase='finished', updated_utc=utc())) + '\n')


if __name__ == '__main__':
    main()
