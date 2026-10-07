from pathlib import Path
import json,subprocess,os,datetime,hashlib,zipfile,psutil
r=Path(__file__).resolve().parents[3]; out=Path(__file__).parent
env=os.environ.copy();env['CARGO_TARGET_DIR']='C:/t/cargo-targets/knot-editor';env['CARGO_BUILD_JOBS']='1'
def gate(label,args):
 start=datetime.datetime.now(datetime.timezone.utc).isoformat();cmd=args
 with (out/(label+'.stdout.log')).open('xb') as stdout,(out/(label+'.stderr.log')).open('xb') as stderr:
  p=subprocess.Popen(cmd,cwd=r,env=env,stdout=stdout,stderr=stderr,creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS); print(label,p.pid,flush=True); code=p.wait()
 result={'command':cmd,'cwd':str(r),'target':env['CARGO_TARGET_DIR'],'priority':'BelowNormal','jobs':1,'pid':p.pid,'started_utc':start,'finished_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'exit_code':code}
 with (out/(label+'-result.json')).open('x',encoding='utf-8') as f:json.dump(result,f,indent=2);f.write('\n')
 print(label,'exit',code,flush=True);return code

import time
while not (out/'workspace-tests-result.json').exists():time.sleep(2)
assert json.loads((out/'workspace-tests-result.json').read_text(encoding='utf-8'))['exit_code']==101
m=json.loads((out/'final-source-inputs.json').read_text(encoding='utf-8'));assert all(hashlib.sha256((r/x['path']).read_bytes()).hexdigest()==x['sha256'] for x in m['raw_inputs'])
with (out/'original-source-after-guard.json').open('x',encoding='utf-8') as f:json.dump({'inputs':len(m['raw_inputs']),'all_raw_hashes_match':True,'source_manifest_sha256':hashlib.sha256((out/'final-source-inputs.json').read_bytes()).hexdigest()},f,indent=2);f.write('\n')
exe=Path('C:/t/cargo-targets/knot-editor/debug/deps/knot_desktop-84d0239d5c44f1d6.exe')
with (out/'same-binary-control-identity.json').open('x',encoding='utf-8') as f:json.dump({'executable':str(exe),'sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'source_manifest':'final-source-inputs.json'},f,indent=2);f.write('\n')
code=gate('same-binary-recovery-control',[str(exe),'--exact','recovery_runtime::tests::failed_write_is_reported_and_later_edit_can_retry','--nocapture'])
for label,args in [
 ('standalone-default-tests',['test','--manifest-path','crates/knot-document/Cargo.toml','--locked','-j1']),
 ('standalone-engine-tests',['test','--manifest-path','crates/knot-document/Cargo.toml','--features','engine','--locked','-j1']),
 ('standalone-engine-metadata',['metadata','--manifest-path','crates/knot-document/Cargo.toml','--features','engine','--locked','--offline','--format-version','1'])]:
 if gate(label,['cargo','+1.98.1']+args)!=0:raise SystemExit(1)
assert all(hashlib.sha256((r/x['path']).read_bytes()).hexdigest()==x['sha256'] for x in m['raw_inputs'])
print('independent gates complete; all frozen inputs unchanged',flush=True)
