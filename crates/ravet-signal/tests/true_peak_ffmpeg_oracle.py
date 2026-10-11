"""Optional worker comparison of own generated finite signals against FFmpeg.

Requires caller-selected executable paths. Writes only bounded temporary own raw
f64 PCM and IEEE-double WAVE files. Does not install, play audio or use standards
programme fixtures. FFmpeg's summary has0.1dB precision and a different filter;
this is separate reference evidence, not sample equality or EBU certification.
"""
import argparse, hashlib, math, re, struct, subprocess, tempfile
from pathlib import Path

parser=argparse.ArgumentParser()
parser.add_argument('example',type=Path)
parser.add_argument('--ffmpeg',required=True,type=Path)
parser.add_argument('--runner',type=Path)
args=parser.parse_args()
print('FFMPEG VERSION',subprocess.run([str(args.ffmpeg),'-version'],check=True,text=True,capture_output=True).stdout.splitlines()[0])
receipts=[]
with tempfile.TemporaryDirectory(prefix='ravet-truepeak-oracle-') as directory:
    directory=Path(directory)
    for rate in [44100,48000,96000,192000]:
        for frequency,offset in [(frequency,offset) for frequency in ([rate/4] if rate<=48000 else [12000.,rate/4]) for offset in [math.pi/4,math.pi/4+math.pi/32]]:
            half=128;length=rate+4*half
            signal=[]
            for n in range(length):
                ramp=min(1.,min(n,length-1-n)/(2*half))
                envelope=.5-.5*math.cos(math.pi*ramp)
                signal.append(envelope*math.cos(math.tau*frequency/rate*n+offset))
            payload=struct.pack('<'+'d'*length,*signal)
            raw=directory/'own-generated.f64le';wav=directory/'own-generated.wav';raw.write_bytes(payload)
            fmt=struct.pack('<HHIIHH',3,1,rate,rate*8,8,64)
            # Fact preserves the exact IEEE floating source-frame count.
            fact=struct.pack('<I',length)
            body=b'WAVE'+b'fmt '+struct.pack('<I',len(fmt))+fmt+b'fact'+struct.pack('<I',4)+fact+b'data'+struct.pack('<I',len(payload))+payload
            wav.write_bytes(b'RIFF'+struct.pack('<I',len(body))+body)
            command=([str(args.example)]if args.runner is None else['python3',str(args.runner),str(args.example)])+[str(raw),str(rate),'8',str(half),'12']
            own=subprocess.run(command,check=True,capture_output=True,text=True).stdout.strip()
            values=dict(v.split('=',1)for v in own.split())
            assert int(values['source_frames'])==length and int(values['tail_frames'])==2*half
            result=subprocess.run([str(args.ffmpeg),'-nostdin','-hide_banner','-nostats','-i',str(wav),'-af','ebur128=peak=true','-f','null','-'],check=True,text=True,capture_output=True)
            peak=re.findall(r'True peak:\s*Peak:\s*([+-]?[\d.]+)\s*dBFS',result.stderr)
            assert len(peak)==1,result.stderr
            reference=float(peak[0]);actual=float(values['db_tp']);delta=actual-reference
            print(f'EXTERNAL rate={rate} source_frames={length} codec=pcm_f64le byte_sha256={hashlib.sha256(payload).hexdigest()} frequency={frequency:.17e} phase={offset:.17e} own_dbTP={actual:.17e} ffmpeg_summary_dbTP={reference:.1f} delta={delta:.17e} summary_precision=0.1dB')
            if frequency <= 20000.:
                assert abs(delta)<.3,'selected standard-band reference disagreement requires diagnosis'
            receipts.append(delta)
print('EXTERNAL cases',len(receipts),'maximum observed absolute disagreement dB',max(map(abs,receipts)),'not a conformance certificate')
