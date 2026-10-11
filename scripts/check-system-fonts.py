#!/usr/bin/env python3
"""Run explicit own-process system-font resource, renderer and standalone evidence."""
import argparse,json,os,shlex,subprocess,sys
from pathlib import Path

def main():
 p=argparse.ArgumentParser(description=__doc__)
 p.add_argument('--root',type=Path,default=Path(__file__).resolve().parents[1])
 p.add_argument('--output',type=Path,default=Path('target/system-fonts'))
 p.add_argument('--target-dir',type=Path)
 p.add_argument('--consumer-manifest',type=Path)
 p.add_argument('--native-tests',type=Path)
 p.add_argument('--html',type=Path)
 p.add_argument('--cargo-driver',default='cargo')
 p.add_argument('--runner',default='')
 args=p.parse_args();root=args.root.resolve();out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
 cache=(args.target_dir or out/'cargo').resolve();tests=(args.native_tests or root/'crates/ravet-webview/native/tests').resolve();html=(args.html or root/'examples/tutorials/src/system_fonts/index.html').resolve()
 environment=dict(os.environ,CARGO_BUILD_JOBS='1',CARGO_TARGET_DIR=str(cache));driver=shlex.split(args.cargo_driver);runner=shlex.split(args.runner)
 def run(arguments,capture=False):
  result=subprocess.run(arguments,cwd=root,env=environment,check=False,capture_output=capture,text=True)
  if result.returncode:
   if capture:(out/'last-failed-command.log').write_text(result.stdout+result.stderr)
   result.check_returncode()
  return result
 if sys.platform!='darwin':raise RuntimeError('This native proof requires macOS; portable contracts are separate Cargo tests')
 package=['--manifest-path',str(args.consumer_manifest.resolve())] if args.consumer_manifest else ['-p','ravet-examples']
 run([*driver,'build',*package,'--features','system-fonts,live-ui','--bin','system_fonts','--locked'])
 executable=cache/'debug/system_fonts'
 for source,name in [('system-fonts.mm','resources'),('system-font-glyphs.mm','native-glyphs')]:
  target=out/name;run(['clang++','-std=c++17','-fobjc-arc','-fobjc-arc-exceptions','-Wall','-Wextra','-Werror','-mmacosx-version-min=12.0',str(tests/source),'-framework','AppKit','-framework','CoreText','-o',str(target)])
  result=run([*runner,str(target)],capture=True)
  (out/('native-resource.log' if name=='resources' else 'native-glyph-oracle.tsv')).write_text(result.stdout+result.stderr)
  if name=='resources':print(result.stdout,end='',flush=True)
 result=run([*runner,str(executable),'--glyph-oracle'],capture=True);(out/'rust-glyph-oracle.log').write_text(result.stdout);(out/'rust-glyph-diagnostics.log').write_text(result.stderr)
 result=run([sys.executable,str(tests/'compare-system-font-glyphs.py'),str(out)],capture=True);(out/'glyph-comparison.json').write_text(result.stdout)
 summary=json.loads(result.stdout);print(summary['glyphs'],'exact glyph/advance profiles; maximum tight bounds difference',summary['maximumBoundError'],'fontunits',flush=True)
 result=run([*runner,str(executable),'--fixture'],capture=True);(out/'native-consumer.log').write_text(result.stdout+result.stderr);print(result.stdout,end='',flush=True)
 result=run([*runner,str(executable),'--export-first-party'],capture=True)
 states=json.loads(result.stdout)
 if len(states)!=36 or len(result.stdout.encode())>2*1024*1024:raise RuntimeError('first-party prepared-state bound')
 (out/'first-party-states.json').write_text(result.stdout)
 fixture=html.read_text();host='''<script>window.fixtureStates=STATES;let revision=0;window.ravetSend=msg=>{let key=Object.keys(msg).find(k=>k!=='revision'),current=window.fixtureApplied;if(msg.revision!==revision||!current)return;let target={...current};if(key==='font')target.selected=msg.font;else if(key==='mode')target.mode=msg.mode===1?'raster':'svg';else if(['sample','size'].includes(key))target[key]=msg[key];else return;const next=fixtureStates.find(s=>['selected','sample','size','mode'].every(k=>s[k]===target[k]));if(!next)return;revision++;window.fixtureApplied={...next,revision};ravetUpdate(fixtureApplied);window.fixtureReceipts.push({key,value:msg[key],revision});};window.fixtureReceipts=[];window.fixtureApplied={...fixtureStates[0],revision};ravetUpdate(fixtureApplied);window.fixtureAll=()=>{const prior=fixtureApplied;let groups=0;for(const s of fixtureStates){revision++;fixtureApplied={...s,revision};ravetUpdate(fixtureApplied);const r=ravetSystemFontReport();if(!r||!r.exact||r.revision!==revision||r.payload!==s.payload||r.selected!==s.selected)throw Error('Rust image state '+groups);groups++;}revision++;fixtureApplied={...prior,revision};ravetUpdate(fixtureApplied);const before=JSON.stringify(ravetSystemFontReport());for(const invalid of [{...fixtureApplied,size:9},{...fixtureApplied,payload:'%%%invalid'},{...fixtureApplied,families:[{id:0,name:42}]}]){ravetUpdate(invalid);if(JSON.stringify(ravetSystemFontReport())!==before)throw Error('invalid changed presentation');groups++;}return {groups,source:'first-party Rust-rendered rectangle data',systemFontBytes:false,focus:document.hasFocus()};};</script>'''.replace('STATES',json.dumps(states,separators=(',',':')))
 path=out/'browser.html';path.write_text(fixture.replace('</body>',host+'</body>'))
 result=run([*runner,str(executable),'--browser-fixture',str(path)],capture=True);(out/'webkit-first-party.log').write_text(result.stdout+result.stderr);print(result.stdout,end='',flush=True)
 print('First-party Chromium fixture for product-tool inspection:',path,flush=True)
if __name__=='__main__':main()
