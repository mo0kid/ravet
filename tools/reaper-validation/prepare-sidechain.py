#!/usr/bin/env python3
"""Generate a public four-channel, two-second sidechain stimulus in an explicit test resource directory."""
import argparse
from pathlib import Path
import struct
import wave


def prepare(resource: Path) -> Path:
    destination = resource / "ravet-validation" / "sidechain-input.wav"
    destination.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(destination), "wb") as output:
        output.setparams((4, 2, 48000, 96000, "NONE", "not compressed"))
        output.writeframes(b"".join(
            struct.pack("<hhhh", 16384, -8192, 6553 if (frame // 24000) % 2 else 0, 0)
            for frame in range(96000)
        ))
    return destination


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("resource_directory", type=Path)
    print(prepare(parser.parse_args().resource_directory))
