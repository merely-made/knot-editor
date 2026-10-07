from pathlib import Path
import json,subprocess,os,datetime,hashlib,zipfile,psutil
r=Path(__file__).resolve().parents[3]; out=Path(__file__).parent
for p in psutil.process_iter(['pid','name','cmdline','cwd']):
 try:
  if (p.info['name'] or '').lower() in ['cargo.exe','rustc.exe'] and ((p.info['cwd'] or '').lower()==str(r).lower() or 'c:/t/cargo-targets/knot-editor' in ' '.join(p.info['cmdline'] or []).replace('\\','/').lower()): raise RuntimeError(('owned build exists',p.info))
 except (psutil.AccessDenied,psutil.NoSuchProcess): pass
env=os.environ.copy();env['CARGO_TARGET_DIR']='C:/t/cargo-targets/knot-editor';env['CARGO_BUILD_JOBS']='1'
def gate(label,args):
 start=datetime.datetime.now(datetime.timezone.utc).isoformat();cmd=['cargo','+1.98.1']+args
 with (out/(label+'.stdout.log')).open('xb') as stdout,(out/(label+'.stderr.log')).open('xb') as stderr:
  p=subprocess.Popen(cmd,cwd=r,env=env,stdout=stdout,stderr=stderr,creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS); print(label,p.pid,flush=True); code=p.wait()
 result={'command':cmd,'cwd':str(r),'target':env['CARGO_TARGET_DIR'],'priority':'BelowNormal','jobs':1,'pid':p.pid,'started_utc':start,'finished_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'exit_code':code}
 with (out/(label+'-result.json')).open('x',encoding='utf-8') as f:json.dump(result,f,indent=2);f.write('\n')
 print(label,'exit',code,flush=True);return code
if gate('metadata',['metadata','--locked','--format-version','1'])!=0:raise SystemExit(1)
raise SystemExit(gate('workspace-check',['check','--workspace','--all-targets','--locked','-j1']))
