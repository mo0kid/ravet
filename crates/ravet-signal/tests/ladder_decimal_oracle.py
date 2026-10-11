from decimal import Decimal,localcontext
from pathlib import Path
import math,csv,argparse
# 450-digit exp-based tanh/Newton, independently generated from exact rounded
# equation metadata (not production solver), including tiny/subnormal amplitudes.
def tanh(x):
 if x>500:return Decimal(1)
 if x<-500:return Decimal(-1)
 e=(2*x).exp();return (e-1)/(e+1)
parser=argparse.ArgumentParser(description='Independent450-digit nonlinear-ladder root oracle')
parser.add_argument('inputs',type=Path)
parser.add_argument('output',type=Path)
args=parser.parse_args()
rows=[]
with localcontext() as ctx:
 ctx.prec=450
 for line in csv.reader(args.inputs.read_text().splitlines()):
  kind,ratio,r,drive,x,c,lam=line
  cf=float(c);lf=float(lam)
  if math.isinf(cf):root=math.copysign(1,cf)
  else:
   c=Decimal.from_float(cf);lam=Decimal.from_float(lf);u=tanh(c)/(1+lam)
   for _ in range(80):
    t=tanh(c-lam*u);delta=(u-t)/(1+lam*(1-t*t));nextu=u-delta
    if nextu==u:break
    u=nextu
   assert abs(u-tanh(c-lam*u))<Decimal('1e-440') or abs(u)<Decimal('1e-440')
   root=float(u)
  rows.append(','.join([kind,ratio,r,drive,x,format(root,'.17e')]))
args.output.write_text('\n'.join(rows)+'\n')
print('450-digit independent exp/Newton root vectors',len(rows))
