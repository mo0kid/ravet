import decimal, struct
from pathlib import Path
D=decimal.Decimal
decimal.getcontext().prec=120
lines=['precision,input_bits,expected_bits']
for width in [32,64]:
 fmt,bfmt=('f','I') if width==32 else ('d','Q')
 fractions=[0,1,2,3,7,17,127,1023]
 patterns=(fractions+[0x007fffff,0x00800000,0x00800001,0x3e800000,0x3f000000,0x3f800000,0x3f800001,0x40000000,0x41100000,0x7f7fffff]) if width==32 else (fractions+[0x000fffffffffffff,0x0010000000000000,0x0010000000000001,0x3fd0000000000000,0x3fe0000000000000,0x3ff0000000000000,0x3ff0000000000001,0x4000000000000000,0x4022000000000000,0x7fefffffffffffff])
 for bits in patterns:
  value=struct.unpack('<'+fmt,struct.pack('<'+bfmt,bits))[0]
  root=D.from_float(value).sqrt()
  result=struct.unpack('<'+bfmt,struct.pack('<'+fmt,float(root)))[0]
  lines.append(f'{width},{bits:x},{result:x}')
# Regenerates tests/data/sqrt-decimal-oracle.csv beside this script.
data=Path(__file__).resolve().parent/'data'
data.mkdir(exist_ok=True)
(data/'sqrt-decimal-oracle.csv').write_text('\n'.join(lines)+'\n')
print('36 independently computed Decimal120 non-negative dyadic roots; f32 conversion may double-round. Tests require native hardware to match exactly and hold Scalar libm to 1 ULP')
