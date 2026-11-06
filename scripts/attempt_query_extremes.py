#!/usr/bin/env python3
"""Actually attempt both queried-row cases with formal commit/open lifecycles.
No active available-memory guard or estimate exclusion. Do not modify baseline data.
"""
import argparse
import json
import pathlib
import shutil
import subprocess

from attempt_retained_extremes import monitor_command, prepare_attempt
from run_sweep import rows, sha, utc, write_table

MEASUREMENTS = ['commit_time_ms', 'open_time_ms', 'verify_time_ms', 'commitment_bytes',
                'eval_proof_bytes', 'proof_size_bytes', 'proof_size_kb', 'diagonal_transition_ms', 'fingerprint']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=pathlib.Path)
    parser.add_argument('--root', required=True, type=pathlib.Path)
    parser.add_argument('--mu', type=int, default=30)
    args = parser.parse_args()
    root = args.root.resolve()
    root.mkdir(parents=True, exist_ok=True)
    if any(root.iterdir()):
        raise RuntimeError('new empty evidence directory required')
    shutil.copy2(args.binary.resolve(), root / 'bench')
    repo = pathlib.Path(__file__).resolve().parents[1]
    common = [str(root / 'bench'), '--mu', str(args.mu), '--fields', 'm127', '--r', '2', '--c', '4',
              '--threads', '32', '--memory-mode', 'retained', '--first-tree-strategy', 'recovered',
              '--repetitions', '3', '--max-estimated-gib', '0.000001']
    # The intentionally tiny legacy ceiling must not exclude this unmodelled strategy.
    listing = subprocess.check_output(common + ['--list', '--backends', 'qa,rs'], text=True)
    (root / 'selected-cases.csv').write_text(listing)
    selected = rows(root / 'selected-cases.csv')
    assert len(selected) == 2
    assert {r['backend'].lower() for r in selected} == {'qa', 'rs'}
    assert all(r['first_tree_strategy'] == 'prehashed_recovered' and r['diagonal_storage'] == 'retained_both'
               and r['estimated_gib'] == '' and r['low_memory'] == 'false' for r in selected)
    manifest = dict(started_utc=utc(), binary_sha256=sha(root / 'bench'), mu=args.mu,
                    source_commit=subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip(),
                    source_status=subprocess.check_output(['git', '-C', str(repo), 'status', '--short'], text=True).strip(),
                    repetitions=3, warmup=1, threads=32, setup_per_process=1,
                    fresh_commit_every_repetition=True, first_tree_strategy='prehashed_recovered',
                    diagonal_storage='retained_both', memory_guard_enabled=False,
                    whole_tree_rebuild=False, estimate_exclusions=False,
                    directive='ac83bfb5-eeb2-4070-9356-e45ec2884dac',
                    setup_seed_byte=42, message_seed_byte=27, point_seed_byte=28,
                    scope='separate actual attempts; original 142 configurations and all prior failures immutable')
    (root / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    policy = {k: manifest[k] for k in ('first_tree_strategy', 'diagonal_storage', 'memory_guard_enabled', 'directive')}
    policy['enabled'] = True
    policy_path = root / 'policy.json'
    policy_path.write_text(json.dumps(policy, indent=2) + '\n')

    def enabled():
        return json.loads(policy_path.read_text()) == policy

    outcomes, completed_raw = [], []
    for selection in selected:
        code = selection['backend'].lower()
        directory = root / 'cases' / code
        directory.mkdir(parents=True)
        output = directory / 'results.csv'
        command = ['/usr/bin/time', '-v', '-o', str(directory / 'resources.txt')] + common + [
            '--backends', code, '--output', str(output)]
        contract = dict(binary_sha256=manifest['binary_sha256'], command=command,
                        first_tree_strategy='prehashed_recovered', diagonal_storage='retained_both',
                        memory_guard_enabled=False, repetitions=3, policy_directive=manifest['directive'])
        assert prepare_attempt(directory, command, contract) is None
        (root / 'progress.json').write_text(json.dumps(dict(completed=len(outcomes), total=2,
                                                           current=code, updated_utc=utc())) + '\n')
        if not enabled():
            raise RuntimeError('attempt policy revoked')
        status = monitor_command(command, directory, None, enabled)
        status['execution_contract'] = contract
        (directory / 'status.json').write_text(json.dumps(status, indent=2) + '\n')
        if status['stop_reason'] == 'policy_revoked':
            raise RuntimeError('attempt canceled; evidence preserved')
        sample_path = output.with_suffix('.raw.csv')
        samples = rows(sample_path) if sample_path.exists() else []
        if status['exit_code'] == 0:
            result = rows(output)
            assert len(result) == 1 and result[0]['status'] == 'measured' and len(samples) == 3
            row = result[0]
            assert [s['repetition'] for s in samples] == ['1', '2', '3']
            for record in [row] + samples:
                assert record['first_tree_strategy'] == 'prehashed_recovered'
                assert record['diagonal_storage'] == 'retained_both' and record['low_memory'] == 'false'
                assert record['fingerprint'] == row['fingerprint'] and record['estimated_gib'] == ''
            completed_raw.extend(samples)
        else:
            row = {**selection, 'status': 'runtime_failed', 'threads': '32', 'repetitions': '3'}
            for field in MEASUREMENTS:
                row[field] = ''
        row.update(exit_code=str(status['exit_code']), memory_guard_enabled=False,
                   recorded_raw_samples=len(samples), observed_peak_hwm_kib=status['observed_peak_hwm_kib'],
                   minimum_host_available_bytes=status['minimum_host_available_bytes'],
                   global_oom_kill_delta=status['system_after']['vmstat']['oom_kill'] - status['system_before']['vmstat']['oom_kill'])
        outcomes.append(row)
        # Write heterogeneous measured/failure rows with a union schema, never drop evidence fields.
        fields = list(dict.fromkeys(k for r in outcomes for k in r))
        write_table(root / 'outcomes.csv', [{k: r.get(k, '') for k in fields} for r in outcomes])
        write_table(root / 'completed-raw.csv', completed_raw)
    summary = dict(finished_utc=utc(), processed=2, measured=sum(r['status'] == 'measured' for r in outcomes),
                   runtime_failed=sum(r['status'] == 'runtime_failed' for r in outcomes),
                   complete_raw_samples=len(completed_raw), first_tree_strategy='prehashed_recovered',
                   diagonal_storage='retained_both', memory_guard_enabled=False, whole_tree_rebuild=False)
    (root / 'finished.json').write_text(json.dumps(summary, indent=2) + '\n')
    (root / 'progress.json').write_text(json.dumps(dict(phase='finished', **summary)) + '\n')


if __name__ == '__main__':
    main()
