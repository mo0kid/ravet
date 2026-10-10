"""Boundary checks for local VST3 profile selection and validation failures."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("ravet_vst3_bundle", Path(__file__).resolve().parents[1] / "vst3_bundle.py")
bundle = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = bundle
spec.loader.exec_module(bundle)


class Runner:
    def __init__(self, target, fail=False):
        self.target, self.calls, self.fail = target, [], fail

    def run(self, arguments, *, capture=False):
        args = [str(item) for item in arguments]
        self.calls.append(args)
        if args[:2] == ["cargo", "metadata"]:
            return json.dumps({"target_directory": str(self.target)})
        if self.fail and Path(args[0]).name == "validator":
            raise subprocess.CalledProcessError(9, args)


class Vst3BundleTests(unittest.TestCase):
    def fixture(self, directory):
        root, sdk = Path(directory) / "root", Path(directory) / "sdk"
        for relative in ("LICENSE.txt", "CMakeLists.txt", "pluginterfaces/vst/ivstaudioprocessor.h"):
            path = sdk / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("external SDK fixture")
        root.mkdir()
        (root / "LICENSE").write_text("public licence")
        return root, sdk, root / "target"

    def test_gain_default_and_performance_keep_builds_and_loaded_artifacts_separate(self):
        with tempfile.TemporaryDirectory() as directory:
            root, sdk, target = self.fixture(directory)
            for name in ("performance", "gain"):
                runner = Runner(target)
                context = bundle.prepare_context(root, sdk, name, platform="darwin", runner=runner)
                built = bundle.build_reference(context)
                expected = target / "vst3" / ("performance/release" if name == "performance" else "release")
                self.assertEqual(built.build, expected)
                artifact = built.build / "VST3/Release" / (context.plugin.binary + ".vst3")
                artifact.mkdir(parents=True)
                # Unselected stale output must never be packaged or loaded.
                stale = built.build / "other.vst3"
                stale.mkdir()
                artifacts = bundle.package_reference(built)
                self.assertEqual(artifacts, [artifact])
                self.assertFalse((stale / "Contents").exists())
                self.assertEqual((artifact / "Contents/Resources/notices/RAVET-LICENSE.txt").read_text(), "public licence")
                bundle.validate_reference(built, artifacts)
                loaded = [args for args in runner.calls if Path(args[0]).name == "validator"]
                self.assertEqual(loaded[0][-1], str(artifact))
                ctest = next(args for args in runner.calls if args[0] == "ctest")
                self.assertEqual("-R" in ctest, name == "performance")
                configure = next(args for args in runner.calls if args[0] == "cmake" and "-S" in args)
                performance = next(arg for arg in configure if arg.startswith("-DRAVET_PERFORMANCE_LIBRARY="))
                self.assertEqual(performance.endswith("libravet_performance_native.a"), name == "performance")
                self.assertEqual(bundle.prepare_context(root, sdk, platform="darwin", runner=runner).plugin.name, "gain")

    def test_invalid_options_and_sdk_reject_before_commands(self):
        with tempfile.TemporaryDirectory() as directory:
            root, sdk, target = self.fixture(directory)
            runner = Runner(target)
            for kwargs in ({"plugin": "performance", "native_editor": True},
                           {"plugin": "performance", "factory_programs": True},
                           {"platform": "linux", "native_editor": True},
                           {"platform": "win32", "factory_programs": True},
                           {"plugin": "unknown"}, {"profile": "unknown"}):
                with self.assertRaises(ValueError):
                    bundle.prepare_context(root, sdk, runner=runner, **kwargs)
            (sdk / "LICENSE.txt").unlink()
            with self.assertRaises(ValueError):
                bundle.prepare_context(root, sdk, runner=runner)
            self.assertFalse(runner.calls)
            self.assertFalse(target.exists())

    def test_validator_failure_propagates_and_no_bundle_is_an_error(self):
        with tempfile.TemporaryDirectory() as directory:
            root, sdk, target = self.fixture(directory)
            runner = Runner(target, fail=True)
            context = bundle.prepare_context(root, sdk, "performance", platform="linux", runner=runner)
            built = bundle.build_reference(context)
            with self.assertRaises(RuntimeError):
                bundle.package_reference(built)
            artifact = built.build / (context.plugin.binary + ".vst3")
            artifact.mkdir(parents=True)
            artifacts = bundle.package_reference(built)
            with self.assertRaises(subprocess.CalledProcessError):
                bundle.validate_reference(built, artifacts)
            self.assertFalse(any(args[0] == "codesign" for args in runner.calls))


if __name__ == "__main__":
    unittest.main()
