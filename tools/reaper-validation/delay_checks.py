#!/usr/bin/env python3
"""Check REAPER renders of the RAVET Delay host check (see delay.py).

Reads `cases.json`, `delay-host.log` and the rendered WAV files in a work
directory. For every case the synthetic input holds isolated stereo impulses;
with feedback 0 the output must hold the dry impulse scaled by (1 - mix) at the
impulse frame, a single echo scaled by mix exactly `delay` frames later, and
silence elsewhere. Prints PASS/FAIL lines and exits non-zero on any failure.
"""
from __future__ import annotations

import argparse
import json
import struct
import sys
from pathlib import Path

# Synthetic source: impulse frames (in seconds) and per-channel amplitudes.
IMPULSE_SECONDS = (0.5, 2.0)
AMPLITUDES = (0.5, -0.25)
LENGTH_SECONDS = 4.0
# Amplitude/silence tolerance: well above 24-bit quantization and f32 rounding,
# far below any misplaced or interpolated echo.
TOLERANCE = 1e-4


def impulse_frames(rate: int) -> list[int]:
    return [round(seconds * rate) for seconds in IMPULSE_SECONDS]


def synced_delay_ms(beats: float, tempo: float) -> float:
    """Delay of a note division, in quarter notes, at a tempo in BPM."""
    return beats * 60_000.0 / tempo


def delay_frames(delay_ms: float, rate: int) -> int:
    frames = delay_ms * rate / 1000.0
    if abs(frames - round(frames)) > 1e-9:
        raise ValueError(f'{delay_ms} ms is not a whole number of frames at {rate} Hz')
    return round(frames)


def write_input(path: Path, rate: int) -> None:
    """Write the 16-bit stereo impulse source (amplitudes are exact in 16 bits)."""
    frames = round(LENGTH_SECONDS * rate)
    data = bytearray(frames * 4)
    for frame in impulse_frames(rate):
        struct.pack_into('<hh', data, frame * 4, *(round(a * 32768) for a in AMPLITUDES))
    write_wav(path, rate, 2, 16, bytes(data))


def write_wav(path: Path, rate: int, channels: int, bits: int, data: bytes, float_format: bool = False) -> None:
    block = channels * bits // 8
    header = struct.pack('<4sI4s4sIHHIIHH4sI', b'RIFF', 36 + len(data), b'WAVE', b'fmt ', 16,
                         3 if float_format else 1, channels, rate, rate * block, block, bits,
                         b'data', len(data))
    path.write_bytes(header + data)


def read_wav(path: Path) -> tuple[int, list[list[float]]]:
    """Read PCM 16/24/32-bit or 32/64-bit float WAV (plain or extensible)."""
    raw = path.read_bytes()
    if raw[:4] not in (b'RIFF', b'RF64') or raw[8:12] != b'WAVE':
        raise ValueError(f'{path.name}: not a WAV file')
    position, fmt, data = 12, None, None
    while position + 8 <= len(raw):
        chunk, size = struct.unpack_from('<4sI', raw, position)
        body = raw[position + 8:position + 8 + size]
        if chunk == b'fmt ':
            fmt = body
        elif chunk == b'data':
            data = body
        position += 8 + size + (size & 1)
    if fmt is None or data is None:
        raise ValueError(f'{path.name}: missing fmt or data chunk')
    tag, channels, rate, _, block, bits = struct.unpack_from('<HHIIHH', fmt)
    if tag == 0xFFFE:
        tag = struct.unpack_from('<H', fmt, 24)[0]
    width = bits // 8
    frames = len(data) // block
    if tag == 3 and bits in (32, 64):
        values = struct.unpack('<' + ('f' if bits == 32 else 'd') * (frames * channels), data[:frames * block])
    elif tag == 1 and bits in (16, 24, 32):
        scale = float(2 ** (bits - 1))
        if bits == 24:
            values = [int.from_bytes(data[i:i + 3], 'little', signed=True) / scale
                      for i in range(0, frames * block, 3)]
        else:
            values = [v / scale for v in struct.unpack('<' + ('h' if bits == 16 else 'i') * (frames * channels),
                                                         data[:frames * block])]
    else:
        raise ValueError(f'{path.name}: unsupported WAV format tag {tag} with {bits} bits')
    del width
    return rate, [list(values[c::channels]) for c in range(channels)]


def check_case(name: str, rate: int, channels: list[list[float]], expected_rate: int,
               delay_ms: float, mix: float) -> list[str]:
    """Return PASS lines; raise AssertionError with a FAIL description."""
    lines = []
    if rate != expected_rate:
        raise AssertionError(f'{name}: render rate {rate} Hz, expected {expected_rate} Hz')
    if len(channels) != 2:
        raise AssertionError(f'{name}: {len(channels)} channels, expected stereo')
    frames = len(channels[0])
    minimum = round(LENGTH_SECONDS * rate)
    if frames < minimum:
        raise AssertionError(f'{name}: {frames} frames, expected at least {minimum}')
    delay = delay_frames(delay_ms, rate)
    expected: dict[int, list[float]] = {}
    for frame in impulse_frames(rate):
        for offset, gain in ((0, 1.0 - mix), (delay, mix)):
            values = expected.setdefault(frame + offset, [0.0, 0.0])
            for c in range(2):
                values[c] += gain * AMPLITUDES[c]
    for c, samples in enumerate(channels):
        for frame in impulse_frames(rate):
            # Locate the echo independently: the largest sample after the dry
            # impulse and before the next impulse (or the end of the render).
            later = [f for f in impulse_frames(rate) if f > frame]
            end = min(later[0] if later else frames, frames)
            window = range(frame + 1, end)
            found = max(window, key=lambda f: abs(samples[f]))
            offset = found - frame
            if offset != delay:
                raise AssertionError(f'{name} ch{c}: echo of impulse at frame {frame} found at +{offset} frames '
                                     f'({offset * 1000 / rate:.4f} ms), expected +{delay} ({delay_ms:g} ms)')
            if abs(samples[found] - mix * AMPLITUDES[c]) > TOLERANCE:
                raise AssertionError(f'{name} ch{c}: echo amplitude {samples[found]:.7f}, expected {mix * AMPLITUDES[c]:.7f}')
            if abs(samples[frame] - (1 - mix) * AMPLITUDES[c]) > TOLERANCE:
                raise AssertionError(f'{name} ch{c}: dry amplitude {samples[frame]:.7f} at frame {frame}, '
                                     f'expected {(1 - mix) * AMPLITUDES[c]:.7f}')
            lines.append(f'PASS {name} ch{c} impulse frame {frame}: echo at frame {found} (+{offset} frames = '
                         f'{offset * 1000 / rate:g} ms) amplitude {samples[found]:.7f}, dry {samples[frame]:.7f}')
        residual, where = 0.0, None
        for f, value in enumerate(samples):
            error = abs(value - expected.get(f, (0.0, 0.0))[c])
            if error > residual:
                residual, where = error, f
        if residual > TOLERANCE:
            raise AssertionError(f'{name} ch{c}: unexpected signal {residual:.3g} at frame {where}')
        lines.append(f'PASS {name} ch{c} silence elsewhere, max residual {residual:.3g} over {frames} frames')
    return lines


def check_log(log: Path) -> list[str]:
    if not log.exists():
        raise AssertionError(f'host log missing: {log}')
    text = log.read_text(errors='replace')
    failures = [line for line in text.splitlines() if line.startswith('FAIL')]
    if failures:
        raise AssertionError('host check failed: ' + failures[0])
    if 'PASS REAPER delay discovery/edit/render/recall checks completed' not in text:
        raise AssertionError('host check did not complete')
    return [f'PASS host log: {sum(1 for l in text.splitlines() if l.startswith("PASS "))} host checks passed']


def check_directory(directory: Path) -> tuple[bool, list[str]]:
    lines: list[str] = []
    try:
        config = json.loads((directory / 'cases.json').read_text())
        lines += check_log(directory / 'delay-host.log')
    except (AssertionError, OSError, ValueError) as error:
        return False, lines + [f'FAIL {error}']
    ok = True
    for case in config['cases']:
        path = directory / f"{case['name']}.wav"
        try:
            rate, channels = read_wav(path)
            lines += check_case(case['name'], rate, channels, config['rate'], case['delay_ms'], case['mix'] / 100.0)
        except (AssertionError, OSError, ValueError) as error:
            ok = False
            lines.append(f'FAIL {error}')
    return ok, lines


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path, help='work directory written by delay.py')
    args = parser.parse_args(argv)
    ok, lines = check_directory(args.directory)
    print('\n'.join(lines))
    print('RESULT PASS' if ok else 'RESULT FAIL')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
