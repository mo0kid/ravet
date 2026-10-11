"""Optional worker comparison on independently generated double PCM.
Explicit installed executables; no devices, installation or standards fixtures.
Original I/M/S and explicitly zero-padded LRA references are kept separate.
"""
import argparse,hashlib,math,re,struct,subprocess,tempfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('example',type=Path);p.add_argument('--ffmpeg',required=True,type=Path);p.add_argument('--runner',type=Path);args=p.parse_args()
print('FFMPEG VERSION',subprocess.run([str(args.ffmpeg),'-version'],check=True,capture_output=True,text=True).stdout.splitlines()[0])
def wave(path,payload,rate,channels):
    if channels==6:
        guid=bytes.fromhex('0300000000001000800000aa00389b71')
        fmt=struct.pack('<HHIIHHHHI',0xfffe,channels,rate,rate*channels*8,channels*8,64,22,64,0x3f)+guid
    else:fmt=struct.pack('<HHIIHH',3,channels,rate,rate*channels*8,channels*8,64)
    fact=struct.pack('<I',len(payload)//(8*channels))
    body=b'WAVE'+b'fmt '+struct.pack('<I',len(fmt))+fmt+b'fact'+struct.pack('<I',4)+fact+b'data'+struct.pack('<I',len(payload))+payload
    path.write_bytes(b'RIFF'+struct.pack('<I',len(body))+body)
def ffmpeg(path):
    result=subprocess.run([str(args.ffmpeg),'-nostdin','-hide_banner','-nostats','-i',str(path),'-af','ebur128','-f','null','-'],check=True,text=True,capture_output=True)
    summary=result.stderr.split('Summary:')[-1]
    i=re.findall(r'I:\s*([+-]?[\d.]+)\s*LUFS',summary);lra=re.findall(r'LRA:\s*([\d.]+)\s*LU',summary)
    live=re.findall(r'\bM:\s*([+-]?[\d.]+)\s*S:\s*([+-]?[\d.]+)',result.stderr)
    assert len(i)==1 and len(lra)==1 and live,result.stderr
    return {'I':float(i[0]),'LRA':float(lra[0]),'M':float(live[-1][0]),'S':float(live[-1][1])}
receipts=[]
with tempfile.TemporaryDirectory(prefix='ravet-loudness-oracle-') as folder:
    folder=Path(folder)
    for rate,length in [(8000,320000),(8001,320040),(44100,220500),(48000,240000),(96000,316800)]:
        for channels in [1,2,6]:
            roles={1:'f',2:'f,f',6:'f,f,f,x,s,s'}[channels]
            payload=bytearray()
            for n in range(length):
                amp=.01 if rate<=8001 and n<length//2 else .1
                values=[amp*math.sin(math.tau*997*n/rate+ch*.4) for ch in range(channels)]
                if channels==6:values[3]=.9*math.sin(math.tau*50*n/rate) # explicitly excluded LFE
                payload.extend(struct.pack('<'+'d'*channels,*values))
            assert len(payload)<=16*1024*1024
            raw=folder/'own.f64le';raw.write_bytes(payload)
            original=folder/'original.wav';wave(original,payload,rate,channels)
            command=([str(args.example)]if args.runner is None else['python3',str(args.runner),str(args.example)])+[str(raw),str(rate),roles,'lra1500']
            own=subprocess.run(command,check=True,text=True,capture_output=True).stdout.strip();values=dict(v.split('=',1)for v in own.split())
            tail=(3*rate+1)//2
            assert int(values['source_frames'])==length and int(values['tail_frames'])==tail
            reference=ffmpeg(original)
            padded=folder/'padded.wav';wave(padded,payload+bytes(tail*channels*8),rate,channels)
            tail_reference=ffmpeg(padded)
            delta={key:float(values[key])-(tail_reference if key=='LRA' else reference)[key] for key in ['I','LRA','M','S']}
            print(f'EXTERNAL rate={rate} channels={channels} roles={roles} original={length} artificial_tail={tail} pcm_sha256={hashlib.sha256(payload).hexdigest()} own={own} ffmpeg_original={reference} ffmpeg_padded_LRA={tail_reference["LRA"]} deltas={delta} summary_precision=0.1LU',flush=True)
            # Selected normal-rate I and complete-window M/S should agree within
            # printed precision plus the separately documented histogram rounding.
            for key in ['I','M','S']:
                assert abs(delta[key])<.12,(rate,channels,key,delta)
            # LRA algorithm/grid/percentile differences are visible observations,
            # not a normative assertion or hidden admission-threshold waiver.
            receipts.append((rate,channels,delta))
print('EXTERNAL15 files maxabs', {key:max(abs(v[key])for _,_,v in receipts)for key in ['I','M','S','LRA']},'no standards certificate')
