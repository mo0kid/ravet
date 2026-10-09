"""Check actual REAPER-rendered wet, bypass and automated synthetic stereo audio."""
from pathlib import Path
import argparse
import struct
import wave

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('directory', type=Path)
args = parser.parse_args()
for name, sections in {
    'wet': [(0.05, 3.9, (0.375, -0.75))],
    'dry': [(0.05, 3.9, (0.25, -0.5))],
    'automation': [(0.05, 1.9, (0.125, -0.25)), (2.05, 3.9, (0.5, -1.0))],
}.items():
    files = list(args.directory.glob(name + '*.wav'))
    assert len(files) == 1, f'{name}: expected one render, got {files}'
    with wave.open(str(files[0]), 'rb') as audio:
        rate = audio.getframerate()
        width = audio.getsampwidth()
        assert audio.getnchannels() == 2 and rate == 48000
        assert audio.getnframes() == 4 * rate
        assert width in (2, 3, 4), f'unsupported PCM width {width}'
        for start, end, expected in sections:
            audio.setpos(round(start * rate))
            raw = audio.readframes(round((end - start) * rate))
            if width == 2:
                samples = struct.unpack('<' + 'h' * (len(raw) // 2), raw)
            elif width == 4:
                samples = struct.unpack('<' + 'i' * (len(raw) // 4), raw)
            else:
                samples = [int.from_bytes(raw[i:i+3], 'little', signed=True) for i in range(0, len(raw), 3)]
            scale = 2 ** (width * 8 - 1)
            errors = [max(abs(v / scale - expected[channel]) for v in samples[channel::2]) for channel in (0, 1)]
            assert max(errors) <= 2 / scale, f'{name} {start}-{end}: channel errors {errors}'
            print(f'PASS {name} {start}-{end}s {len(samples)//2} frames, channel errors {errors}')
