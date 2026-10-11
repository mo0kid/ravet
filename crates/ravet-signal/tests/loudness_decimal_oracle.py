"""Own Decimal90 bilinear/complex-response oracle, no native transcendental calls.
Calibration parameter facts attributed in the loudness guide. No vendor body/table.
"""
from decimal import Decimal as D, localcontext
from pathlib import Path
import argparse

def atan(x):
    term=total=x; k=1
    while True:
        term *= -x*x
        new=total+term/(2*k+1)
        if new==total:return total
        total=new;k+=1

def sincos(x):
    st=ss=x; ct=cs=D(1); k=1
    while True:
        st *= -x*x/((2*k)*(2*k+1));ct *= -x*x/((2*k-1)*(2*k))
        sn=ss+st;cn=cs+ct
        if sn==ss and cn==cs:return ss,cs
        ss,cs=sn,cn;k+=1

def sections(rate,pi):
    # Bilinear expansion by multiplying numerator and denominator by k^2(1+q)^2.
    f=D('1681.9744509555319');q=D('0.7071752369554193')
    gain=D('3.99984385397');exp=D('0.499666774155')
    sine,cosine=sincos(pi*f/rate);k=sine/cosine
    vh=(D(10).ln()*gain/20).exp(); vb=(vh.ln()*exp).exp()
    den=[1+k/q+k*k,2*(k*k-1),1-k/q+k*k]
    num=[vh+vb*k/q+k*k,2*(k*k-vh),vh-vb*k/q+k*k]
    shelf=( [v/den[0] for v in num], [v/den[0] for v in den[1:]] )
    f=D('38.13547087613982');q=D('0.5003270373253953')
    sine,cosine=sincos(pi*f/rate);k=sine/cosine
    den=[1+k/q+k*k,2*(k*k-1),1-k/q+k*k]
    return [shelf,([D(1),D(-2),D(1)],[v/den[0] for v in den[1:]])]

def response(rows,frequency,rate,pi):
    sn,cs=sincos(2*pi*frequency/rate)
    sn2=2*sn*cs;cs2=2*cs*cs-1
    result=D(1)
    for b,a in rows:
        re=b[0]+b[1]*cs+b[2]*cs2;im= -b[1]*sn-b[2]*sn2
        dre=1+a[0]*cs+a[1]*cs2;dim= -a[0]*sn-a[1]*sn2
        result *= ((re*re+im*im)/(dre*dre+dim*dim)).sqrt()
    return result

parser=argparse.ArgumentParser();parser.add_argument('output',type=Path);args=parser.parse_args()
records=[]
with localcontext() as ctx:
    ctx.prec=90;pi=16*atan(D(1)/5)-4*atan(D(1)/239)
    for rate in list(range(8000,8010))+[16000,22050,44100,48000,96000,192000,768000]:
        rows=sections(D(rate),pi)
        for stage,(b,a) in enumerate(rows):
            for index,v in enumerate(b+a):records.append(f'C,{rate},{stage},{index},{float(v):.17e}')
        for frequency in [D(37),D(137),D(997),D(rate)/4,D(rate)/2]:
            v=response(rows,frequency,D(rate),pi)
            records.append(f'R,{rate},{frequency},0,{float(v):.17e}')
args.output.write_text('\n'.join(records)+'\n')
print('independent Decimal90 coefficient/transfer rows',len(records))
