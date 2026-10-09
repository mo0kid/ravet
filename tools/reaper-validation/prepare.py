"""Generate public synthetic fixtures in a disposable REAPER resource directory."""
from pathlib import Path
import argparse
import struct
import wave

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('resource_directory', type=Path)
args = parser.parse_args()
output = args.resource_directory.resolve() / 'ravet-validation'
output.mkdir(parents=True, exist_ok=True)
(output / 'public-fixture.txt').write_text('RAVET public synthetic validation\n')
with wave.open(str(output / 'input.wav'), 'wb') as audio:
    audio.setnchannels(2)
    audio.setsampwidth(2)
    audio.setframerate(48000)
    audio.writeframes(struct.pack('<hh',8192,-16384) * (48000 * 4))
print(output)
