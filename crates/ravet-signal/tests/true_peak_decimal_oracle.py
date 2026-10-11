"""Independent Decimal90 Taylor/Bessel phase coefficients, no native math calls.

Own equations only. No ITU/EBU tables, external audio or production code copied.
"""
from decimal import Decimal, localcontext, ROUND_FLOOR
from pathlib import Path
import argparse


def atan(x):
    power, total, i = x, x, 1
    while True:
        power *= -x*x
        following = total + power/(2*i+1)
        if following == total: return total
        total, i = following, i+1


def sine(x, pi):
    tau = 2*pi
    x -= (x/tau).to_integral_value(rounding=ROUND_FLOOR)*tau
    if x > pi: x -= tau
    term, total, i = x, x, 1
    while True:
        term *= -x*x/((2*i)*(2*i+1))
        following = total + term
        if following == total: return total
        total, i = following, i+1


def i0(x):
    term = total = Decimal(1)
    for i in range(1, 257):
        term *= x*x/(4*i*i)
        following = total + term
        if following == total: return total
        total = following
    raise AssertionError('independent Bessel series did not converge')


def row(half, factor, beta, phase, pi):
    if phase == 0: return [Decimal(int(j == half)) for j in range(2*half+1)]
    result = []
    for j in range(2*half+1):
        d = Decimal(j-half)+Decimal(phase)/factor
        if abs(d)>half: result.append(Decimal(0)); continue
        ratio = d/half
        window = i0(Decimal(beta)*(1-ratio*ratio).sqrt())/i0(Decimal(beta))
        result.append(sine(pi*d,pi)/(pi*d)*window)
    normalization = sum(result)
    return [x/normalization for x in result]


parser = argparse.ArgumentParser();parser.add_argument('output',type=Path);args=parser.parse_args()
records=[]
with localcontext() as ctx:
    ctx.prec=90
    pi=16*atan(Decimal(1)/5)-4*atan(Decimal(1)/239)
    for half in [32,128,512]:
        for factor in [4,8,16]:
            for beta in [0,6,12,16]:
                for phase in [1,factor//2,factor-1]:
                    coefficients=row(half,factor,beta,phase,pi)
                    for j in [0,1,half-1,half,half+1,2*half-1,2*half]:
                        records.append(f'{half},{factor},{beta},{phase},{j},{float(coefficients[j]):.17e}')
    for phase in range(8):
        coefficients=row(32,8,12,phase,pi)
        for j,value in enumerate(coefficients):records.append(f'32,8,12,{phase},{j},{float(value):.17e}')
args.output.write_text('\n'.join(records)+'\n')
print('independent Decimal90 coefficient samples',len(records))
