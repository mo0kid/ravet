#!/usr/bin/env python3
"""Compile actual portable color sources and check independent Decimal formulas."""
import argparse
from decimal import Decimal as D, localcontext, ROUND_HALF_UP
import json
from pathlib import Path
import shlex
import subprocess


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,default=Path('target/color-preparation'))
    parser.add_argument('--runner',default='',help='Optional executable runner parsed as arguments; no shell.')
    options=parser.parse_args()
    root=Path(__file__).resolve().parent.parent
    output=options.output.resolve();output.mkdir(parents=True,exist_ok=True)
    executable=output/'color-reference'
    subprocess.run(['rustc','--edition','2024','-D','warnings',str(root/'crates/ravet-surface/tests/fixtures/color/reference.rs'),'-o',str(executable)],check=True)
    result=subprocess.run([*shlex.split(options.runner),str(executable)],check=True,capture_output=True)
    if not result.stdout or len(result.stdout)>2*1024*1024:
        raise RuntimeError('Reference JSON capacity exceeded')
    payload=result.stdout.decode('utf-8')
    (output/'reference.json').write_text(payload)
    with localcontext() as context:
     context.prec=70
     zero=D(0);one=D(1);byte=D(255)
     def decode(v,profile):
      if profile&1:return v
      return v/D('12.92') if v<=D('0.04045') else ((v+D('0.055'))/D('1.055'))**D('2.4')
     def encode(v,profile):
      if profile&1:return v
      return v*D('12.92') if v<=D('0.0031308') else D('1.055')*(v**(one/D('2.4')))-D('0.055')
     def normalized(pixel,profile):
      alpha=D(pixel[3])/byte
      return [decode(D(c)/D(pixel[3]) if pixel[3] else zero,profile) if profile&2 else decode(D(c)/byte,profile) for c in pixel[:3]]+[alpha]
     def quantize(v):return int((max(zero,min(one,v))*byte).quantize(one,rounding=ROUND_HALF_UP))
     data=json.loads(payload)
     assert len(data['scalars'])==256 and len(data['conversions'])==4096 and len(data['cases'])==2048
     for c,actual in data['scalars']:
      expected=decode(D(c)/byte,0)
      assert abs(expected-D(str(actual)))<D('0.000000000000002'),(c,expected,actual)
     for pixel,sp,tp,actual in data['conversions']:
      linear=normalized(pixel,sp);alpha=linear[3]
      if (sp&1)==(tp&1):
       straight=[D(c)/D(pixel[3]) if pixel[3] else zero for c in pixel[:3]] if sp&2 else [D(c)/byte for c in pixel[:3]]
       encoded=[v*(alpha if tp&2 else one) for v in straight]
      else:
       encoded=[encode(v,tp)*(alpha if tp&2 else one) for v in linear[:3]]
      expected=[quantize(v) for v in encoded]+[quantize(alpha)]
      assert expected==actual,(pixel,sp,tp,expected,actual)
     for s,b,sp,bp,tp,actual in data['cases']:
      source=normalized(s,sp);backdrop=normalized(b,bp);sa=source[3];ba=backdrop[3];alpha=sa+ba*(one-sa)
      straight=[(source[i]*sa+backdrop[i]*ba*(one-sa))/alpha if alpha else zero for i in range(3)]
      encoded=[encode(v,tp)*(alpha if tp&2 else one) for v in straight]
      expected=[quantize(v) for v in encoded]+[quantize(alpha)]
      assert expected==actual,(s,b,sp,bp,tp,expected,actual)
     print(f"Independent70-digit Decimal: {len(data['scalars'])} transfer samples + {len(data['conversions'])} profile conversions + {len(data['cases'])} mixed-profile source-over cases PASS")


if __name__=="__main__":
    main()
