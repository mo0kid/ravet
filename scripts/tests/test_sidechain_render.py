import importlib.util
from pathlib import Path
import struct
import tempfile
import unittest
import wave

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("sidechain_checker", ROOT / "tools/reaper-validation/check-sidechain-render.py")
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class SidechainRendering(unittest.TestCase):
    def fixture(self, path, opened):
        with wave.open(str(path), "wb") as output:
            output.setparams((2, 2, 8000, 16000, "NONE", "not compressed"))
            output.writeframes(b"".join(
                struct.pack("<hh", 16384, -8192) if opened(frame) else b"\0\0\0\0"
                for frame in range(16000)
            ))

    def test_expected_render_and_wrong_detector_phase(self):
        with tempfile.TemporaryDirectory(prefix="ravet-render-fixture-") as folder:
            path = Path(folder) / "render.wav"
            self.fixture(path, lambda frame: (frame // 4000) % 2 == 1)
            CHECKER.check(path)
            self.fixture(path, lambda frame: True)
            with self.assertRaisesRegex(ValueError, "phase 1"):
                CHECKER.check(path)

    def test_truncated_duration_rejects_before_sample_regions(self):
        with tempfile.TemporaryDirectory(prefix="ravet-render-fixture-") as folder:
            path = Path(folder) / "short.wav"
            with wave.open(str(path), "wb") as output:
                output.setparams((2, 2, 8000, 1, "NONE", "not compressed"))
                output.writeframes(b"\0\0\0\0")
            with self.assertRaisesRegex(ValueError, "shape"):
                CHECKER.check(path)
