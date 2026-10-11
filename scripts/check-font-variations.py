#!/usr/bin/env python3
"""Independent installed HarfBuzz/fontTools checks for the authored variable fixture."""
import argparse,shlex,subprocess
from pathlib import Path

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,default=Path(__file__).resolve().parents[1]);p.add_argument('--output',type=Path,default=Path('target/font-variations'));p.add_argument('--fixtures',type=Path);a=p.parse_args();root=a.root.resolve();out=a.output.resolve();out.mkdir(parents=True,exist_ok=True);fixtures=(a.fixtures or root/'crates/ravet-surface/tests/fixtures/variation').resolve()
 version=subprocess.check_output(['pkg-config','--modversion','harfbuzz'],text=True).strip();flags=shlex.split(subprocess.check_output(['pkg-config','--cflags','--libs','harfbuzz'],text=True));binary=out/'harfbuzz-variation';subprocess.run(['clang','-std=c11','-Wall','-Wextra','-Werror',str(fixtures/'harfbuzz-variation.c'),'-o',str(binary),*flags],check=True)
 actual=subprocess.check_output([str(binary),str(fixtures/'RavetVariation.ttf')],text=True)
 expected=['400 100 2 500 10 700 470 -700','900 100 2 600 10 700 550 -700','100 100 2 450 10 700 430 -700','400 200 2 700 10 700 670 -700','400 50 2 400 10 700 370 -700','650 150 2 650 10 700 610 -700']
 if actual.splitlines()!=expected:raise RuntimeError('independent analytic glyph/advance/extents disagreement: '+actual)
 from fontTools.ttLib import TTFont
 from fontTools.varLib.instancer import instantiateVariableFont
 for weight,width,advance,right in [(400,100,500,480),(900,100,600,560),(100,100,450,440),(400,200,700,680),(400,50,400,380),(650,150,650,620)]:
  font=TTFont(fixtures/'RavetVariation.ttf');instantiateVariableFont(font,{'wght':weight,'wdth':width},inplace=True)
  if font['hmtx']['a'][0]!=advance or max(x for x,y in font['glyf']['a'].coordinates)!=right:raise RuntimeError('independent fontTools analytic instance disagreement')
  font.close()
 receipt=f'Independent HarfBuzz {version}\n'+actual+'6 HarfBuzz +6 fontTools analytic profiles PASS\n';(out/'independent.log').write_text(receipt);print(receipt,end='')
if __name__=='__main__':main()
