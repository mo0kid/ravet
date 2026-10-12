"""Hardware-free standalone popup behavior; no WebKit or audio devices."""
from pathlib import Path
import shutil
import subprocess
import unittest


class IoSettingsTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which("node"), "Node.js is needed for the DOM fake")
    def test_nominal_defaults_and_explicit_rate_retry(self):
        root = Path(__file__).resolve().parents[2]
        result = subprocess.run(
            [shutil.which("node"), str(root / "crates/ravet-webview/web/io-settings.test.js")],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
