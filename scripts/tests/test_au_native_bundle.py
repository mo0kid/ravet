"""Product-derived AU identities and dependency-free native command routing."""
import importlib.util
from pathlib import Path
import plistlib
import subprocess
import sys
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("au_native_bundle", SCRIPTS / "au_native_bundle.py")
bundle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bundle)


class NativeAuBundleTests(unittest.TestCase):
    def test_metadata_roundtrip_uses_table_identity_and_version(self):
        product = dict(name="A & B Delay", vendor="A Test Vendor", version="1.2.3",
                       plugin_id="org.example.delay", au_type=0x61756678,
                       au_subtype=0x54455354, au_manufacturer=0x41624364)
        plist = plistlib.loads(plistlib.dumps(bundle.component_plist(product)))
        self.assertEqual(plist["CFBundleIdentifier"], "org.example.delay.auv2")
        component = plist["AudioComponents"][0]
        self.assertEqual(component["name"], "A Test Vendor: A & B Delay")
        self.assertEqual((component["type"], component["subtype"], component["manufacturer"]), ("aufx", "TEST", "AbCd"))
        self.assertEqual(component["version"], 0x010203)
        self.assertEqual(component["factoryFunction"], "RavetAUFactory")
        self.assertFalse(component["sandboxSafe"])

    def test_invalid_fourcc_and_unrepresentable_version_reject(self):
        for value in (0, 0xFFFFFFFF, 0x7F202020):
            with self.assertRaises((ValueError, UnicodeDecodeError)):
                bundle.fourcc(value)
        product = dict(name="Test", vendor="Test", version="1.256.0", plugin_id="org.example.test",
                       au_type=0x61756D75, au_subtype=0x54455354, au_manufacturer=0x41624364)
        with self.assertRaises(ValueError):
            bundle.component_plist(product)

    def test_native_help_requires_no_vst3_sdk_or_product_allowlist(self):
        result = subprocess.run([sys.executable, str(SCRIPTS / "bundle-au.py"), "--help"], capture_output=True, text=True, check=True)
        self.assertIn("--manifest-path", result.stdout)
        self.assertIn("--product-library", result.stdout)
        self.assertIn("--mock", result.stdout)
        self.assertNotIn("--plugin {gain,performance}", result.stdout)


if __name__ == "__main__":
    unittest.main()
