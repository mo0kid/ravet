#!/usr/bin/env python3
"""Independent CGPath tight bounds against analytically bounded Rust Beziers; no font bytes."""
import math,json,argparse
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('output',type=Path,nargs='?',default=Path(__file__).parent);base=parser.parse_args().output
native={}
for line in (base/'system-native-glyph-oracle.tsv').read_text().splitlines():
 p=line.split('\t');native[(p[0],int(p[2]))]=(int(p[1]),int(p[3]),*[float(x) for x in p[4:]])
def axis(points):
 # Bernstein Bezier evaluation at derivative roots, independent from either renderer.
 vals=[points[0],points[-1]]
 roots=[]
 if len(points)==3:
  a,b,c=points;den=a-2*b+c
  if den:roots=[(a-b)/den]
 elif len(points)==4:
  a,b,c,d=points;qa=-a+3*b-3*c+d;qb=2*(a-2*b+c);qc=b-a
  if abs(qa)<1e-14:
   if qb:roots=[-qc/qb]
  else:
   disc=qb*qb-4*qa*qc
   if disc>=0:roots=[(-qb+math.sqrt(disc))/(2*qa),(-qb-math.sqrt(disc))/(2*qa)]
 n=len(points)-1
 for t in roots:
  if 0<t<1:vals.append(sum(math.comb(n,i)*(1-t)**(n-i)*t**i*p for i,p in enumerate(points)))
 return min(vals),max(vals)
records=[];points=[];current=None;start=None
for line in (base/'system-rust-glyph-oracle.log').read_text().splitlines():
 p=line.split('\t')
 if p[0] not in ['glyph','end','M','L','Q','C','Z']:continue
 if p[0]=='glyph':
  key=(p[1],int(p[3]));header=(int(p[2]),int(p[4]),float(p[5]));points=[]
 elif p[0]=='end':
  bounds=(min(x[0] for x in points),min(x[1] for x in points),max(x[0] for x in points),max(x[1] for x in points))
  expected=native[key];nb=(expected[3],expected[4],expected[3]+expected[5],expected[4]+expected[6])
  assert header[:2]==expected[:2],(key,header,expected)
  records.append(dict(face=key[0],character=key[1],advanceError=abs(header[2]-expected[2]),boundError=max(abs(a-b) for a,b in zip(bounds,nb)),rustAdvance=header[2],nativeAdvance=expected[2],rustBounds=bounds,nativeBounds=nb))
 else:
  coords=[float(x) for x in p[1:]]
  if p[0]=='M':current=start=(coords[0],coords[1])
  elif p[0]=='Z':current=start
  else:
   nodes=[current]+list(zip(coords[::2],coords[1::2]));x=axis([x for x,y in nodes]);y=axis([y for x,y in nodes]);points.extend([(x[0],y[0]),(x[1],y[1])]);current=nodes[-1]
print(json.dumps({'glyphs':len(records),'maximumAdvanceError':max(r['advanceError'] for r in records),'maximumBoundError':max(r['boundError'] for r in records),'records':records},indent=2))

assert len(records)==40,("exact eight installed styles/five glyphs",len(records))
assert all(r['advanceError']<=1e-9 and r['boundError']<=1e-8 for r in records),"preserve engine disagreement before qualification"
