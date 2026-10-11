#!/usr/bin/env python3
"""Run bounded paragraph contextual oracles and actual own-window consumers."""
import argparse,base64,hashlib,json,os,shlex,subprocess,sys
from pathlib import Path

def main():
 p=argparse.ArgumentParser(description=__doc__)
 p.add_argument('--root',type=Path,default=Path(__file__).resolve().parents[1])
 p.add_argument('--output',type=Path,default=Path('target/fallback-paragraphs'))
 p.add_argument('--target-dir',type=Path)
 p.add_argument('--consumer-manifest',type=Path)
 p.add_argument('--surface-manifest',type=Path)
 p.add_argument('--consumer-features',default='paragraph-workspace,live-ui')
 p.add_argument('--fixtures',type=Path)
 p.add_argument('--font-root',type=Path)
 p.add_argument('--html',type=Path)
 p.add_argument('--cargo-driver',default='cargo')
 p.add_argument('--runner',default='')
 args=p.parse_args();root=args.root.resolve();out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
 cache=(args.target_dir or out/'cargo').resolve();fixture=(args.fixtures or root/'crates/ravet-surface/tests/fixtures/fallback-paragraph').resolve();html=(args.html or root/'examples/tutorials/src/paragraph_workspace/index.html').resolve()
 environment=dict(os.environ,CARGO_BUILD_JOBS='1',CARGO_INCREMENTAL='0',CARGO_TARGET_DIR=str(cache),RAVET_PARAGRAPH_SNAPSHOT=str(out/'native-consumer.png'))
 driver=shlex.split(args.cargo_driver);runner=shlex.split(args.runner);commands=[]
 def run(name,arguments,capture=False):
  print('RUN',name,flush=True)
  result=subprocess.run([str(a)for a in arguments],cwd=root,env=environment,capture_output=True,text=True)
  (out/(name+'.log')).write_text(result.stdout+result.stderr);commands.append({'name':name,'argv':[str(a)for a in arguments],'exit_code':result.returncode})
  if result.returncode:print(result.stdout+result.stderr,flush=True);result.check_returncode()
  if not capture:print(result.stdout,end='',flush=True)
  return result.stdout
 consumer=['--manifest-path',args.consumer_manifest.resolve()]if args.consumer_manifest else ['-p','ravet-examples']
 surface=['--manifest-path',args.surface_manifest.resolve()]if args.surface_manifest else ['-p','ravet-surface']
 data=run('contextual-export',[*driver,'run',*surface,'--features','text','--example','paragraph-fallback-oracle','--locked'],True)
 # Cargo diagnostics are stderr; captured stdout remains exact prepared Rust JSON.
 (out/'paragraph-oracle.json').write_text(data)
 oracle=[sys.executable,fixture/'check-oracle.py','--root',root,'--output',out]
 if args.font_root:oracle.extend(['--font-root',args.font_root.resolve()])
 run('independent-oracle',oracle)
 if sys.platform!='darwin':
  (out/'terminal.json').write_text(json.dumps({'commands':commands,'native':'not executed: macOS required'},indent=2)+'\n');return
 run('consumer-build',[*driver,'build',*consumer,'--features',args.consumer_features,'--bin','paragraph_workspace','--locked'])
 executable=cache/'debug/paragraph_workspace'
 run('native-consumer',[*runner,executable,'--fixture'])
 data=run('authored-export',[*runner,executable,'--export'],True);states=json.loads(data)
 if len(states)!=274 or len(data.encode())>700000:raise RuntimeError('authored prepared-state bound')
 (out/'consumer-states.json').write_text(data)
 base=html.read_text();harness="window.ravetParagraphFixtureHTML=new TextDecoder().decode(Uint8Array.from(atob("+json.dumps(base64.b64encode(base.encode()).decode())+"),c=>c.charCodeAt(0)));\n"+(fixture/'browser-harness.js').read_text()
 (out/'browser-harness.js').write_text(harness)
 (out/'browser.html').write_text(base.replace('</body>','<script src="browser-harness.js"></script></body>'))
 embedded=harness.replace("const states=await(await fetch('consumer-states.json')).json(),",'const states=window.ravetParagraphFixtureStates,')
 own=base.replace('</body>','<script>window.ravetParagraphFixtureStates='+json.dumps(states,separators=(',',':'))+';</script><script>'+embedded+'</script></body>')
 if len(own.encode())>1040000:raise RuntimeError('own-window fixture HTML bound')
 (out/'browser-webkit.html').write_text(own)
 run('webkit-authored',[*runner,executable,'--browser-fixture',out/'browser-webkit.html'])
 (out/'terminal.json').write_text(json.dumps({'commands':commands,'native_groups':41,'webkit_authored_groups':65,'source_sha256':{str(p):hashlib.sha256(p.read_bytes()).hexdigest()for p in [html,fixture/'browser-harness.js',fixture/'check-oracle.py']},'states_sha256':hashlib.sha256((out/'consumer-states.json').read_bytes()).hexdigest(),'scope':'actual own-window dynamic Rust system-font paragraph consumer separately from authored Rust prepared-state HTML transport; browser trusted keys require product tools; no AX/deadline/general Unicode certification'},indent=2)+'\n')
 print('Chromium fixture for product-tool inspection:',out/'browser.html',flush=True)
if __name__=='__main__':main()
