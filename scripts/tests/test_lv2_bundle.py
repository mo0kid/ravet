"""Boundary checks for explicit LV2 profiles and local bundle ownership."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

MODULE = Path(__file__).resolve().parents[1] / "lv2_bundle.py"
spec = importlib.util.spec_from_file_location("ravet_lv2_bundle", MODULE)
import sys
bundle = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = bundle
spec.loader.exec_module(bundle)


class Runner:
    def __init__(self, target, fail_validation=False):
        self.target, self.calls, self.fail_validation = target, [], fail_validation

    def run(self, arguments, *, capture=False):
        args = [str(item) for item in arguments]
        self.calls.append(args)
        if args[:2] == ["cargo", "metadata"]:
            return json.dumps({"target_directory": str(self.target)})
        if self.fail_validation and "contracts" in Path(args[0]).name:
            raise subprocess.CalledProcessError(1, args)


class Lv2BundleTests(unittest.TestCase):
    def fixture(self, temporary):
        root = Path(temporary) / "root"
        sdk = Path(temporary) / "sdk"
        target = root / "target"
        for relative in ("include/lv2/core/lv2.h", "include/lv2/atom/atom.h", "include/lv2/state/state.h", "include/lv2/urid/urid.h", "include/lv2/midi/midi.h", "include/lv2/time/time.h", "COPYING"):
            path = sdk / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(relative)
        root.mkdir(parents=True)
        (root / "LICENSE").write_text("public licence")
        (root / "LICENSING.md").write_text("public policy")
        for profile in bundle.PROFILES.values():
            source = root / "examples" / profile.source
            source.mkdir(parents=True)
            for name in profile.metadata:
                (source / name).write_text(profile.name + name)
            binary = target / "lv2/release" / (profile.binary + ".so")
            binary.parent.mkdir(parents=True, exist_ok=True)
            binary.write_bytes(profile.name.encode())
        return root, sdk, target

    def test_both_profiles_share_pipeline_and_keep_artifacts_separate(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, sdk, target = self.fixture(temporary)
            for name in ("performance", "gain"):
                runner = Runner(target)
                context = bundle.prepare_context(root, sdk, name, platform="darwin", runner=runner)
                built = bundle.build_reference(context)
                artifact = bundle.package_reference(built)
                bundle.validate_reference(built, artifact)
                self.assertEqual(artifact, target / f"lv2/ravet_{name}.lv2")
                self.assertEqual((artifact / (context.profile.binary + ".so")).read_bytes(), name.encode())
                self.assertTrue((artifact / "LV2-SDK-LICENSE.txt").is_file())
                self.assertTrue((artifact / "RAVET-LICENSE.txt").is_file())
                self.assertTrue((artifact / "RAVET-LICENSING.md").is_file())
                configured = next(args for args in runner.calls if args[0] == "cmake" and "-S" in args)
                flag = next(arg for arg in configured if arg.startswith("-DRAVET_PERFORMANCE_LIBRARY="))
                self.assertEqual(flag.endswith("libravet_performance_native.a"), name == "performance")
                signed = next(args for args in runner.calls if args[0] == "codesign")
                self.assertEqual(Path(signed[-1]).parent, artifact)
                checked = [args for args in runner.calls if "contracts" in Path(args[0]).name]
                self.assertEqual(len(checked), len(context.profile.validators))
                self.assertTrue(all(Path(args[-1]).parent == artifact for args in checked))

    def test_configuration_failures_precede_commands_and_mutation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, sdk, target = self.fixture(temporary)
            runner = Runner(target)
            with self.assertRaises(ValueError):
                bundle.prepare_context(root, sdk, "gain", platform="win32", runner=runner)
            (sdk / "COPYING").unlink()
            with self.assertRaises(ValueError):
                bundle.prepare_context(root, sdk, "gain", platform="linux", runner=runner)
            self.assertFalse(runner.calls)
            self.assertFalse((target / "lv2/ravet_gain.lv2").exists())

    def test_linux_omits_signing_and_propagates_validator_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, sdk, target = self.fixture(temporary)
            runner = Runner(target, fail_validation=True)
            context = bundle.prepare_context(root, sdk, "performance", platform="linux", runner=runner)
            built = bundle.build_reference(context)
            artifact = bundle.package_reference(built)
            with self.assertRaises(subprocess.CalledProcessError):
                bundle.validate_reference(built, artifact)
            self.assertFalse(any(args[0] == "codesign" for args in runner.calls))


if __name__ == "__main__":
    unittest.main()
