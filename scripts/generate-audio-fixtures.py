#!/usr/bin/env python3
"""Regenerate public synthetic codec fixtures; requires a local ffmpeg encoder."""
import math
from pathlib import Path
import shutil
import struct
import subprocess
import wave

root = Path(__file__).resolve().parents[1]
fixtures = root / "crates/ravet-audio-file/tests/fixtures"
fixtures.mkdir(parents=True, exist_ok=True)
source = fixtures / "sine.wav"
samples = []
for frame in range(4096):
    samples.extend((round(8192 * math.sin(frame * 2 * math.pi * 1000 / 48000)),
                    round(16384 * math.sin(frame * 2 * math.pi * 500 / 48000))))
with wave.open(str(source), "wb") as output:
    output.setparams((2, 2, 48000, 0, "NONE", "not compressed"))
    output.writeframes(struct.pack(f"<{len(samples)}h", *samples))
encoder = shutil.which("ffmpeg")
if encoder is None:
    raise SystemExit("Install/use a local ffmpeg with FLAC, libmp3lame and libvorbis encoders")
for extension, options in [("flac", ["-c:a", "flac"]),
                           ("mp3", ["-c:a", "libmp3lame", "-q:a", "2"]),
                           ("ogg", ["-c:a", "libvorbis", "-q:a", "5"])]:
    subprocess.run([encoder, "-nostdin", "-loglevel", "error", "-y", "-i", str(source),
                    "-map_metadata", "-1", "-fflags", "+bitexact", "-flags:a", "+bitexact",
                    *options, str(fixtures / f"sine.{extension}")], check=True)
print("Generated only public synthetic stereo tones (4096 frames, 48 kHz).")
