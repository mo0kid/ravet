"""Generic VST3 command construction and rejection/validator propagation."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from generic_vst3_bundle import build_generic


class Runner:
    def __init__(self, target, *, fail_validator=False):
        self.target = target
        self.environment = {"PATH": "/usr/bin"}
        self.commands = []
        self.fail_validator = fail_validator

    def run(self, args, *, capture=False):
        args = [str(x) for x in args]
        self.commands.append(args)
        if args[:2] == ["cargo", "metadata"]:
            return json.dumps({"target_directory": str(self.target), "packages": [{
                "name": "ravet-delay-plugin", "targets": [{"name": "ravet_delay_plugin", "crate_types": ["cdylib", "rlib"]}]}]})
        if args[:2] == ["cmake", "--build"]:
            name = args[args.index("--target") + 1]
            (Path(args[2]) / "VST3/Release" / (name + ".vst3")).mkdir(parents=True)
        if self.fail_validator and Path(args[0]).name in ("validator", "validator.exe"):
            raise subprocess.CalledProcessError(1, args)
        return "" if capture else None


class GenericBundleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.sdk = self.root / "sdk"
        for name in ("LICENSE.txt", "CMakeLists.txt", "pluginterfaces/vst/ivstaudioprocessor.h"):
            p = self.sdk / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text("fixture")
        (self.root / "LICENSE").write_text("fixture")
        self.runner = Runner(self.root / "target")

    def run_bundle(self, **kw):
        return build_generic(self.root, self.sdk, "delay", runner=self.runner, **kw)

    def test_product_platforms(self):
        for platform, suffix in (("darwin", "libravet_delay_plugin.dylib"), ("linux", "libravet_delay_plugin.so"), ("win32", "ravet_delay_plugin.dll")):
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as target:
                self.runner = Runner(Path(target))
                artifacts = self.run_bundle(platform=platform)
                self.assertEqual(len(artifacts), 1)
                cargo = next(c for c in self.runner.commands if c[:2] == ["cargo", "build"])
                self.assertEqual(cargo, ["cargo", "build", "--locked", "-p", "ravet-delay-plugin", "--release"])
                configure = next(c for c in self.runner.commands if c[:2] == ["cmake", "-S"])
                self.assertTrue(any(c.endswith(suffix) for c in configure))
                self.assertFalse(any("--install" in c for c in self.runner.commands))
                if platform == "darwin":
                    signs = [c for c in self.runner.commands if c[0] == "codesign"]
                    self.assertEqual(len(signs), 2)
                    self.assertTrue(signs[0][-1].endswith(suffix))
                self.assertEqual(Path(self.runner.commands[-1][0]).name, "validator.exe" if platform == "win32" else "validator")

    def test_mock_skips_cargo_build_and_runs_contracts(self):
        self.run_bundle(mock=True, platform="linux")
        self.assertFalse(any(c[:2] == ["cargo", "build"] for c in self.runner.commands))
        self.assertTrue(any(c[0] == "ctest" for c in self.runner.commands))

    def test_external_manifest(self):
        manifest = self.root / "consumer/Cargo.toml"
        self.run_bundle(manifest_path=manifest, platform="linux")
        for command in self.runner.commands:
            if command[:2] in (["cargo", "metadata"], ["cargo", "build"]):
                self.assertIn(str(manifest.resolve()), command)

    def test_missing_sdk_before_build(self):
        (self.sdk / "CMakeLists.txt").unlink()
        with self.assertRaises(ValueError):
            self.run_bundle(platform="linux")
        self.assertEqual(self.runner.commands, [])

    def test_missing_product(self):
        with self.assertRaises(ValueError):
            self.run_bundle(package="missing", platform="linux")
        self.assertFalse(any(c[:2] == ["cargo", "build"] for c in self.runner.commands))

    def test_windows_cross_build(self):
        with mock.patch("shutil.which", return_value=None):
            artifacts = self.run_bundle(platform="linux", cross="windows")
        cargo = next(c for c in self.runner.commands if c[:3] == ["cargo", "xwin", "build"])
        self.assertIn("x86_64-pc-windows-msvc", cargo)
        configure = next(c for c in self.runner.commands if c[:2] == ["cmake", "-S"])
        library = self.root / "target/x86_64-pc-windows-msvc/release/ravet_delay_plugin.dll"
        self.assertIn(f"-DRAVET_PRODUCT_LIBRARY={library}", configure)
        self.assertIn("-DSMTG_USE_STATIC_CRT=ON", configure)
        self.assertTrue(any(c.startswith("-DCMAKE_TOOLCHAIN_FILE=") for c in configure))
        self.assertEqual(Path(configure[configure.index("-B") + 1]).name, "windows-release")
        build = next(c for c in self.runner.commands if c[:2] == ["cmake", "--build"])
        self.assertIn("moduleinfotool", build)
        self.assertFalse(any(Path(c[0]).name.startswith("validator") for c in self.runner.commands),
                         "the Windows validator runs on Windows")
        self.assertFalse(any(c[0] == "codesign" for c in self.runner.commands))
        self.assertEqual(len(artifacts), 1)

    def test_windows_cross_mock_contracts_need_an_emulator(self):
        with mock.patch("shutil.which", return_value=None):
            self.run_bundle(mock=True, platform="linux", cross="windows")
        self.assertFalse(any(c[0] == "ctest" for c in self.runner.commands))
        self.runner = Runner(self.root / "target2")
        with mock.patch("shutil.which", return_value="/usr/bin/wine"):
            self.run_bundle(mock=True, platform="linux", cross="windows")
        self.assertTrue(any(c[0] == "ctest" for c in self.runner.commands))

    def test_final_bundle_name_matches_its_native_module(self):
        artifacts = self.run_bundle(platform="linux", bundle_name="my-final-product", validate=False)
        self.assertEqual(artifacts[0].name, "my-final-product.vst3")
        configure = next(c for c in self.runner.commands if c[:2] == ["cmake", "-S"])
        self.assertIn("-DRAVET_VST3_NAME=my-final-product", configure)
        self.assertFalse(any(Path(c[0]).name == "validator" for c in self.runner.commands))

    def test_validator_failure_propagates(self):
        self.runner.fail_validator = True
        with self.assertRaises(subprocess.CalledProcessError):
            self.run_bundle(platform="linux")


if __name__ == "__main__":
    unittest.main()
