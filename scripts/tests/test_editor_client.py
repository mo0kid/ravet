import shutil
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class EditorClient(unittest.TestCase):
    @unittest.skipUnless(shutil.which("node"), "Node.js is not installed")
    def test_injected_editor_client_speaks_the_protocol(self):
        result = subprocess.run(
            ["node", str(ROOT / "crates/ravet-plugin/tests/client.mjs")],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("protocol checks passed", result.stdout)


if __name__ == "__main__":
    unittest.main()
