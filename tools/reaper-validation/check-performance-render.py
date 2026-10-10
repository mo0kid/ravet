#!/usr/bin/env python3
"""Check bounded synthetic DAW instrument renders, independently of live devices."""
import argparse
import math
from pathlib import Path
import wave


def check_wave(path: Path, label: str) -> None:
    with wave.open(str(path), "rb") as audio:
        channels, width, rate, count = (
            audio.getnchannels(), audio.getsampwidth(),
            audio.getframerate(), audio.getnframes(),
        )
        if not (channels == 2 and width in (2, 3, 4) and rate == 48000 and
                4 * rate <= count <= 30 * rate):
            raise ValueError(f"{label}: unexpected bounded stereo PCM render shape")
        divisor = 2 ** (width * 8 - 1)
        # Interior held regions avoid attack/decay, release, and envelope edges.
        regions = [(0.05, 0.20, 0.0), (0.6, 1.2, 0.04),
                   (1.7, 2.1, 0.04), (2.5, 3.5, 0.0)]
        if label == "automation":
            regions[1] = (0.6, 1.2, 0.02)
        tolerance = max(4 / divisor, 0.00003)
        for first, last, amplitude in regions:
            audio.setpos(round(first * rate))
            frames = round((last - first) * rate)
            payload = audio.readframes(frames)
            if len(payload) != frames * channels * width:
                raise ValueError(f"{label}: truncated region")
            peak = square = 0.0
            crossings = 0
            previous = None
            for frame in range(frames):
                offset = frame * channels * width
                left = int.from_bytes(payload[offset:offset + width], "little", signed=True) / divisor
                right = int.from_bytes(payload[offset + width:offset + 2 * width], "little", signed=True) / divisor
                if abs(left - right) > tolerance:
                    raise ValueError(f"{label}: stereo disagreement")
                peak = max(peak, abs(left))
                square += left * left
                if previous is not None and previous <= 0 < left:
                    crossings += 1
                previous = left
            rms = math.sqrt(square / frames)
            if abs(peak - amplitude) > tolerance or abs(rms - amplitude / math.sqrt(2)) > tolerance:
                raise ValueError(f"{label} {first}-{last}s: peak={peak}, RMS={rms}, expected amplitude={amplitude}")
            if amplitude and abs(crossings - 440 * (last - first)) > 2:
                raise ValueError(f"{label}: expected A4 frequency, got {crossings} positive crossings")
            print(f"PASS {label} {first}-{last}s: {frames} actual DAW frames, peak {peak:.8g}, RMS {rms:.8g}")


def check_resource(resource: Path) -> None:
    fixture = (resource / "ravet-validation").resolve()
    log = (fixture / "performance-host.log").read_text()
    if "FAIL " in log or "PASS REAPER VST3 MIDI/state/automation renders prepared" not in log:
        raise ValueError("host fixture did not complete successfully")
    paths = {}
    for line in log.splitlines():
        if line.startswith("rendered "):
            label, raw = line[len("rendered "):].split("=", 1)
            path = Path(raw).resolve()
            if not path.is_relative_to(fixture):
                raise ValueError("render must remain inside the public fixture directory")
            if label in paths:
                raise ValueError("duplicate render label")
            paths[label] = path
    if set(paths) != {"level", "recalled", "automation"}:
        raise ValueError("three distinct synthetic renders required")
    for label in ("level", "recalled", "automation"):
        check_wave(paths[label], label)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("resource_directory", type=Path)
    check_resource(parser.parse_args().resource_directory)
