"""Fixtures for the RAVET Delay REAPER harness, independent of an actual REAPER run."""
from pathlib import Path
import json
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import delay  # noqa: E402
import delay_checks  # noqa: E402

RATE = 48_000


def render(delay_frames, mix, rate=RATE, split=False, extra=None):
    """Ideal delay output for the synthetic source, as two float channels."""
    frames = round(delay_checks.LENGTH_SECONDS * rate)
    channels = [[0.0] * frames for _ in range(2)]
    for frame in delay_checks.impulse_frames(rate):
        for c, amplitude in enumerate(delay_checks.AMPLITUDES):
            channels[c][frame] += (1 - mix) * amplitude
            if split:  # a fractional delay interpolates the echo over two samples
                channels[c][frame + delay_frames] += mix * amplitude * 0.5
                channels[c][frame + delay_frames + 1] += mix * amplitude * 0.5
            else:
                channels[c][frame + delay_frames] += mix * amplitude
    if extra is not None:
        channels[0][extra] += 0.01
    return channels


def write(path, channels, rate=RATE, bits=24, float_format=False):
    frames = len(channels[0])
    data = bytearray()
    for f in range(frames):
        for c in range(2):
            value = channels[c][f]
            if float_format:
                data += struct.pack('<f', value)
            else:
                data += round(value * 2 ** (bits - 1)).to_bytes(bits // 8, 'little', signed=True)
    delay_checks.write_wav(path, rate, 2, bits, bytes(data), float_format)


class DelayChecks(unittest.TestCase):
    def test_source_holds_exact_stereo_impulses(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'input.wav'
            delay_checks.write_input(path, RATE)
            rate, channels = delay_checks.read_wav(path)
            self.assertEqual(rate, RATE)
            self.assertEqual(len(channels[0]), 4 * RATE)
            for c, amplitude in enumerate(delay_checks.AMPLITUDES):
                nonzero = [(f, v) for f, v in enumerate(channels[c]) if v]
                self.assertEqual(nonzero, [(24000, amplitude), (96000, amplitude)])

    def test_expected_delays_are_whole_frames(self):
        self.assertEqual(delay_checks.delay_frames(100.0, RATE), 4800)
        self.assertEqual(delay_checks.synced_delay_ms(0.5, 150.0), 200.0)
        self.assertEqual(delay_checks.delay_frames(200.0, RATE), 9600)
        self.assertEqual(delay_checks.delay_frames(100.0, 44_100), 4410)
        with self.assertRaises(ValueError):
            delay_checks.delay_frames(0.01, 44_100)

    def test_ideal_renders_pass_in_pcm_and_float(self):
        with tempfile.TemporaryDirectory() as temporary:
            for bits, float_format in ((16, False), (24, False), (32, False), (32, True)):
                path = Path(temporary) / f'r{bits}{float_format}.wav'
                write(path, render(9600, 0.5), bits=bits, float_format=float_format)
                rate, channels = delay_checks.read_wav(path)
                lines = delay_checks.check_case('synced', rate, channels, RATE, 200.0, 0.5)
                self.assertIn('echo at frame 33600 (+9600 frames = 200 ms)', lines[0])

    def test_misplaced_interpolated_or_unexpected_audio_fails(self):
        cases = [
            (render(4801, 1.0), r'found at \+4801 frames'),
            (render(4800, 1.0, split=True), 'echo amplitude'),
            (render(4800, 0.5), 'echo amplitude'),
            (render(4800, 1.0, extra=150000), 'unexpected signal'),
        ]
        for channels, message in cases:
            with self.assertRaisesRegex(AssertionError, message):
                delay_checks.check_case('fixed', RATE, channels, RATE, 100.0, 1.0)
        with self.assertRaisesRegex(AssertionError, 'render rate'):
            delay_checks.check_case('fixed', 44_100, render(4800, 1.0), RATE, 100.0, 1.0)

    def test_directory_requires_complete_host_log_and_every_render(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config = {'rate': RATE, 'cases': [c for c in delay.cases()]}
            (root / 'cases.json').write_text(json.dumps(config))
            write(root / 'fixed.wav', render(4800, 1.0))
            write(root / 'recalled.wav', render(4800, 1.0))
            write(root / 'synced.wav', render(9600, 0.5))
            log = root / 'delay-host.log'
            log.write_text('PASS a\nPASS REAPER delay discovery/edit/render/recall checks completed\nDONE ok\n')
            ok, lines = delay_checks.check_directory(root)
            self.assertTrue(ok, lines)
            command = [sys.executable, str(HERE / 'delay_checks.py'), str(root)]
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout)
            self.assertIn('RESULT PASS', result.stdout)
            write(root / 'recalled.wav', render(17999, 1.0))
            ok, lines = delay_checks.check_directory(root)
            self.assertFalse(ok)
            self.assertTrue(any(line.startswith('FAIL recalled') for line in lines), lines)
            log.write_text('PASS a\nFAIL check failed: host edit\nDONE failed\n')
            ok, lines = delay_checks.check_directory(root)
            self.assertFalse(ok)
            self.assertIn('host check failed', lines[-1])
            log.unlink()
            self.assertFalse(delay_checks.check_directory(root)[0])

    def test_prepare_writes_disposable_configuration_only_in_work_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / 'RAVET Delay.clap'
            (bundle / 'Contents').mkdir(parents=True)
            ini, script, env = delay.prepare(root / 'work', 'clap', bundle, 'CLAP: RAVET Delay', True, ['extra=1'])
            text = ini.read_text()
            self.assertTrue(text.startswith('[REAPER]\n'))
            self.assertIn('vst_scan=2', text)
            self.assertIn('extra=1', text)
            for key in delay.FORMATS['clap']['keys'].get(delay.platform_key(), []):
                self.assertIn(f'{key}={root / "work" / "plugins" / "clap"}', text)
            self.assertTrue((root / 'work' / 'plugins' / 'clap' / 'RAVET Delay.clap' / 'Contents').is_dir())
            self.assertNotIn('CLAP_PATH', set(env) - set(__import__('os').environ))
            self.assertTrue(script.read_text().startswith('RAVET_DELAY = {root = '))
            self.assertTrue((script.parent / 'input.wav').exists())
            luac = shutil.which('luac') or shutil.which('luac5.4')
            if luac:
                result = subprocess.run([luac, '-p', str(script)], capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_ini_sections_and_version_check(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / 'ravet_delay.lv2'
            bundle.mkdir()
            ini, _, _ = delay.prepare(root / 'work', 'lv2', bundle, 'LV2: RAVET Delay', True,
                                      ['key=1', '[audioconfig]mode=3', '[other]a=b'])
            text = ini.read_text()
            reaper, _, rest = text.partition('[audioconfig]\n')
            self.assertIn('verchk=0\n', reaper)
            self.assertIn('key=1\n', reaper)
            self.assertEqual(rest, 'mode=3\n[other]\na=b\n')

    def test_windows_preparation_rewrites_paths_and_writes_runner(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / 'RAVET Delay.clap'
            bundle.write_bytes(b'dll')
            work = root / 'clap-1'
            code = delay.main(['--format', 'clap', '--bundle', str(bundle), '--work', str(work),
                               '--windows-root', r'C:\Users\tester\runs', '--timeout', '99'])
            self.assertEqual(code, 0)
            text = (work / 'resource' / 'reaper.ini').read_text()
            self.assertIn('clap_path_win64=C:\\Users\\tester\\runs\\clap-1\\plugins\\clap\n', text)
            self.assertIn('vstpath64=C:\\Users\\tester\\runs\\clap-1\\plugins\\empty\n', text)
            self.assertIn('[audioconfig]\nmode=2\n', text)
            self.assertNotIn(str(root), text)
            script = (work / 'output' / 'ravet-delay-check.lua').read_text()
            self.assertTrue(script.startswith(
                'RAVET_DELAY = {root = [==[C:\\Users\\tester\\runs\\clap-1\\output]==], '
                'resource = [==[C:\\Users\\tester\\runs\\clap-1\\resource]==]'), script[:200])
            runner = (work / 'run-windows.ps1').read_bytes()
            self.assertTrue(runner.startswith(b'\xef\xbb\xbf'))
            self.assertNotIn(b'\n', runner.replace(b'\r\n', b''))
            runner = runner.decode('utf-8-sig')
            self.assertIn('[double]$Timeout = 99', runner)
            self.assertIn("$reaper = 'C:\\REAPER\\reaper.exe'", runner)
            self.assertIn('-cfgfile "C:\\Users\\tester\\runs\\clap-1\\resource\\reaper.ini"', runner)
            self.assertNotIn('__', runner)
            self.assertTrue((work / 'plugins' / 'clap' / 'RAVET Delay.clap').exists())

    def test_check_only_compares_windows_guard_snapshots(self):
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            (work / 'output').mkdir()
            (work / 'run-windows.ps1').write_text('')
            self.assertFalse(delay.compare_snapshots(work)[0])
            line = 'C:\\Users\\t\\AppData\\Roaming\\REAPER\\REAPER.ini\t10\t5\tABC'
            (work / 'guard-before.tsv').write_text('\ufeff' + line + '\r\n', encoding='utf-8')
            (work / 'guard-after.tsv').write_text('\ufeff' + line + '\r\n', encoding='utf-8')
            ok, lines = delay.compare_snapshots(work)
            self.assertTrue(ok, lines)
            self.assertIn('1 files', lines[0])
            (work / 'guard-after.tsv').write_text(line.replace('ABC', 'DEF') + '\n', encoding='utf-8')
            ok, lines = delay.compare_snapshots(work)
            self.assertFalse(ok)
            self.assertIn('REAPER.ini', lines[0])
            # Renders are missing, so the whole check fails regardless.
            self.assertEqual(delay.main(['--check-only', str(work)]), 1)

    def test_script_aborts_without_host_changes_in_a_foreign_resource_directory(self):
        lua = shutil.which('lua') or shutil.which('lua5.4')
        if lua is None:
            self.skipTest('optional Lua interpreter is not installed')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / 'RAVET Delay.lv2'
            bundle.mkdir()
            _, script, _ = delay.prepare(root / 'work', 'lv2', bundle, 'LV2: RAVET Delay', True, [])
            stub = root / 'stub.lua'
            stub.write_text("""reaper=setmetatable({
 GetResourcePath=function() return '/home/user/.config/REAPER' end,
 GetOS=function() return 'Other' end,
},{__index=function(_, name) error('host call reached: ' .. name) end})
dofile(arg[1])
""")
            result = subprocess.run([lua, str(stub), str(script)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            log = (script.parent / 'delay-host.log').read_text()
            self.assertTrue(log.startswith('ABORT resource directory is /home/user/.config/REAPER'), log)


if __name__ == '__main__':
    unittest.main()
