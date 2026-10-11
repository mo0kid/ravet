"""Independent 450-digit alternate-equation roots for drift-bound qualification.

These deliberately synthetic perturbations are 1e-4; they are neither observed
platform-libm drift nor production solver samples. The original and alternate
roots use Decimal exp/Newton, without native tanh or production solver calls.
"""
from decimal import Decimal, localcontext
from pathlib import Path
import argparse


def tanh(x):
    e = (2 * x).exp()
    return (e - 1) / (e + 1)


def root(c, lam):
    c, lam = Decimal.from_float(c), Decimal.from_float(lam)
    u = tanh(c) / (1 + lam)
    for _ in range(80):
        t = tanh(c - lam * u)
        v = u - (u - t) / (1 + lam * (1 - t * t))
        if v == u:
            break
        u = v
    assert abs(u - tanh(c - lam * u)) < Decimal('1e-440')
    return float(u)


parser = argparse.ArgumentParser()
parser.add_argument('output', type=Path)
args = parser.parse_args()
rows = []
with localcontext() as ctx:
    ctx.prec = 450
    for c in [-2., -.5, 0., .3, 1.]:
        for lam in [0., .13, 1., 2.2]:
            reference = root(c, lam)
            for dc, dl in [(1e-4, 0.), (0., 1e-4), (1e-4, 1e-4)]:
                actual_c, actual_lam = c + dc, lam + dl
                actual = root(actual_c, actual_lam)
                rows.append(','.join(format(v, '.17e') for v in
                                     [c, lam, reference, actual_c, actual_lam, actual]))
args.output.write_text('\n'.join(rows) + '\n')
print('independent synthetic alternate-equation vectors', len(rows))
