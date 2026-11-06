#!/usr/bin/env python3
"""Pinned exact-field baseline, sequential32-thread process rounds."""
import argparse
import csv
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
from datetime import datetime, timezone


def utc():
    return datetime.now(timezone.utc).isoformat()


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=pathlib.Path, required=True)
    parser.add_argument('--binary', type=pathlib.Path)
    parser.add_argument('--sizes', default='8192,32768,1048576')
    parser.add_argument('--kernels', default='')
    parser.add_argument('--rounds', type=int, default=3)
    args = parser.parse_args()
    here = pathlib.Path(__file__).resolve().parent
    repo = here.parents[1]
    root = args.root.resolve()
    root.mkdir(parents=True, exist_ok=True)
    assert not any(root.iterdir()), 'new empty evidence directory required'
    binary = args.binary or here / 'target/release/hyperfold-m127-compare'
    shutil.copy2(binary, root / 'profile')
    variants = ['ours', 'ark', 'fourq-native', 'fourq-canonical']
    jobs = [(r+1, v) for r in range(args.rounds) for v in
            (variants[r % 4:] + variants[:r % 4])]
    command_text = lambda cmd: subprocess.check_output(cmd, text=True).strip()
    fourq = pathlib.Path(os.environ.get('FOURQ_ROOT', '/home/zjl/zkp-research/reference/FourQlib'))
    source = command_text(['git', '-C', str(repo), 'rev-parse', 'HEAD'])
    manifest = dict(started_utc=utc(), source_commit=source,
                    source_status=command_text(['git', '-C', str(repo), 'status', '--short']),
                    binary_sha256=sha(root / 'profile'), sizes=args.sizes, kernels=args.kernels,
                    threads=32, windows=5, window_ms=100, jobs=jobs,
                    root_lock_sha256=sha(repo / 'Cargo.lock'), comparison_lock_sha256=sha(here / 'Cargo.lock'),
                    arithmetic_sha256=sha(repo/'src/field_m127.rs'),
                    plonky3_commit=command_text(['git','-C','/home/zjl/BinaryField-Bench/vendor/Plonky3','rev-parse','HEAD']),
                    plonky3_status=command_text(['git','-C','/home/zjl/BinaryField-Bench/vendor/Plonky3','status','--short']),
                    rustc=command_text(['rustc', '-Vv']), cc=command_text(['cc', '--version']),
                    build_flags='RUSTFLAGS=-C target-cpu=native;C:-O3 -march=native -fwrapv -fno-strict-aliasing;_AMD64_,__LINUX__;noASM/noAVX macros',
                    fourq_commit=command_text(['git', '-C', str(fourq), 'rev-parse', 'HEAD']),
                    fourq_files={str(p.relative_to(fourq)): sha(p) for p in
                                 [fourq/'FourQ_64bit_and_portable/eccp2_core.c',
                                  fourq/'FourQ_64bit_and_portable/AMD64/fp_x64.h',
                                  fourq/'FourQ_64bit_and_portable/FourQ_internal.h',
                                  fourq/'FourQ_64bit_and_portable/FourQ.h']},
                    hardware=command_text(['lscpu']), meminfo=pathlib.Path('/proc/meminfo').read_text(),
                    affinity=sorted(os.sched_getaffinity(0)), hostname=os.uname().nodename,
                    exclusive_host=False, host_settings_modified=False,
                    directive='c983b3c0-0511-4d4e-b7e3-180b247efe62',
                    plan='/home/zjl/zkp-research/notes/impl-plan-hyperfold-m127-performance.md')
    assert not manifest['source_status'], 'commit verified benchmark sources before timing'
    assert manifest['plonky3_commit']=='0d803737b681774186cd283b1fd2d8e493655c82' and not manifest['plonky3_status']
    assert manifest['fourq_commit'] == '1031567f23278e1135b35cc04e5d74c2ac88c029'
    assert not command_text(['git','-C',str(fourq),'status','--porcelain','--untracked-files=no'])
    (root/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    shutil.copytree(here, root/'source', ignore=shutil.ignore_patterns('target'))
    shutil.copy2(repo/'Cargo.lock', root/'root-Cargo.lock')
    shutil.copy2(repo/'src/field_m127.rs',root/'field_m127.rs')
    shutil.copy2(manifest['plan'], root/'implementation-plan.md')
    signatures = {}
    statuses = []
    for iteration, variant in jobs:
        case = root/f'round{iteration}-{variant}'
        case.mkdir()
        cmd = ['/usr/bin/time', '-v', '-o', str(case/'resources.txt'), str(root/'profile'),
               '--backend', variant, '--sizes', args.sizes, '--threads', '32',
               '--output', str(case/'windows.csv')]
        if args.kernels:
            cmd += ['--kernels', args.kernels]
        (case/'command.json').write_text(json.dumps(cmd, indent=2)+'\n')
        (root/'progress.json').write_text(json.dumps(dict(completed=len(statuses), total=len(jobs),
                                                        current=case.name, updated_utc=utc()))+'\n')
        started = utc()
        with (case/'stdout.log').open('w') as out, (case/'stderr.log').open('w') as err:
            result = subprocess.run(cmd, stdout=out, stderr=err)
        status = dict(case=case.name, started_utc=started, finished_utc=utc(), exit_code=result.returncode)
        (case/'status.json').write_text(json.dumps(status, indent=2)+'\n')
        assert result.returncode == 0, status
        with (case/'windows.csv').open() as stream:
            rows = list(csv.DictReader(stream))
        sizes = args.sizes.split(',')
        kernels = args.kernels.split(',') if args.kernels else sorted(set(r['kernel'] for r in rows))
        assert len(rows) == 2 * len(sizes) * len(kernels) * 5
        for r in rows:
            key = (r['extension_degree'], r['kernel'], r['elements'])
            fingerprint = (r['input_hash'], r['result_hash'])
            assert signatures.setdefault(key, fingerprint) == fingerprint, (case.name, key)
            assert r['threads'] == '32' and float(r['elapsed_seconds']) >= .1
            counts = list(map(int, r['thread_work_items'].split(';')))
            assert len(counts) == 32 and sum(counts) == int(r['work_items'])
        statuses.append(status)
    (root/'finished.json').write_text(json.dumps(dict(finished_utc=utc(), processes=len(statuses),
                                                    recorded_windows=sum(2*len(args.sizes.split(','))*(len(args.kernels.split(',')) if args.kernels else 23)*5 for _ in statuses),
                                                    exit_codes=[x['exit_code'] for x in statuses]), indent=2)+'\n')


if __name__ == '__main__':
    main()
