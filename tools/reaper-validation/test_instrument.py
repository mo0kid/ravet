"""Synthetic regression tests for instrument render checks and disposable setup."""
import tempfile
import unittest
from pathlib import Path
import instrument
import instrument_checks as checks


def samples(name):
    channels = [[0.0] * 96000 for _ in range(2)]
    for channel in channels:
        for start, level in ((24000, .25), (48000, .25 if name == 'cc_off' else .25 * 64 / 127)):
            channel[start:start + 64] = [level * (1 if i % 2 == 0 else -1) for i in range(64)]
    return channels


class InstrumentChecks(unittest.TestCase):
    def test_exact_samples_and_failures(self):
        for name in checks.CASES:
            with self.subTest(name=name):
                audio = samples(name)
                self.assertIn('PASS', checks.check_case(name, 48000, audio))
                for frame, value in ((23999, .25), (48000, 0), (48064, .01)):
                    old = audio[0][frame]
                    audio[0][frame] = value
                    with self.assertRaises(AssertionError):
                        checks.check_case(name, 48000, audio)
                    audio[0][frame] = old

    def test_incomplete_host_log_never_passes(self):
        with tempfile.TemporaryDirectory() as directory:
            ok, lines = checks.check_directory(Path(directory))
            self.assertFalse(ok)
            self.assertIn('FAIL host checks incomplete', lines)

    def test_prepare_uses_only_new_work_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle = root / 'synth.clap'
            bundle.write_text('fixture')
            work = root / 'work'
            ini, script, _ = instrument.prepare(work, 'clap', bundle, 'CLAPi: RAVET Evidence Synth', True, [])
            self.assertIn(str(work / 'plugins' / 'clap'), ini.read_text())
            text = script.read_text()
            self.assertIn('RAVET_SYNTH = ', text)
            self.assertIn('GetResourcePath()', text)
            self.assertIn('TrackFX_SetPinMappings', text)
            self.assertIn(str(work / 'resource'), text)
            with self.assertRaises(FileExistsError):
                instrument.prepare(work, 'clap', bundle, '', True, [])


if __name__ == '__main__':
    unittest.main()
