#!/usr/bin/env python3
"""Independent fresh-Setup trials for retained-vs-opening-only lifecycles.
Pilot defaults: mu26 QA/RS, two AB/BA rounds, 1 warmup +3 repetitions per process.
Largest follow-up: --mu 30 --modes release --rounds 1 --continue-on-failure.
"""
import argparse
import json
import pathlib
import re
import shutil
import statistics
import subprocess

from attempt_retained_extremes import monitor_command, prepare_attempt
from run_sweep import rows, sha, utc, write_table

METRICS = ('setup_ms', 'input_ms', 'commit_ms', 'forward_release_ms', 'open_ms',
           'witness_release_ms', 'first_tree_query_ms', 'verify_ms', 'serialization_ms',
           'diagnostics_ms', 'cleanup_ms', 'wall_ms', 'online_total_ms')


def write_allow_empty(path, entries):
    if entries:
        write_table(path, entries)
    else:
        path.write_text('')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=pathlib.Path)
    parser.add_argument('--root', required=True, type=pathlib.Path)
    parser.add_argument('--mu', type=int, default=26)
    parser.add_argument('--modes', default='retained,release')
    parser.add_argument('--rounds', type=int, default=2)
    parser.add_argument('--repetitions', type=int, default=3)
    parser.add_argument('--continue-on-failure', action='store_true')
    args = parser.parse_args()
    modes = args.modes.split(',')
    if not modes or len(set(modes)) != len(modes) or not set(modes) <= {'retained', 'release'}:
        parser.error('modes must select retained/release once each')
    if args.rounds < 1 or args.repetitions < 1:
        parser.error('positive rounds/repetitions required')
    root = args.root.resolve()
    root.mkdir(parents=True, exist_ok=True)
    if any(root.iterdir()):
        raise RuntimeError('new empty evidence directory required')
    shutil.copy2(args.binary.resolve(), root / 'profile')
    repo = pathlib.Path(__file__).resolve().parents[1]
    jobs = [(iteration+1, code, mode) for iteration in range(args.rounds)
            for code in ('qa', 'rs') for mode in (modes if iteration % 2 == 0 else modes[::-1])]
    manifest = dict(started_utc=utc(), source_commit=subprocess.check_output(
        ['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip(),
        source_status=subprocess.check_output(['git', '-C', str(repo), 'status', '--short'], text=True).strip(),
        binary_sha256=sha(root / 'profile'), mu=args.mu, r=2, c=4, field='M127', threads=32,
        process_rounds=args.rounds, repetitions=args.repetitions, warmup=1, setup_per_trial=True,
        first_tree_strategy='prehashed_recovered', jobs=jobs, modes=modes,
        memory_guard_enabled=False, estimate_exclusions=False, whole_tree_rebuild=False,
        directive='69fd73e0-2c8f-43a0-a570-356f7828c11e',
        setup_seed_byte=42, message_seed_byte=27, point_seed_byte=28,
        plan='/home/zjl/zkp-research/notes/impl-plan-hyperfold-one-shot-lifetimes.md',
        build_flags='-C target-cpu=native',
        wall_scope='Setup/input+claimed evaluation/commit/forward release/open/verify/serialization/diagnostics/cleanup; CSV IO excluded',
        online_scope='commit+forward_release+open+verify; Setup/input/serialization/diagnostics/cleanup separately reported',
        scope='separate lifecycle experiment; never overwrite official142, prior diagnostics or actual failures')
    (root / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    policy = dict(enabled=True, memory_guard_enabled=False, directive=manifest['directive'],
                  first_tree_strategy='prehashed_recovered', setup_per_trial=True)
    policy_path = root / 'policy.json'
    policy_path.write_text(json.dumps(policy, indent=2) + '\n')

    def enabled():
        return json.loads(policy_path.read_text()) == policy

    samples, outcomes, signatures = [], [], {}
    for iteration, code, mode in jobs:
        key = f'M127-{code}-{args.mu}-r2-c4-round{iteration}-{mode}'
        directory = root / 'cases' / key
        directory.mkdir(parents=True)
        command = ['/usr/bin/time', '-v', '-o', str(directory / 'resources.txt'), str(root / 'profile'),
                   '--mu', str(args.mu), '--backend', code, '--mode', mode, '--threads', '32',
                   '--trial-round', str(iteration), '--repetitions', str(args.repetitions),
                   '--output', str(directory / 'samples.csv')]
        contract = dict(binary_sha256=manifest['binary_sha256'], command=command, mode=mode,
                        setup_per_trial=True, memory_guard_enabled=False, repetitions=args.repetitions,
                        directive=manifest['directive'])
        assert prepare_attempt(directory, command, contract) is None
        (root / 'progress.json').write_text(json.dumps(dict(completed=len(outcomes), total=len(jobs),
                                                           current=key, updated_utc=utc())) + '\n')
        if not enabled():
            raise RuntimeError('policy revoked')
        status = monitor_command(command, directory, None, enabled)
        status['execution_contract'] = contract
        (directory / 'status.json').write_text(json.dumps(status, indent=2) + '\n')
        if status['stop_reason'] == 'policy_revoked':
            raise RuntimeError('policy revoked; evidence preserved')
        current = rows(directory / 'samples.csv') if (directory / 'samples.csv').exists() else []
        row = dict(field='M127', backend=code, mu=args.mu, r=2, c=4, mode=mode,
                   trial_round=iteration, threads=32, repetitions=args.repetitions,
                   first_tree_strategy='prehashed_recovered', setup_per_trial=True,
                   parameter_lifecycle='opening_only' if mode == 'release' else 'reusable_both',
                   witness_lifecycle='consumed_after_first_round' if mode == 'release' else 'borrowed_retained',
                   exit_code=status['exit_code'], memory_guard_enabled=False, recorded_samples=len(current))
        if status['exit_code'] == 0:
            assert len(current) == args.repetitions
            assert [s['repetition'] for s in current] == [str(j) for j in range(1, args.repetitions+1)]
            for sample in current:
                assert sample['backend'].lower() == code and sample['mode'].lower() == mode
                assert (sample['mu'], sample['r'], sample['c'], sample['threads']) == (str(args.mu), '2', '4', '32')
                assert sample['trial_round'] == str(iteration) and sample['setup_per_trial'] == 'true'
                assert sample['first_tree_strategy'] == 'prehashed_recovered'
                assert sample['parameter_lifecycle'] == row['parameter_lifecycle']
                assert sample['witness_lifecycle'] == row['witness_lifecycle']
                assert sample['extension_degree'] == '2' and int(sample['inverse_capacity_elements']) > 0
                if mode == 'release':
                    assert sample['forward_capacity_elements'] == '0'
                    assert int(sample['released_witness_elements']) == 1 << args.mu
                    assert sample['witness_release_level'] == sample['depth'] and sample['witness_release_subround'] == '0'
                else:
                    assert int(sample['forward_capacity_elements']) > 0 and sample['released_witness_elements'] == '0'
            signature = {(s['fingerprint'], s['proof_size_bytes']) for s in current}
            assert len(signature) == 1
            signature = signature.pop()
            assert signatures.setdefault(code, signature) == signature
            row.update(status='measured', fingerprint=signature[0], proof_size_bytes=signature[1])
            for metric in METRICS:
                values = [float(s[metric]) for s in current]
                row[f'{metric}_mean'] = f'{statistics.mean(values):.6f}'
                row[f'{metric}_sample_sd'] = f'{statistics.stdev(values):.6f}' if len(values) > 1 else ''
            samples.extend(current)
        else:
            row.update(status='runtime_failed', fingerprint='', proof_size_bytes='')
            for metric in METRICS:
                row[f'{metric}_mean'] = row[f'{metric}_sample_sd'] = ''
        resource_text = (directory / 'resources.txt').read_text()
        row['gnu_peak_rss_gib'] = f"{int(re.search(r'Maximum resident set size \(kbytes\): (\d+)', resource_text)[1])/2**20:.6f}"
        signal = re.search(r'Command terminated by signal (\d+)', resource_text)
        row['termination_signal'] = signal[1] if signal else ''
        row['global_oom_kill_delta'] = status['system_after']['vmstat']['oom_kill']-status['system_before']['vmstat']['oom_kill']
        row['minimum_available_gib'] = f"{status['minimum_host_available_bytes']/2**30:.6f}"
        outcomes.append(row)
        write_table(root / 'outcomes.csv', outcomes)
        write_allow_empty(root / 'raw-samples.csv', samples)
        if status['exit_code'] and not args.continue_on_failure:
            raise RuntimeError(f'{key} failed; preserve evidence and repair before largest attempts')
    summary = dict(finished_utc=utc(), processes=len(jobs), measured=sum(r['status'] == 'measured' for r in outcomes),
                   runtime_failed=sum(r['status'] == 'runtime_failed' for r in outcomes),
                   complete_raw_samples=len(samples), setup_per_trial=True, first_tree_strategy='prehashed_recovered',
                   memory_guard_enabled=False)
    (root / 'finished.json').write_text(json.dumps(summary, indent=2) + '\n')
    (root / 'progress.json').write_text(json.dumps(dict(phase='finished', **summary)) + '\n')


if __name__ == '__main__':
    main()
