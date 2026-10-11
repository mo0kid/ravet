"""450-digit independent exp/Newton roots of actual native history equations."""
from decimal import Decimal, localcontext
from pathlib import Path
import argparse, csv

def tanh(x):
    if x > 500: return Decimal(1)
    if x < -500: return Decimal(-1)
    e = (2*x).exp()
    return (e-1)/(e+1)

p = argparse.ArgumentParser()
p.add_argument('inputs', type=Path)
p.add_argument('output', type=Path)
a = p.parse_args()
rows = []
with localcontext() as ctx:
    ctx.prec = 450
    for row in csv.reader(a.inputs.read_text().splitlines()):
        kind, ratio, resonance, drive, pattern, warmup, x, c, lam = row
        c, lam = Decimal.from_float(float(c)), Decimal.from_float(float(lam))
        u = tanh(c)/(1+lam)
        for _ in range(80):
            t = tanh(c-lam*u)
            delta = (u-t)/(1+lam*(1-t*t))
            v = u-delta
            if v == u: break
            u = v
        assert abs(u-tanh(c-lam*u)) < Decimal('1e-440') or abs(u) < Decimal('1e-440')
        rows.append(','.join(row[:7]+[format(float(u),'.17e')]+row[7:]))
a.output.write_text('\n'.join(rows)+'\n')
print('450-digit history/cancellation/knee roots',len(rows))
