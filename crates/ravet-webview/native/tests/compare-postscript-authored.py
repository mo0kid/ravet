#!/usr/bin/env python3
"""Compare explicit closing-line-normalized contours; preserve native differences."""
from pathlib import Path
import argparse,json,math
p=argparse.ArgumentParser();p.add_argument('output',type=Path);a=p.parse_args()
def read(path):
 records={};record=None
 for line in path.read_text().splitlines():
  fields=line.split('\t')
  if fields[0]=='glyph':
   _,backend,file,user,glyph,advance,normalized=fields
   key=(Path(file).name,int(float(user)))
   record=dict(backend=backend,glyph=int(glyph),advance=float(advance),normalized=float(normalized),commands=[])
  elif fields[0]=='end':
   commands=record['commands'];commands=[c for c in commands if c[0]!='Z']
   if commands[-1][0]=='L' and commands[-1][1:]==commands[0][1:]:commands.pop()
   record['commands']=commands;records[key]=record;record=None
  elif fields[0]in ['M','L','Q','C','Z'] and record is not None:
   record['commands'].append([fields[0],*[float(x)for x in fields[1:]]])
 return records
rust=read(a.output/'authored-rust-outline-oracle-final.tsv')
foreign=read(a.output/'authored-native-outline-oracle-ordered.tsv')
# Native file interleaves two backends; parse each independently without loss.
records={}
for backend in ['HB','FT']:
 source=(a.output/'authored-native-outline-oracle-ordered.tsv').read_text().splitlines();selected=[];active=False
 for line in source:
  if line.startswith('glyph\t'):active=line.split('\t')[1]==backend
  if active:selected.append(line)
 path=a.output/('authored-'+backend+'-selected.tsv');path.write_text('\n'.join(selected)+'\n');records[backend]=read(path)
records['CT']=read(a.output/'authored-coretext-outline-oracle.tsv')
assert len(rust)==10 and all(len(r)==10 for r in records.values())
report=[]
for key,r in rust.items():
 f=records['FT'][key]
 assert r['glyph']==f['glyph']==2
 assert r['commands']==f['commands'],(key,r['commands'],f['commands'])
 assert abs(r['advance']-f['advance'])<1e-9,(key,r['advance'],f['advance'])
 if key[1]:assert r['normalized']==f['normalized']/4==records['HB'][key]['normalized']
 differences={}
 for backend in ['HB','CT']:
  v=records[backend][key]
  assert r['glyph']==v['glyph']
  assert [c[0]for c in r['commands']]==[c[0]for c in v['commands']],(key,backend)
  error=max(abs(x-y)for c,d in zip(r['commands'],v['commands'])for x,y in zip(c[1:],d[1:]))
  differences[backend]=dict(maximumControlDifference=error,advanceDifference=r['advance']-v['advance'],advance=v['advance'])
 report.append(dict(font=key[0],user=key[1],rustAdvance=r['advance'],actualNormalized2_14=r['normalized'],freeTypeCommandsExact=True,engineDifferences=differences))
assert any(r['engineDifferences']['HB']['maximumControlDifference']>0 for r in report)
assert any(r['engineDifferences']['CT']['advanceDifference']!=0 for r in report)
print(json.dumps(dict(profiles=len(report),nativeVersions='HarfBuzz14.4.0 / FreeType2.14.3; CoreText own-process paths',closingPolicy='ignore Z and a final explicit line returning to the first move; retain all cubic commands including degenerate',records=report),indent=2))
