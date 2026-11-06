#!/usr/bin/env python3
"""Validate arithmetic windows and aggregate independent process rounds."""
import argparse
import csv
import hashlib
import json
import math
import pathlib
import shutil
import statistics as stats
import tarfile


def rows(path):
    with path.open() as stream:
        return list(csv.DictReader(stream))


def table(path, entries):
    with path.open('w', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=list(entries[0]))
        writer.writeheader()
        writer.writerows(entries)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    root, out = args.root, args.output
    manifest = json.loads((root/'manifest.json').read_text())
    schema=manifest.get('timing_schema',1)
    finished = json.loads((root/'finished.json').read_text())
    assert finished['processes'] == len(manifest['jobs'])
    assert finished['exit_codes'] == [0]*len(manifest['jobs'])
    if (root/'profile').exists():
        assert hashlib.sha256((root/'profile').read_bytes()).hexdigest() == manifest['binary_sha256']
    fingerprints, records, processes = {}, [], []
    keys = ('extension_degree', 'kernel', 'elements')
    for iteration, variant in manifest['jobs']:
        case = root/f'round{iteration}-{variant}'
        status = json.loads((case/'status.json').read_text())
        command = json.loads((case/'command.json').read_text())
        assert status['exit_code'] == 0 and command[command.index('--backend')+1] == variant
        assert command[command.index('--threads')+1] == '32'
        assert 'checks passed' in (case/'stdout.log').read_text()
        current = rows(case/'windows.csv')
        grouped = {}
        for r in current:
            assert r['backend'] == variant and r['threads'] == '32'
            assert int(r['bytes_per_element']) == 16*int(r['extension_degree'])
            assert r['elements'] in manifest['sizes'].split(',')
            elapsed = float(r['elapsed_seconds'])
            work = int(r['work_items'])
            assert math.isfinite(elapsed) and elapsed >= .1
            assert int(r['passes'])*int(r['elements'])//(32 if schema==2 else 1) == work > 0
            clocks = list(map(int,r['thread_elapsed_ns'].split(';')))
            counts = list(map(int,r['thread_work_items'].split(';')))
            assert len(clocks) == len(counts) == 32 and sum(counts) == work
            if schema==2:
                passes=list(map(int,r['thread_passes'].split(';')))
                walls=list(map(int,r['thread_wall_ns'].split(';')))
                quanta=list(map(int,r['thread_quantum'].split(';')))
                assert len(passes)==len(walls)==len(quanta)==32
                assert all(p>0 and q>0 and p%q==0 for p,q in zip(passes,quanta))
                assert sum(passes)==int(r['passes'])
                assert counts==[p*int(r['elements'])//32 for p in passes]
                assert all(w>=100000000 and 0<t<=w for t,w in zip(clocks,walls))
                assert all(w<=elapsed*1e9+5000 for w in walls)
            assert all(t >= 0 for t in clocks) and all(c >= 0 for c in counts)
            assert all(t <= elapsed*1e9+5000 for t in clocks)
            # elapsed is stored to nine decimal seconds; tolerate its <.5ns rounding.
            assert abs(float(r['wall_ns_per_item'])-elapsed*1e9/work) < .000002
            assert abs(float(r['mean_worker_ns_per_item'])-sum(clocks)/work) < .000002
            key = tuple(r[k] for k in keys)
            assert fingerprints.setdefault(key,(r['input_hash'],r['result_hash'])) == (r['input_hash'],r['result_hash']), (case.name,key)
            grouped.setdefault(key,[]).append(r)
            records.append(dict(process_round=iteration,case=case.name,**r))
        for key, group in sorted(grouped.items()):
            assert [int(r['window']) for r in group] == list(range(1,6))
            row = dict(process_round=iteration, backend=variant, **dict(zip(keys,key)),windows=5)
            for metric in ('wall_ns_per_item','mean_worker_ns_per_item'):
                values = [float(r[metric]) for r in group]
                row[metric+'_mean'] = f'{stats.mean(values):.9f}'
                row[metric+'_window_sd'] = f'{stats.stdev(values):.9f}'
            row['min_active_workers'] = min(sum(int(x)>0 for x in r['thread_work_items'].split(';')) for r in group)
            row['max_active_workers'] = max(sum(int(x)>0 for x in r['thread_work_items'].split(';')) for r in group)
            processes.append(row)
    assert len(records) == finished['recorded_windows']
    expected = None
    for _, variant in manifest['jobs']:
        found = {tuple(p[k] for k in keys) for p in processes if p['backend']==variant}
        if expected is None:
            expected = found
        assert found == expected
    aggregates = []
    for variant in ('ours','ark','fourq-native','fourq-canonical'):
        for key in sorted(expected):
            group = [r for r in processes if r['backend']==variant and tuple(r[k] for k in keys)==key]
            assert len(group) == sum(v==variant for _,v in manifest['jobs'])
            row = dict(backend=variant,**dict(zip(keys,key)),independent_processes=len(group))
            for metric in ('wall_ns_per_item','mean_worker_ns_per_item'):
                values = [float(r[metric+'_mean']) for r in group]
                row[metric+'_mean'] = f'{stats.mean(values):.9f}'
                row[metric+'_process_sd'] = f'{stats.stdev(values):.9f}' if len(values)>1 else ''
            aggregates.append(row)
    comparisons = []
    for key in sorted(expected):
        group = {r['backend']:r for r in aggregates if tuple(r[k] for k in keys)==key}
        for reference in ('ark','fourq-native','fourq-canonical'):
            for metric in ('wall_ns_per_item','mean_worker_ns_per_item'):
                ours = float(group['ours'][metric+'_mean'])
                ref = float(group[reference][metric+'_mean'])
                ratios = []
                for iteration,_ in manifest['jobs']:
                    if iteration in [x[0] for x in ratios]:
                        continue
                    matched = {r['backend']:r for r in processes if int(r['process_round'])==iteration and tuple(r[k] for k in keys)==key}
                    ratios.append((iteration,float(matched['ours'][metric+'_mean'])/float(matched[reference][metric+'_mean'])))
                values = [v for _,v in ratios]
                comparisons.append(dict(**dict(zip(keys,key)), reference=reference, metric=metric,
                                        ours_ns=f'{ours:.9f}',reference_ns=f'{ref:.9f}',
                                        ours_div_reference=f'{ours/ref:.9f}',
                                        matched_rounds=len(values),min_round_ratio=f'{min(values):.9f}',max_round_ratio=f'{max(values):.9f}',
                                        round_ratio_sd=f'{stats.stdev(values):.9f}' if len(values)>1 else ''))
    out.mkdir(parents=True,exist_ok=True)
    for name,data in [('raw-windows.csv',records),('process-means.csv',processes),('strategy-means.csv',aggregates),('comparisons.csv',comparisons)]:
        table(out/name,data)
    for name in ('manifest.json','finished.json','implementation-plan.md'):
        shutil.copy2(root/name,out/name)
    shutil.copy2(__file__,out/'analyze.py')
    with tarfile.open(out/'raw-evidence.tar.gz','w:gz') as archive:
        for p in sorted(root.rglob('*')):
            if p.is_file() and p!=root/'profile':
                archive.add(p,arcname=pathlib.Path('data')/p.relative_to(root))
    smoke = len(manifest['jobs'])<12 or bool(manifest['kernels'])
    report = '# M127 exact-field arithmetic comparison\n\n'
    if smoke:
        report += '**SMOKE ONLY: not the complete formal baseline.**\n\n'
    report += f"{finished['processes']} independent processes / {len(records)} windows validated. All timings use32threads. Arithmetic source hash: `{manifest.get('arithmetic_sha256','not recorded in this earlier smoke')}`.\n\n"
    if schema==2:
        report+='Timing schema2 uses32resident OS workers synchronized before each window. Workers repeat local kernel batches for at least100ms each; calibration/warmup finish before the measured start. Hashes/CSV/result collection are after all timed workers finish. Worker clocks are amortized across calibrated blocks targeting1ms. The wall clock includes start/end synchronization once per window. Full-width seeded fold challenges are converted before timing and included in the input fingerprint. The process is restricted to the recorded32distinct physical cores; this grants no exclusive use of those cores.\n\n'
    else:
        report+='Legacy short-task dispatch schema: this dataset is retained only as a functional harness smoke, not a formal performance baseline. Its small fold challenges and repeated dispatch were superseded before formal measurement.\n\n'
    report += ('Numbers below are wall nanoseconds per **input item**, inclusive of synchronization (or legacy dispatch for schema1). '
               'Scalar arithmetic has one operation per item; composite kernels have multiple operations. '
               'Per-worker kernel costs, raw thread counters, process/window standard deviations and matched-round ratios are separate CSV columns. '
               'Worker elapsed costs include scheduling effects and are not isolated single-thread latency. '
               'A ratio above1 means our cost is higher.\n\n')
    largest = str(max(map(int,manifest['sizes'].split(','))))
    report += f'Largest working set: {largest} total elements.\n\n'
    report += '| Degree | Kernel | Ours ns/item | Ark ns/item | FourQ native ns/item | FourQ canonical ns/item |\n|---|---|---:|---:|---:|---:|\n'
    for key in sorted(expected):
        if key[2]!=largest:
            continue
        group = {r['backend']:r for r in aggregates if tuple(r[k] for k in keys)==key}
        report += '| '+' | '.join([key[0],key[1]]+[f"{float(group[b]['wall_ns_per_item_mean']):.3f}" for b in ('ours','ark','fourq-native','fourq-canonical')])+' |\n'
    report += '''
The unchanged custom canonical field, generic ark-ff0.6.0 Montgomery field and pinned FourQ x64 C field all use p=2^127−1 and extension X²+1. FourQ is no longer maintained. No ASM/AVX FourQ macros are selected. ark's asm feature does not activate its excluded no-carry M127 multiplication branch. Native FourQ allows redundant zero; canonical FourQ normalizes each returned field operation. Cross-language calls carry whole worker batches, not individual field operations. Canonical exports and initialization are outside arithmetic windows; Encode/Decode/CanonicalRoundtrip measure those boundaries separately. Copy is a dispatch/memory control, not a guaranteed quantity to subtract.

Current custom arithmetic was not optimized for this baseline. Three process rounds and five windows per process, when present, are distinct levels of repetition; windows are not independent processes. No single aggregate score across unrelated kernels is meaningful. Large-workset costs may include memory bandwidth; small-workset wall costs may be dominated by scheduling. Both remain in the evidence rather than selecting only favorable sizes. These arithmetic kernels do not themselves measure a complete PCS, establish constant-time behavior or prove cryptographic security. Batch inverse is a common chunked prefix algorithm, with a separately tagged allocation-inclusive variant. Full lifecycle PCS checks are needed before attributing end-to-end speedup or rerunning the72certified M127 configurations.

The archive contains every command, exit/resource record, window, source copy and pinned version/hash; only the executable is omitted. Frozen source uses the recorded root commit and local Plonky3 pin. Build instructions and exact semantics are in data/source/README.md after extracting the archive. No host kernel/swap/cgroup/frequency changes were made. Affinity and shared-host limitations are recorded.

Archive-only numeric reproduction:

```sh
mkdir evidence
tar -xzf raw-evidence.tar.gz -C evidence
python3 analyze.py --root evidence/data --output regenerated
```

Compare the four numeric CSVs byte-for-byte; checksums cover the snapshot.
'''
    (out/'REPORT.md').write_text(report)
    (out/'SHA256SUMS').write_text(''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n' for p in sorted(out.iterdir()) if p.is_file() and p.name!='SHA256SUMS'))
    print(json.dumps(dict(processes=finished['processes'],windows=len(records),smoke=smoke)))


if __name__=='__main__':
    main()
