#!/usr/bin/env python3
"""Sequential native PCS comparison; immutable per-process commands and raw evidence."""
import argparse,csv,hashlib,json,os,pathlib,re,shutil,subprocess,time
from datetime import datetime,timezone
BACKENDS=['ours-frozen','ark-adapter','fourq-native-adapter']
def utc():return datetime.now(timezone.utc).isoformat()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def readrows(p):
    with p.open(newline='') as f:return list(csv.DictReader(f))
def system():
    def values(path):return {l.split()[0].rstrip(':'):int(l.split()[1]) for l in pathlib.Path(path).read_text().splitlines() if len(l.split())>=2 and l.split()[1].isdigit()}
    return dict(meminfo_kib=values('/proc/meminfo'),vmstat=values('/proc/vmstat'))
def monitor(command,directory):
    before=system();peak=0;minfree=before['meminfo_kib']['MemAvailable'];started=utc()
    with (directory/'stdout.log').open('w') as out,(directory/'stderr.log').open('w') as err,(directory/'memory.jsonl').open('w') as mem:
        proc=subprocess.Popen(command,stdout=out,stderr=err,start_new_session=True)
        (directory/'pid').write_text(str(proc.pid)+'\n')
        while True:
            # Descendant processes only; thread RSS must not be summed repeatedly.
            todo=[proc.pid];rss=0
            while todo:
                pid=todo.pop()
                try:
                    status=pathlib.Path(f'/proc/{pid}/status').read_text();hit=re.search(r'^VmRSS:\s+(\d+)',status,re.M)
                    rss+=int(hit[1]) if hit else 0
                    todo.extend(map(int,pathlib.Path(f'/proc/{pid}/task/{pid}/children').read_text().split()))
                except (FileNotFoundError,ProcessLookupError):pass
            host=system();free=host['meminfo_kib']['MemAvailable'];peak=max(peak,rss);minfree=min(minfree,free)
            mem.write(json.dumps(dict(utc=utc(),process_tree_rss_kib=rss,host_available_kib=free,global_oom_kill=host['vmstat'].get('oom_kill',0)))+'\n');mem.flush()
            code=proc.poll()
            if code is not None:break
            time.sleep(.5)
    return dict(started_utc=started,finished_utc=utc(),exit_code=code,monitor_peak_rss_kib=peak,minimum_host_available_kib=minfree,system_before=before,system_after=system(),memory_guard_enabled=False)
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',required=True,type=pathlib.Path);p.add_argument('--binary',required=True,type=pathlib.Path);p.add_argument('--mus',default='20,26');p.add_argument('--smoke',action='store_true');p.add_argument('--rounds',type=int,default=3);p.add_argument('--repetitions',type=int,default=3);a=p.parse_args()
    assert a.rounds>0 and a.repetitions>0
    assert (a.smoke and a.mus=="20" and a.rounds==1 and a.repetitions==1) or (not a.smoke and a.mus=="20,26" and a.rounds==3 and a.repetitions==3), "formal contract is four points/three rounds/three repetitions; smoke is mu20/one round/one repetition"
    allowed=sorted(os.sched_getaffinity(0));physical={}
    for cpu in allowed:
        t=pathlib.Path(f'/sys/devices/system/cpu/cpu{cpu}/topology');key=((t/'physical_package_id').read_text().strip(),(t/'core_id').read_text().strip());physical.setdefault(key,cpu)
    cpus=list(physical.values())[:32];assert len(cpus)==32;os.sched_setaffinity(0,cpus)
    root=a.root.resolve();root.mkdir(parents=True,exist_ok=True);assert not any(root.iterdir()),'require empty evidence directory'
    here=pathlib.Path(__file__).resolve().parent;repo=here.parents[1];mus=list(map(int,a.mus.split(',')));assert set(mus)<= {20,26}
    shutil.copy2(a.binary,root/'profile')
    text=lambda cmd:subprocess.check_output(cmd,text=True).strip()
    fourq=pathlib.Path(os.environ.get('FOURQ_ROOT','/home/zjl/zkp-research/reference/FourQlib'))
    jobs=[dict(trial_round=r+1,mu=mu,backend=code,field_backend=v) for r in range(a.rounds) for mu in mus for code in ['qa','rs'] for v in BACKENDS[r%3:]+BACKENDS[:r%3]]
    files=['Cargo.toml','Cargo.lock','build.rs','fourq_scalar.c','README.md','run.py','analyze.py']+[str(f.relative_to(here)) for f in sorted((here/'src').glob('*.rs'))]
    manifest=dict(experiment_role='functional_smoke' if a.smoke else 'formal_representative_comparison',started_utc=utc(),directive='ae4b0be0-9144-41e0-b86f-2f73e226eec6',source_commit=text(['git','-C',str(repo),'rev-parse','HEAD']),source_status=text(['git','-C',str(repo),'status','--short']),binary_sha256=sha(root/'profile'),jobs=jobs,threads=32,cpu_affinity=cpus,initial_affinity=allowed,physical_core_policy='first32distinct allowed physical cores; no exclusive host reservation',repetitions=a.repetitions,warmup=1,setup_per_trial=True,first_tree_strategy='prehashed_recovered',parameter_lifecycle='opening_only',witness_lifecycle='consumed_after_first_round',memory_guard_enabled=False,whole_tree_rebuild=False,estimate_exclusions=False,field='M127',extension_degree=2,r=2,c=4,setup_seed_byte=42,message_seed_byte=27,point_seed_byte=28,rustc=text(['rustc','-Vv']),cc=text(['cc','--version']),build_flags='RUSTFLAGS=-C target-cpu=native; C -O3 -march=native -fwrapv -fno-strict-aliasing; no cross-language LTO; noASM/noAVX macros',root_lock_sha256=sha(repo/'Cargo.lock'),production_field_sha256=sha(repo/'src/field_m127.rs'),package_files={f:sha(here/f) for f in files},plonky3_commit=text(['git','-C','/home/zjl/BinaryField-Bench/vendor/Plonky3','rev-parse','HEAD']),plonky3_status=text(['git','-C','/home/zjl/BinaryField-Bench/vendor/Plonky3','status','--short']),fourq_commit=text(['git','-C',str(fourq),'rev-parse','HEAD']),fourq_status=text(['git','-C',str(fourq),'status','--short']),fourq_files={f:sha(fourq/'FourQ_64bit_and_portable'/f) for f in ['eccp2_core.c','AMD64/fp_x64.h','FourQ_internal.h','FourQ.h']},layouts=dict(ours_frozen=dict(base_bytes=16,extension_bytes=32,base_alignment=16,extension_alignment=16),ark_adapter=dict(base_bytes=16,extension_bytes=32,base_alignment=8,extension_alignment=8),fourq_native_adapter=dict(base_bytes=16,extension_bytes=32,base_alignment=16,extension_alignment=16)),wall_scope='fresh Setup/input including evaluation/commit/forward release/open/verify/serialization/diagnostics/cleanup; CSV IO excluded',online_scope='commit+forward_release+open+verify; release and query costs included',operation_mapping_file='README.md',plan='/home/zjl/zkp-research/notes/impl-plan-hyperfold-m127-pcs-backends.md')
    manifest['hardware']=text(['lscpu']);manifest['host_meminfo']=pathlib.Path('/proc/meminfo').read_text();manifest['fourq_range_audit_message']='4b4e2f56-00bf-4cc5-a1b7-2df5fa417787'
    assert manifest['plonky3_commit']=='0d803737b681774186cd283b1fd2d8e493655c82'
    assert manifest['fourq_commit']=='1031567f23278e1135b35cc04e5d74c2ac88c029'
    assert not manifest['source_status'],'freeze a clean source commit before running';assert not manifest['plonky3_status'] and not manifest['fourq_status']
    (root/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    source=root/'source';source.mkdir();
    for f in files:
        dest=source/f;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(here/f,dest)
    shutil.copy2(repo/'Cargo.lock',root/'root-Cargo.lock');shutil.copy2(repo/'src/field_m127.rs',root/'frozen-field_m127.rs');shutil.copy2(manifest['plan'],root/'implementation-plan.md')
    shutil.copy2('/home/zjl/zkp-research/experiments/hyperfold-v3-gold-m127-2026-11-05/results/m127-parameters.csv',root/'m127-parameters.csv')
    completed=[];signatures={}
    for job in jobs:
        key=f"mu{job['mu']}-{job['backend']}-round{job['trial_round']}-{job['field_backend']}";d=root/'cases'/key;d.mkdir(parents=True)
        command=['/usr/bin/time','-v','-o',str(d/'resources.txt'),str(root/'profile'),'--mu',str(job['mu']),'--backend',job['backend'],'--field-backend',job['field_backend'],'--threads','32','--trial-round',str(job['trial_round']),'--repetitions',str(a.repetitions),'--output',str(d/'samples.csv')]
        contract=dict(**job,command=command,binary_sha256=manifest['binary_sha256'],memory_guard_enabled=False,setup_per_trial=True,first_tree_strategy='prehashed_recovered',repetitions=a.repetitions)
        (d/'execution-contract.json').write_text(json.dumps(contract,indent=2)+'\n');(root/'progress.json').write_text(json.dumps(dict(completed=len(completed),total=len(jobs),current=key,updated_utc=utc()))+'\n')
        assert sha(root/'profile')==manifest['binary_sha256'];status=monitor(command,d);(d/'status.json').write_text(json.dumps(status,indent=2)+'\n')
        samples=readrows(d/'samples.csv') if (d/'samples.csv').exists() else []
        if status['exit_code']==0:
            assert len(samples)==a.repetitions
            sig={(s['fingerprint'],s['proof_size_bytes']) for s in samples};assert len(sig)==1;sig=sig.pop();assert signatures.setdefault((job['mu'],job['backend']),sig)==sig
            for s in samples:
                assert s['field_backend']==job['field_backend'].replace('-','_') and int(s['mu'])==job['mu'] and int(s['trial_round'])==job['trial_round']
        completed.append(dict(key=key,exit_code=status['exit_code'],recorded_samples=len(samples)))
        (root/'outcomes.json').write_text(json.dumps(completed,indent=2)+'\n')
        if status['exit_code']!=0:raise RuntimeError(f'{key} failed; evidence preserved, no fallback or partial means')
    result=dict(finished_utc=utc(),processes=len(jobs),formal_samples=sum(c['recorded_samples'] for c in completed),all_exit_zero=True)
    (root/'finished.json').write_text(json.dumps(result,indent=2)+'\n');(root/'progress.json').write_text(json.dumps(dict(phase='finished',**result))+'\n')
if __name__=='__main__':main()
