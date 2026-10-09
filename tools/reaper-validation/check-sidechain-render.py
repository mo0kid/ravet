#!/usr/bin/env python3
"""Check actual DAW-rendered main samples from the public four-channel sidechain stimulus."""
import argparse
from pathlib import Path
import wave


def check(path: Path) -> None:
    with wave.open(str(path), "rb") as audio:
        channels, width, rate, count = audio.getnchannels(), audio.getsampwidth(), audio.getframerate(), audio.getnframes()
        if not (2 <= channels <= 4 and width in (2, 3, 4) and 8000 <= rate <= 192000 and 2 * rate <= count <= 10 * rate):
            raise ValueError("unexpected bounded PCM render shape")
        divisor = 2 ** (width * 8 - 1)
        for phase in range(4):
            audio.setpos(round((phase * 0.5 + 0.1) * rate))
            frames = round(0.3 * rate)
            samples = audio.readframes(frames)
            if len(samples) != frames * channels * width:
                raise ValueError("truncated validation region")
            expected = (0.5, -0.25) if phase % 2 else (0.0, 0.0)
            maximum = 0.0
            for frame in range(frames):
                offset = frame * channels * width
                for channel in (0, 1):
                    start = offset + channel * width
                    value = int.from_bytes(samples[start:start + width], "little", signed=True) / divisor
                    maximum = max(maximum, abs(value - expected[channel]))
            if maximum > 0.0003:
                raise ValueError(f"detector phase {phase + 1}: maximum main sample error {maximum}")
            print(f"PASS detector phase {phase + 1}: {frames} actual DAW frames, maximum error {maximum:.9g}")
        print(f"PASS native sidechain render: {channels} channels, {rate} Hz, PCM{width * 8}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("rendered_wave", type=Path)
    check(parser.parse_args().rendered_wave)
