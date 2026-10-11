"""Boundaries of local AU profile selection, SDK ownership and packaging."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
spec = importlib.util.spec_from_file_location("ravet_au_bundle", SCRIPTS / "au_bundle.py")
bundle = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = bundle
spec.loader.exec_module(bundle)


class Runner:
    def __init__(self):
        self.calls = []
        self.fail = False

    def run(self, arguments, *, capture=False):
        args = [str(argument) for argument in arguments]
        self.calls.append(args)
        if self.fail and "contracts" in Path(args[0]).name:
            raise subprocess.CalledProcessError(1, args)


class AuBundleTests(unittest.TestCase):
    def fixture(self, temporary, plugin="performance", **options):
        root = Path(temporary) / "root"
        vst3 = Path(temporary) / "vst3"
        audio = Path(temporary) / "audio"
        for base, names in ((vst3, ("LICENSE.txt", "CMakeLists.txt", "pluginterfaces/vst/ivstaudioprocessor.h")),
                            (audio, ("LICENSE.txt", "include/AudioUnitSDK/MusicDeviceBase.h"))):
            for relative in names:
                path = base / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(relative)
        (audio / "AudioUnitSDK.xcodeproj").mkdir()
        root.mkdir()
        (root / "LICENSE").write_text("public licence")
        runner = Runner()
        context = bundle.prepare_context(root, vst3, audio, plugin, selected_platform="darwin", runner=runner, **options)
        target = context.root / "target"
        build = target / "vst3" / ("release" if plugin == "gain" else "performance/release")
        nested = build / "VST3/Release" / (context.vst3.plugin.binary + ".vst3")
        nested.mkdir(parents=True)
        (nested / "public-fixture.txt").write_text(plugin)
        fake_vst3 = SimpleNamespace(target=target, build=build)
        return context, fake_vst3, nested

    def build(self, context, vst3, nested):
        with patch.object(bundle.vst3_bundle, "build_reference", return_value=vst3), \
             patch.object(bundle.vst3_bundle, "package_reference", return_value=[nested]), \
             patch.object(bundle.vst3_bundle, "validate_reference") as validate:
            reference = bundle.build_reference(context)
            validate.assert_called_once_with(vst3, [nested])
        return reference

    def test_performance_packages_its_own_nested_module_and_notices(self):
        with tempfile.TemporaryDirectory() as temporary:
            context, vst3, nested = self.fixture(temporary)
            reference = self.build(context, vst3, nested)
            self.assertEqual(reference.validators, ("ravet_au_performance_contracts", "ravet_au_native_parameters_contracts", "ravet_au_native_audio_contracts", "ravet_au_native_transport_contracts", "ravet_au_native_transport_fast_contracts", "ravet_au_native_midi_output_contracts", "ravet_au_native_midi_output_fast_contracts", "ravet_au_synthetic_midi_output_contracts", "ravet_au_native_audio64_contracts", "ravet_au_native_audio64_fast_contracts", "ravet_au_synthetic_audio64_contracts", "ravet_au_performance64_contracts", "ravet_au_performance_precision_contracts", "ravet_au_native_precision_schema_contracts", "ravet_au_native_precision_schema_fast_contracts"))
            artifact = bundle.package_reference(reference)
            bundle.validate_reference(reference)
            self.assertEqual(artifact, context.root / "target/au-performance/release/AU/ravet_performance_au.component")
            self.assertEqual((artifact / "Contents/Resources/plugin.vst3/public-fixture.txt").read_text(), "performance")
            self.assertEqual(len(list((artifact / "Contents/Resources/notices").iterdir())), 3)
            commands = context.vst3.runner.calls
            self.assertTrue(any(args[:2] == ["cmake", "-S"] and str(context.root / "examples/performance-au") in args for args in commands))
            self.assertEqual(Path(commands[-1][-1]), artifact)
            self.assertEqual(Path(commands[-2][-1]), artifact)
            # Repackaging replaces only the owned nested fixture, leaving other
            # component resources and the source VST3 bundle untouched.
            keep = artifact / "Contents/Resources/keep.txt"
            keep.write_text("retained")
            bundle.package_reference(reference)
            self.assertTrue(keep.is_file())
            self.assertTrue((nested / "public-fixture.txt").is_file())

    def test_gain_options_and_location_are_retained(self):
        with tempfile.TemporaryDirectory() as temporary:
            context, vst3, nested = self.fixture(temporary, "gain", native_editor=True, factory_programs=True)
            reference = self.build(context, vst3, nested)
            self.assertEqual(reference.component, vst3.build / "AU/ravet_gain_au.component")
            self.assertEqual(reference.validators, ("ravet_au_contracts", "ravet_au_editor_contracts", "ravet_au_program_contracts"))
            context.vst3.runner.fail = True
            with self.assertRaises(subprocess.CalledProcessError):
                bundle.validate_reference(reference)

    def test_invalid_platform_or_performance_gain_options_reject_before_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            context, _, _ = self.fixture(temporary)
            runner = context.vst3.runner
            with self.assertRaises(ValueError):
                bundle.prepare_context(context.root, context.vst3.sdk, context.sdk, selected_platform="linux", runner=runner)
            with self.assertRaises(ValueError):
                bundle.prepare_context(context.root, context.vst3.sdk, context.sdk, "performance", native_editor=True, selected_platform="darwin", runner=runner)
            self.assertFalse(runner.calls)

    def test_failed_nested_vst3_validation_stops_au_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            context, vst3, nested = self.fixture(temporary)
            with patch.object(bundle.vst3_bundle, "build_reference", return_value=vst3), \
                 patch.object(bundle.vst3_bundle, "package_reference", return_value=[nested]), \
                 patch.object(bundle.vst3_bundle, "validate_reference", side_effect=RuntimeError("module fault")):
                with self.assertRaises(RuntimeError):
                    bundle.build_reference(context)
            self.assertFalse(context.vst3.runner.calls)


if __name__ == "__main__":
    unittest.main()
