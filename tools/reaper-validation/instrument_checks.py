"""Exact sample checks for RAVET Evidence Synth's REAPER renders."""
from pathlib import Path
from delay_checks import read_wav, write_input, synced_delay_ms, LENGTH_SECONDS

CASES = ('pdc', 'zero', 'cc_off', 'echo')

def check_case(name, rate, channels):
    assert rate == 48000, f'{name}: rate {rate}'
    assert len(channels) == 2 and all(len(c) == 96000 for c in channels), f'{name}: shape'
    for channel, samples in enumerate(channels):
        for i, value in enumerate(samples):
            expected = 0.0
            for start, level in ((24000, .25), (48000, .25 if name == 'cc_off' else .25 * 64 / 127)):
                if start <= i < start + 64:
                    expected = level * (1 if (i-start) % 2 == 0 else -1)
            assert abs(value - expected) < 1e-5, f'{name} ch{channel} sample {i}: {value}, expected {expected}'
    return f'PASS {name}: exact onsets 24000/48000, CC levels, 64-sample one-shots, silence/tail (192000 samples)'

def check_directory(output: Path):
    lines = []
    log = output / 'delay-host.log'
    text = log.read_text() if log.exists() else ''
    ok = 'DONE PASS' in text and 'FAIL ' not in text
    if not ok: lines.append('FAIL host checks incomplete')
    for name in CASES:
        try:
            lines.append(check_case(name, *read_wav(output / (name + '.wav'))))
        except (OSError, ValueError, AssertionError) as error:
            ok = False; lines.append('FAIL ' + str(error))
    return ok, lines
