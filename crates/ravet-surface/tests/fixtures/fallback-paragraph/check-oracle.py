"""Independent native HarfBuzz glyph oracle plus fixed visual/wrap vectors."""
from pathlib import Path
import argparse,hashlib,json,math,shlex,subprocess
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parents[5])
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--font-root',type=Path)
args=parser.parse_args();root=args.root.resolve();out=args.output.resolve();fontroot=(args.font_root or root/'crates/ravet-surface').resolve();fixtures=Path(__file__).resolve().parent
cmd=['clang','-std=c11','-Wall','-Wextra','-Werror',str(fixtures/'harfbuzz-paragraph.c'),*shlex.split(subprocess.check_output(['pkg-config','--cflags','--libs','harfbuzz'],text=True)),'-o',str(out/'harfbuzz-paragraph')]
r=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);(out/'harfbuzz-paragraph-build.log').write_text(r.stdout);assert r.returncode==0,r.stdout
cases=json.loads((out/'paragraph-oracle.json').read_text());count=0;glyphs=0
for case in cases:
    encoded=case['text'].encode()
    for line in case['lines']:
        for run in line['runs']:
            path=root/run['font']
            if not path.exists(): path=fontroot/Path(run['font']).relative_to('crates/ravet-surface')
            context=encoded[run['contextStart']:run['contextEnd']].decode()
            start=run['start']-run['contextStart'];end=run['end']-run['contextStart']
            cmd=[str(out/'harfbuzz-paragraph'),str(path),context,str(start),str(end),run['direction'],run['script'],'',run['variation']]
            hb=json.loads(subprocess.check_output(cmd,text=True));assert hb['upem']==run['upem'];assert len(hb['glyphs'])==len(run['glyphs']),(case['name'],run,hb)
            scale=case['size']/hb['upem'];penx=peny=0
            for actual,expected in zip(run['glyphs'],hb['glyphs']):
                assert actual['id']==expected['id'] and actual['cluster']==expected['cluster']+run['contextStart'],(case['name'],actual,expected)
                values={'x':(penx+expected['xo'])*scale,'y':(peny+expected['yo'])*scale,'xa':expected['xa']*scale,'ya':expected['ya']*scale}
                for key,value in values.items(): assert math.isclose(actual[key],value,rel_tol=0,abs_tol=1e-9),(case['name'],key,actual[key],value)
                penx+=expected['xa'];peny+=expected['ya'];glyphs+=1
            count+=1
# These fixed expected ranges/advances are authored independently from primitive
# advance metrics and logical character bidi classes, not derived from Rust output.
byname={c['name']:c for c in cases}
fixed={
 'wrap':[(0,3,14.5),(3,6,14.5),(6,8,12)],
 'soft-isolate':[(0,7,12),(7,10,7),(10,16,12.5)],
 'hard-lines':[(0,2,12),(4,4,0),(5,6,5),(7,8,7),(11,12,5),(15,15,0)],
 'selected-heavy':[(0,2,85),(2,3,60)]}
for name,expected in fixed.items(): assert [(l['start'],l['end'],l['width']) for l in byname[name]['lines']]==expected,name
visual={
 'mixed':[(0,2),(2,4),(4,5),(5,6)],
 'mixed-rtl':[(5,6),(4,5),(2,4),(1,2),(0,1)],
 'arabic':[(4,6),(2,4),(0,2)]}
for name,expected in visual.items(): assert [(r['start'],r['end']) for r in byname[name]['lines'][0]['runs']]==expected,name
assert [(r['start'],r['end']) for r in byname['line-isolate']['lines'][1]['runs']]==[(10,13),(9,10),(13,17),(17,18)]
assert [(r['start'],r['end']) for r in byname['paragraph-isolate']['lines'][1]['runs']]==[(9,11),(11,13),(13,17),(17,18)]
# Independent, authored grapheme boundaries and L1/L2 visual-order vectors.
# These do not call Rust or another copy of its segmentation algorithm.
atomic={
 'arabic-zwj':([(0,5)], [[(0,5)]]),
 'rtl-zwj-wrap':([(0,1),(1,2),(2,6),(6,7),(7,8),(8,9),(9,10)], [[(6,7),(0,1),(1,2),(2,6)],[(7,8),(8,9),(9,10)]]),
 'rtl-zwj-discarded':([(0,1),(1,2),(2,3),(3,4),(4,8),(8,9),(9,10),(10,11),(11,12)], [[(1,2),(0,1)],[(8,9),(2,3),(3,4),(4,8)],[(9,10),(10,11),(11,12)]]),
 'flag-discarded':([(0,2),(2,3),(3,31),(31,32),(32,34)], [[(0,2),(2,3)],[(3,31),(31,32)],[(32,34)]]),
 'flag-full':([(0,2),(2,3),(3,31),(31,32),(32,34)], [[(32,34),(31,32),(3,31),(2,3),(0,2)]]),
 'flag-candidate':([(0,2),(2,3),(3,31),(31,32),(32,34)], [[(3,31),(2,3),(0,2),(31,32)],[(32,34)]]),
 'prepend-latin':([(0,2),(2,3),(3,8),(8,9),(9,11)], [[(2,3),(0,2),(3,8),(8,9),(9,11)]]),
 'prepend-latin-rtl':([(0,2),(2,3),(3,8),(8,9),(9,11)], [[(9,11),(8,9),(3,8),(2,3),(0,2)]]),
 'prepend-arabic':([(0,1),(1,2),(2,8),(8,9),(9,10)], [[(0,1),(1,2),(2,8),(8,9),(9,10)]]),
 'prepend-arabic-rtl':([(0,1),(1,2),(2,8),(8,9),(9,10)], [[(9,10),(8,9),(2,8),(1,2),(0,1)]])}
for name,(units,expected) in atomic.items():
 actual=[]
 for line in byname[name]['lines']:
  result=[]
  for run in line['runs']:
   selected=[unit for unit in units if unit[0]>=run['start'] and unit[1]<=run['end']]
   assert selected and selected[0][0]==run['start'] and selected[-1][1]==run['end'],(name,run)
   if run['direction']=='rtl':selected.reverse()
   result.extend(selected)
  actual.append(result)
 assert actual==expected,(name,actual,expected)
assert [(l['start'],l['end'])for l in byname['rtl-zwj-wrap']['lines']]==[(0,7),(7,10)]
assert [(l['start'],l['end'])for l in byname['flag-candidate']['lines']]==[(0,32),(32,34)]
assert [(l['start'],l['end'])for l in byname['rtl-zwj-discarded']['lines']]==[(0,2),(2,9),(9,12)]
assert [(l['start'],l['end'])for l in byname['flag-discarded']['lines']]==[(0,3),(3,32),(32,34)]
receipt={'cases':len(cases),'runs':count,'glyphs':glyphs,'fixed_wrap_vectors':4,'fixed_visual_vectors':5,'independent_atomic_grapheme_vectors':10,'additional_wrap_vectors':4,'harfbuzz_version':subprocess.check_output(['pkg-config','--modversion','harfbuzz'],text=True).strip(),'rust_export_sha256':hashlib.sha256((out/'paragraph-oracle.json').read_bytes()).hexdigest(),'oracle_source_sha256':hashlib.sha256((fixtures/'harfbuzz-paragraph.c').read_bytes()).hexdigest(),'scope':'exact contextual glyph IDs/clusters/positions plus independent authored wrap/visual vectors; no all-Unicode/layout/platform/deadline claim'}
(out/'independent-oracle-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt))
