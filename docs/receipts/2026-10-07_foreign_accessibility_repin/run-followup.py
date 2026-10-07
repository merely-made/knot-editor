from pathlib import Path
import json,subprocess,os,datetime,hashlib,zipfile,psutil
r=Path(__file__).resolve().parents[3]; out=Path(__file__).parent
env=os.environ.copy();env['CARGO_TARGET_DIR']='C:/t/cargo-targets/knot-editor';env['CARGO_BUILD_JOBS']='1'
def gate(label,args):
 start=datetime.datetime.now(datetime.timezone.utc).isoformat();cmd=['cargo','+1.98.1']+args
 with (out/(label+'.stdout.log')).open('xb') as stdout,(out/(label+'.stderr.log')).open('xb') as stderr:
  p=subprocess.Popen(cmd,cwd=r,env=env,stdout=stdout,stderr=stderr,creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS); print(label,p.pid,flush=True); code=p.wait()
 result={'command':cmd,'cwd':str(r),'target':env['CARGO_TARGET_DIR'],'priority':'BelowNormal','jobs':1,'pid':p.pid,'started_utc':start,'finished_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'exit_code':code}
 with (out/(label+'-result.json')).open('x',encoding='utf-8') as f:json.dump(result,f,indent=2);f.write('\n')
 print(label,'exit',code,flush=True);return code

import time
check=out/'workspace-check-result.json'
while not check.exists():time.sleep(2)
assert json.loads(check.read_text(encoding='utf-8'))['exit_code']==0,'workspace check failed'
if gate('standalone-metadata-resolved',['metadata','--manifest-path','crates/knot-document/Cargo.toml','--offline','--format-version','1'])!=0:raise SystemExit(1)
# Final raw archive freezes the resolved root/standalone locks and every tracked non-documentation input.
paths=[p for p in subprocess.check_output(['git','ls-files'],cwd=r,text=True).splitlines() if not p.startswith(('docs/receipts/','design_docs/')) and not p.endswith('.md')]+['crates/knot-document/Cargo.lock']
records=[]
with zipfile.ZipFile(out/'final-source-inputs.zip','x',zipfile.ZIP_DEFLATED) as z:
 for p in sorted(set(paths)):
  f=r/p
  if f.is_file():
   b=f.read_bytes();z.writestr(p,b);records.append({'path':p,'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()})
with (out/'final-source-inputs.json').open('x',encoding='utf-8') as f:json.dump({'base':'2967972ce2953fa15c5e44ac0977c3114e1df67d','raw_inputs':records,'archive_sha256':hashlib.sha256((out/'final-source-inputs.zip').read_bytes()).hexdigest()},f,indent=2);f.write('\n')
for label,args in [
 ('workspace-tests',['test','--workspace','--all-targets','--locked','--no-fail-fast','-j1']),
 ('standalone-default-tests',['test','--manifest-path','crates/knot-document/Cargo.toml','--locked','-j1']),
 ('standalone-engine-tests',['test','--manifest-path','crates/knot-document/Cargo.toml','--features','engine','--locked','-j1'])]:
 if gate(label,args)!=0:raise SystemExit(1)
for x in records:assert hashlib.sha256((r/x['path']).read_bytes()).hexdigest()==x['sha256'],x['path']
print('final input hashes unchanged',len(records),flush=True)
