"""Selection, SDK discovery and command sequence of the generic LV2 bundler."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
spec = importlib.util.spec_from_file_location("ravet_lv2_generic", SCRIPTS / "lv2_generic.py")
generic = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = generic
spec.loader.exec_module(generic)


class Runner:
    def __init__(self, target, fail=None):
        self.target, self.calls, self.fail = target, [], fail
        self.environment = {"PATH": "/usr/bin"}

    def run(self, arguments, *, capture=False):
        args = [str(item) for item in arguments]
        self.calls.append(args)
        if args[:2] == ["cargo", "metadata"]:
            return json.dumps({"target_directory": str(self.target)})
        if self.fail and args[0] == self.fail:
            raise subprocess.CalledProcessError(3, args)
        return ""


class GenericLv2Tests(unittest.TestCase):
    def fixture(self, temporary, flat=False):
        root, sdk = Path(temporary) / "root", Path(temporary) / "sdk"
        include = sdk if flat else sdk / "include"
        for header in generic.HEADERS:
            path = include / "lv2" / header
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(header)
        (sdk / "COPYING").write_text("lv2 licence")
        root.mkdir(parents=True)
        (root / "LICENSE").write_text("licence")
        (root / "LICENSING.md").write_text("policy")
        return root, sdk, root / "target"

    def built_bundle(self, build, name):
        bundle = build / "bundles" / f"{name}.lv2"
        bundle.mkdir(parents=True)
        for file in ("manifest.ttl", "plugin.ttl", "ravet_lv2.so", "ravet_lv2_ui.so", "ravet_product.so"):
            (bundle / file).write_text(file)

    def test_delay_builds_crate_configures_tests_and_packages(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, sdk, target = self.fixture(temporary)
            runner = Runner(target)
            context = generic.prepare_generic(root, sdk, "delay", platform="linux", runner=runner)
            built_target, build = generic.build_generic(context)
            self.assertEqual(build, target / "lv2/generic-ravet_delay")
            self.assertIn(["cargo", "build", "--release", "--locked", "-p", "ravet-delay-plugin"], runner.calls)
            configure = next(args for args in runner.calls if args[0] == "cmake" and "-S" in args)
            self.assertTrue(configure[2].endswith("crates/ravet-plugin/lv2"))
            self.assertIn(f"-DRAVET_PLUGIN_LIBRARY={target / 'release/libravet_delay_plugin.so'}", configure)
            self.assertIn("-DRAVET_LV2_BUNDLE_NAME=ravet_delay", configure)
            self.assertEqual(runner.calls[-1][0], "ctest")
            self.built_bundle(build, "ravet_delay")
            artifact = generic.package_generic(context, built_target, build)
            self.assertEqual(artifact, target / "lv2/ravet_delay.lv2")
            for file in ("plugin.ttl", "ravet_product.so", "LV2-SDK-LICENSE.txt", "RAVET-LICENSE.txt", "RAVET-LICENSING.md"):
                self.assertTrue((artifact / file).is_file(), file)
            self.assertFalse(any(args[0] == "codesign" for args in runner.calls))

    def test_mock_skips_cargo_build_and_macos_signs_every_binary(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, sdk, target = self.fixture(temporary, flat=True)
            runner = Runner(target)
            context = generic.prepare_generic(root, sdk, "mock", platform="darwin", runner=runner)
            built_target, build = generic.build_generic(context)
            self.assertFalse(any(args[:2] == ["cargo", "build"] for args in runner.calls))
            bundle = build / "bundles/ravet_lv2_mock.lv2"
            bundle.mkdir(parents=True)
            for file in ("manifest.ttl", "plugin.ttl", "ravet_lv2.dylib", "ravet_lv2_ui.dylib", "ravet_product.dylib"):
                (bundle / file).write_text(file)
            artifact = generic.package_generic(context, built_target, build)
            signed = [Path(args[-1]).name for args in runner.calls if args[0] == "codesign"]
            self.assertEqual(signed, ["ravet_lv2.dylib", "ravet_lv2_ui.dylib", "ravet_product.dylib"])
            self.assertEqual(artifact, target / "lv2/ravet_mock.lv2")

    def test_crate_option_and_windows_library_name(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, sdk, target = self.fixture(temporary)
            runner = Runner(target)
            context = generic.prepare_generic(root, sdk, crate="my-product", platform="win32", runner=runner)
            generic.build_generic(context)
            configure = next(args for args in runner.calls if args[0] == "cmake" and "-S" in args)
            self.assertIn(f"-DRAVET_PLUGIN_LIBRARY={target / 'release/my_product.dll'}", configure)

    def test_windows_cross_build(self):
        with tempfile.TemporaryDirectory() as temporary, mock.patch("shutil.which", return_value="/usr/bin/wine"):
            root, sdk, target = self.fixture(temporary)
            runner = Runner(target)
            context = generic.prepare_generic(root, sdk, "delay", platform="linux", runner=runner, cross="windows")
            built_target, build = generic.build_generic(context)
            self.assertEqual(build, target / "lv2/windows/generic-ravet_delay")
            self.assertIn(["cargo", "xwin", "build", "--target", "x86_64-pc-windows-msvc", "--release", "--locked",
                           "-p", "ravet-delay-plugin"], runner.calls)
            configure = next(args for args in runner.calls if args[0] == "cmake" and "-S" in args)
            library = target / "x86_64-pc-windows-msvc/release/ravet_delay_plugin.dll"
            self.assertIn(f"-DRAVET_PLUGIN_LIBRARY={library}", configure)
            self.assertIn("-DCMAKE_CROSSCOMPILING_EMULATOR=/usr/bin/wine", configure)
            self.assertEqual(runner.calls[-1][0], "ctest")
            self.built_bundle(build, "ravet_delay")
            artifact = generic.package_generic(context, built_target, build)
            self.assertEqual(artifact, target / "lv2/windows/ravet_delay.lv2")

    def test_windows_cross_needs_wine(self):
        with tempfile.TemporaryDirectory() as temporary, mock.patch("shutil.which", return_value=None):
            root, sdk, target = self.fixture(temporary)
            with self.assertRaisesRegex(ValueError, "Wine"):
                generic.prepare_generic(root, sdk, "delay", platform="linux", runner=Runner(target), cross="windows")

    def test_failures_precede_commands(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, sdk, target = self.fixture(temporary)
            runner = Runner(target)
            with self.assertRaises(ValueError):
                generic.prepare_generic(root, sdk, "unknown", platform="linux", runner=runner)
            with self.assertRaises(ValueError):
                generic.prepare_generic(root, sdk, "delay", platform="freebsd", runner=runner)
            (sdk / "include/lv2/ui/ui.h").unlink()
            with self.assertRaises(ValueError):
                generic.prepare_generic(root, sdk, "delay", platform="linux", runner=runner)
            self.assertFalse(runner.calls)

    def test_test_failure_stops_before_packaging(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, sdk, target = self.fixture(temporary)
            runner = Runner(target, fail="ctest")
            context = generic.prepare_generic(root, sdk, "generic", platform="linux", runner=runner)
            with self.assertRaises(subprocess.CalledProcessError):
                generic.build_generic(context)
            self.assertFalse((target / "lv2/ravet_generic.lv2").exists())


if __name__ == "__main__":
    unittest.main()
