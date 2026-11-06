#!/usr/bin/env python3
"""Validate and reconstruct representative-point results from evidence only."""
import argparse,csv,hashlib,json,math,pathlib,re,shutil,statistics,tarfile,tempfile
from collections import defaultdict
METRICS=['setup_ms','input_ms','commit_ms','forward_release_ms','open_ms','witness_release_ms','first_tree_query_ms','verify_ms','serialization_ms','diagnostics_ms','cleanup_ms','wall_ms','online_total_ms']
SIGNATURE=['fingerprint','parameters_id','commitment_root','y0_hash','proof_hash','challenge_hash','commitment_bytes','eval_proof_bytes','proof_size_bytes']
def rows(p):
    with p.open(newline='') as f:return list(csv.DictReader(f))
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,data):
    assert data
    with p.open('w',newline='') as f:
        w=csv.DictWriter(f,fieldnames=list(data[0]),lineterminator='\n');w.writeheader();w.writerows(data)
def sd(xs):return statistics.stdev(xs) if len(xs)>1 else 0.
def analyze(root,out):
    manifest=json.loads((root/'manifest.json').read_text());finish=json.loads((root/'finished.json').read_text());out.mkdir(parents=True,exist_ok=True)
    assert not any(out.iterdir()),'new output directory required'
    assert finish['processes']==len(manifest['jobs']) and finish['all_exit_zero']
    assert manifest['threads']==32 and manifest['setup_per_trial'] and not manifest['memory_guard_enabled']
    assert manifest['first_tree_strategy']=='prehashed_recovered'
    for f,digest in manifest['package_files'].items():assert sha(root/'source'/f)==digest
    assert sha(root/'frozen-field_m127.rs')==manifest['production_field_sha256'];assert sha(root/'root-Cargo.lock')==manifest['root_lock_sha256']
    certificates={(s['mu'],'qa' if s['backend'].lower().startswith('qa') else s['backend'].lower()):s for s in rows(root/'m127-parameters.csv') if s['r']=='2' and s['c']=='4'}
    raw=[];process=[];signatures={};seen=set()
    for job in manifest['jobs']:
        key=f"mu{job['mu']}-{job['backend']}-round{job['trial_round']}-{job['field_backend']}";assert key not in seen;seen.add(key);d=root/'cases'/key
        contract=json.loads((d/'execution-contract.json').read_text());status=json.loads((d/'status.json').read_text());assert status['exit_code']==0
        for k in job:assert contract[k]==job[k]
        assert contract['binary_sha256']==manifest['binary_sha256'] and not contract['memory_guard_enabled'] and contract['setup_per_trial']
        cmd=contract['command'];assert cmd[0]=='/usr/bin/time'
        for flag,value in [('--mu',str(job['mu'])),('--backend',job['backend']),('--field-backend',job['field_backend']),('--trial-round',str(job['trial_round'])),('--threads','32'),('--repetitions',str(manifest['repetitions']))]:assert cmd[cmd.index(flag)+1]==value
        samples=rows(d/'samples.csv');assert len(samples)==manifest['repetitions'];assert [int(s['repetition']) for s in samples]==list(range(1,len(samples)+1))
        cert=certificates[(str(job['mu']),job['backend'])]
        for s in samples:
            assert s['mu']==str(job['mu']) and s['backend'].lower()==job['backend'] and s['field_backend']==job['field_backend'].replace('-','_') and int(s['trial_round'])==job['trial_round']
            assert (s['r'],s['c'],s['threads'],s['extension_degree'])==('2','4','32','2')
            assert s['mode']=='Release' and s['setup_per_trial']=='true' and s['parameter_lifecycle']=='opening_only' and s['witness_lifecycle']=='consumed_after_first_round' and s['first_tree_strategy']=='prehashed_recovered'
            assert s['forward_capacity_elements']=='0' and int(s['inverse_capacity_elements'])>0 and int(s['released_witness_elements'])==1<<job['mu']
            assert int(s['released_witness_capacity'])>=int(s['released_witness_elements']) and s['witness_release_level']==s['depth'] and s['witness_release_subround']=='0'
            assert int(s['commitment_bytes'])+int(s['eval_proof_bytes'])==int(s['proof_size_bytes'])
            for k in ['mu0','depth','extension_degree']:assert s[k]==cert[k]
            assert s['queries']==cert['queries']
            for m in METRICS:assert math.isfinite(float(s[m])) and float(s[m])>=0
            assert abs(sum(float(s[m]) for m in ['commit_ms','forward_release_ms','open_ms','verify_ms'])-float(s['online_total_ms']))<.00001
            phase=sum(float(s[m]) for m in ['setup_ms','input_ms','commit_ms','forward_release_ms','open_ms','verify_ms','serialization_ms','diagnostics_ms','cleanup_ms']);assert float(s['wall_ms'])>=phase-.00001
            sig=tuple(s[k] for k in SIGNATURE);assert signatures.setdefault((s['mu'],s['backend']),sig)==sig
            raw.append(dict(case=key,**s))
        resource=(d/'resources.txt').read_text();peak=int(re.search(r'Maximum resident set size \(kbytes\): (\d+)',resource)[1])/2**20
        assert 'Command terminated by signal' not in resource
        pr=dict(case=key,mu=job['mu'],backend=job['backend'],field_backend=job['field_backend'].replace('-','_'),trial_round=job['trial_round'],samples=len(samples),peak_rss_gib=f'{peak:.9f}',proof_size_bytes=samples[0]['proof_size_bytes'],fingerprint=samples[0]['fingerprint'])
        for m in METRICS:
            vals=[float(s[m]) for s in samples];pr[m]=f'{statistics.mean(vals):.9f}';pr[m+'_within_process_sample_sd']=f'{sd(vals):.9f}'
        process.append(pr)
    assert len(raw)==finish['formal_samples']==len(manifest['jobs'])*manifest['repetitions']
    groups=defaultdict(list)
    for p in process:groups[(p['mu'],p['backend'],p['field_backend'])].append(p)
    strategies=[]
    for (mu,code,engine),ps in sorted(groups.items()):
        summary=dict(mu=mu,backend=code,field_backend=engine,independent_processes=len(ps),formal_samples=sum(p['samples'] for p in ps),proof_size_bytes=ps[0]['proof_size_bytes'],fingerprint=ps[0]['fingerprint'])
        for m in METRICS+['peak_rss_gib']:
            vals=[float(p[m]) for p in ps]
            for key,value in [('mean',statistics.mean(vals)),('sample_sd',sd(vals)),('min',min(vals)),('max',max(vals))]:summary[f'{m}_{key}']=f'{value:.9f}'
        strategies.append(summary)
    pairs=[]
    for (mu,code,engine),ps in sorted(groups.items()):
        if engine=='ours_frozen':continue
        controls={p['trial_round']:p for p in groups[(mu,code,'ours_frozen')]};assert {p['trial_round'] for p in ps}==set(controls)
        for m in METRICS+['peak_rss_gib']:
            deltas=[float(p[m])-float(controls[p['trial_round']][m]) for p in ps];ratios=[float(p[m])/float(controls[p['trial_round']][m]) for p in ps if float(controls[p['trial_round']][m])>0]
            base=statistics.mean(float(p[m]) for p in controls.values());candidate=statistics.mean(float(p[m]) for p in ps)
            pairs.append(dict(mu=mu,backend=code,field_backend=engine,metric=m,independent_rounds=len(ps),ours_mean=f'{base:.9f}',candidate_mean=f'{candidate:.9f}',ratio_of_means=f'{candidate/base:.9f}' if base else '',percent_of_means=f'{100*(candidate/base-1):.9f}' if base else '',paired_delta_mean=f'{statistics.mean(deltas):.9f}',paired_delta_sample_sd=f'{sd(deltas):.9f}',paired_delta_min=f'{min(deltas):.9f}',paired_delta_max=f'{max(deltas):.9f}',paired_ratio_mean=f'{statistics.mean(ratios):.9f}' if ratios else '',paired_ratio_sample_sd=f'{sd(ratios):.9f}' if ratios else ''))
    for name,data in [('raw-samples',raw),('process-means',process),('strategy-means',strategies),('paired-differences',pairs)]:write(out/f'{name}.csv',data)
    lines=['# Native M127 PCS representative-point comparison','',f"Experiment role: {manifest['experiment_role']}.",'',f"Validated {len(process)} independent processes and {len(raw)} formal trials. One warmup + {manifest['repetitions']} measured trials per process; fresh Setup per trial,32 threads, same physical-core affinity. All signatures and certified main-profile parameters agree across backends.",'','| mu | code | field backend | commit (ms) | open (ms) | verify (ms) | online total (ms) | full wall (ms) | process peak RSS mean (GiB) | process peak RSS max (GiB) | proof bytes |','|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|']
    for s in strategies:lines.append('| '+' | '.join([str(s['mu']),s['backend'],s['field_backend']]+[f"{float(s[m+'_mean']):.4f}" for m in ['commit_ms','open_ms','verify_ms','online_total_ms','wall_ms','peak_rss_gib']]+[f"{float(s['peak_rss_gib_max']):.4f}",s['proof_size_bytes']])+' |')
    lines+=['','Across-process sample SD and paired-round differences/ranges are in the CSV files. Within-process repeats are not additional independent process rounds. These are descriptive shared-host measurements, not confidence intervals or guarantees.','', 'All three paths use opening-only parameters, first-subround witness release and first-tree queried-row recovery. Online total includes forward-table release and open includes witness release/query work. Full wall also includes fresh Setup, input/evaluation, serialization, diagnostics and final cleanup. Peak RSS covers the complete process and warmup. Historical cached-Setup and failed retained experiments are separate and unchanged.','', 'Ark stays Montgomery internally and canonical conversion is included at actual protocol boundaries. FourQ uses audited redundant [0,p] x64 UINT128 C arithmetic with native base/E operations and thin scalar FFI, whose overhead is included. No cross-language LTO or bulk fusion; results characterize this integration, not every possible implementation. No backend has been selected for production by this report. Correctness tests and timing are not security proofs.','',f"Source commit: {manifest['source_commit']}; binary SHA256: {manifest['binary_sha256']}.",'']
    (out/'REPORT.md').write_text('\n'.join(lines))
    (out/'manifest.json').write_bytes((root/'manifest.json').read_bytes())
    return dict(processes=len(process),formal_samples=len(raw),numerical_tables=4)
def main():
    p=argparse.ArgumentParser(description=__doc__);g=p.add_mutually_exclusive_group(required=True);g.add_argument('--root',type=pathlib.Path);g.add_argument('--archive',type=pathlib.Path);p.add_argument('--output',required=True,type=pathlib.Path);a=p.parse_args()
    if a.archive:
        with tempfile.TemporaryDirectory() as tmp:
            with tarfile.open(a.archive) as t:t.extractall(tmp,filter='data')
            result=analyze(pathlib.Path(tmp)/'data',a.output)
    else:
        result=analyze(a.root,a.output)
        with tarfile.open(a.output/'raw-evidence.tar.gz','w:gz') as t:
            for f in sorted(a.root.rglob('*')):
                if f.is_file() and f.name!='profile':t.add(f,arcname='data/'+str(f.relative_to(a.root)),recursive=False)
    result['analysis_sha256']=sha(pathlib.Path(__file__))
    (a.output/'validation.json').write_text(json.dumps(result,indent=2)+'\n')
    if a.root:
        for f in ['analyze.py','README.md']:shutil.copy2(pathlib.Path(__file__).parent/f,a.output/f)
        files=sorted(f for f in a.output.iterdir() if f.is_file());(a.output/'SHA256SUMS').write_text(''.join(f'{sha(f)}  {f.name}\n' for f in files))
    print(json.dumps(result))
if __name__=='__main__':main()
