"""Generic AAX adapter build driver: SDK preflight, product selection and commands."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("ravet_aax_plugin", SCRIPTS / "aax_plugin.py")
plugin = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = plugin
spec.loader.exec_module(plugin)


class Runner:
    def __init__(self, target):
        self.target, self.calls = target, []

    def run(self, arguments, *, capture=False, capture_errors=False):
        args = [str(value) for value in arguments]
        self.calls.append(args)
        if capture:
            return json.dumps({"target_directory": str(self.target)})
        if capture_errors:
            return "note: Link against the following native artifacts\nnote: native-static-libs: -framework WebKit -lSystem -lc -lm\n"
        return None


class AaxPluginTests(unittest.TestCase):
    def fixture(self, temporary):
        root, aax = Path(temporary) / "root", Path(temporary) / "aax"
        (root).mkdir()
        (root / "LICENSE").write_text("licence")
        (root / "Cargo.toml").write_text('[workspace.package]\nversion = "0.1.0"\n')
        for relative in plugin.SDK_FILES:
            path = aax / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(relative)
        return root, aax, Runner(root / "target")

    def test_product_selection(self):
        self.assertEqual(plugin.product("delay").crate, "ravet-delay-plugin")
        self.assertIsNone(plugin.product("mock").crate)
        custom = plugin.product(None, package="ravet-chorus-plugin")
        self.assertEqual((custom.key, custom.identifier), ("chorus", "org.ravet.chorus.aax"))
        with self.assertRaises(ValueError):
            plugin.product("unknown")

    def test_incomplete_sdk_is_named(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, aax, runner = self.fixture(temporary)
            (aax / "Interfaces/AAX_Exports.cpp").unlink()
            with self.assertRaisesRegex(ValueError, "AAX_Exports.cpp"):
                plugin.build(root, aax, plugin.product("mock"), platform="darwin", runner=runner)

    def test_linux_is_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, aax, runner = self.fixture(temporary)
            with self.assertRaises(ValueError):
                plugin.build(root, aax, plugin.product("mock"), platform="linux", runner=runner)

    def test_product_commands_macos(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, aax, runner = self.fixture(temporary)
            built = plugin.build(root, aax, plugin.product("delay"), platform="darwin", runner=runner)
            rustc = runner.calls[1]
            self.assertEqual(rustc[:3], ["cargo", "rustc", "-p"])
            self.assertIn("staticlib", rustc)
            configure = runner.calls[2]
            self.assertIn(f"-DRAVET_PLUGIN_LIBRARY={root / 'target/release/libravet_delay_plugin.a'}", configure)
            self.assertIn("-DRAVET_PLUGIN_LINK_LIBRARIES=-framework WebKit -lSystem -lc -lm", configure)
            self.assertIn("-DRAVET_AAX_NAME=RAVET Delay", configure)
            self.assertIn("-DCMAKE_BUILD_TYPE=Release", configure)
            self.assertEqual(runner.calls[4][:2], ["ctest", "--test-dir"])
            self.assertEqual(built.artifact.name, "RAVET Delay.aaxplugin")
            notice = (built.artifact / "Contents/Resources/notices/AAX-SDK-NOTICE.txt").read_text()
            self.assertIn("not PACE-signed", notice)
            self.assertFalse(any(call[0] == "codesign" for call in runner.calls), "no signing step")

    def test_mock_commands_windows(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, aax, runner = self.fixture(temporary)
            plugin.build(root, aax, plugin.product("mock"), platform="win32", runner=runner)
            self.assertFalse(any(call[:2] == ["cargo", "rustc"] for call in runner.calls))
            configure = runner.calls[1]
            self.assertEqual(configure[-2:], ["-A", "x64"])
            self.assertFalse(any(item.startswith("-DRAVET_PLUGIN_LIBRARY") for item in configure))
            self.assertEqual(runner.calls[2][-4:], ["--config", "Release", "--parallel", "2"])

    def test_native_library_note(self):
        self.assertEqual(plugin.native_libraries("x\nnote: native-static-libs: kernel32.lib ntdll.lib\n"), "kernel32.lib ntdll.lib")
        self.assertEqual(plugin.native_libraries(""), "")


if __name__ == "__main__":
    unittest.main()
