"""Local AAX SDK preflight, packaging and required actual consumer gates."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
spec = importlib.util.spec_from_file_location("ravet_aax_bundle", SCRIPTS / "aax_bundle.py")
bundle = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = bundle
spec.loader.exec_module(bundle)


class Runner:
    def __init__(self, target):
        self.target, self.calls, self.fail = target.resolve(), [], None

    def run(self, arguments, *, capture=False):
        args = [str(value) for value in arguments]
        self.calls.append(args)
        if self.fail and (self.fail in Path(args[0]).name or (self.fail == "codesign-verify" and args[:2] == ["codesign", "--verify"])):
            raise subprocess.CalledProcessError(1, args)
        return json.dumps({"target_directory": str(self.target)}) if capture else None


class AaxBundleTests(unittest.TestCase):
    def fixture(self, temporary, profile="native", plugin="gain", editor=False, internal_precision=32):
        root, sdk, aax = [Path(temporary) / name for name in ("root", "vst3", "aax")]
        for base, relatives in ((root, ("LICENSE",)), (sdk, ("LICENSE.txt", "CMakeLists.txt", "pluginterfaces/vst/ivstaudioprocessor.h")), (aax, ("Interfaces/AAX.h", "Interfaces/AAX_IParameter.h", "Interfaces/AAX_CParameterManager.h"))):
            for relative in relatives:
                path = base / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(relative)
        runner = Runner(root / "target")
        return bundle.prepare_context(root, sdk, aax, selected_platform="darwin", runner=runner, profile=profile, plugin=plugin, editor=editor, internal_precision=internal_precision)

    def test_meter_reference32_64_owns_exact_profile_and_mandatory_consumer_gates(self):
        for precision in (32, 64):
            with self.subTest(precision=precision), tempfile.TemporaryDirectory() as temporary:
                context = self.fixture(temporary, plugin="gain-meters", internal_precision=precision)
                reference = bundle.build_reference(context)
                directory = "gain-meters" if precision == 32 else "gain-meters64"
                self.assertEqual(reference.build, context.root / f"target/aax/{directory}/release")
                self.assertEqual(reference.artifact.name, "ravet_gain_meters_aax.aaxplugin")
                self.assertEqual(context.runner.calls[0], ["cargo", "build", "-p", "ravet-gain-native", "-p", "ravet-bus-routing-native", "-p", "ravet-performance-native", "--release", "--locked"])
                self.assertIn(str(context.root / "examples/gain-meters-aax"), context.runner.calls[2])
                self.assertTrue(all(name in context.runner.calls[3] for name in bundle.METER_CONTRACTS))
                self.assertEqual(context.runner.calls[3][-2:], ["--parallel", "1"])
                self.assertFalse(any(name in context.runner.calls[3] for name in bundle.PROFILES["native"]))
                bundle.package_reference(reference)
                bundle.validate_reference(reference)
                self.assertEqual(context.runner.calls[-2][-2:], ["--plugin", "gain-meters"])
                self.assertEqual(Path(context.runner.calls[-1][0]).name, "ravet_aax_meters_registered_contracts")
                context.runner.fail = "ravet_aax_meters_registered_contracts"
                with self.assertRaises(subprocess.CalledProcessError):
                    bundle.validate_reference(reference)
        with tempfile.TemporaryDirectory() as temporary:
            context = self.fixture(temporary)
            for profile, editor in (("stock", False), ("native", True)):
                with self.assertRaises(ValueError):
                    bundle.prepare_context(context.root, context.sdk, context.aax, plugin="gain-meters", profile=profile, editor=editor, selected_platform="darwin", runner=context.runner)
                self.assertEqual(context.runner.calls, [])

    def test_complete_pipeline_retains_location_and_actual_registered_gate(self):
        with tempfile.TemporaryDirectory() as temporary:
            context = self.fixture(temporary)
            reference = bundle.build_reference(context)
            expected = context.root / "target/aax/release/AAXPLUGIN/Release/ravet_gain_aax.aaxplugin"
            self.assertEqual(reference.artifact, expected)
            module = expected / "Contents/MacOS/ravet_gain_aax"
            module.parent.mkdir(parents=True)
            module.write_text("public actual module")
            keep = expected / "Contents/Resources/keep.txt"
            keep.parent.mkdir(parents=True)
            keep.write_text("retained")
            self.assertEqual(bundle.package_reference(reference), expected)
            bundle.validate_reference(reference)
            self.assertEqual(module.read_text(), "public actual module")
            self.assertTrue(keep.is_file())
            self.assertEqual(len(list((expected / "Contents/Resources/notices").iterdir())), 3)
            commands = context.runner.calls
            self.assertEqual(commands[0], ["cargo", "build", "-p", "ravet-gain-native", "-p", "ravet-bus-routing-native", "--release", "--locked"])
            self.assertTrue(all(name in commands[3] for name in bundle.CONTRACTS))
            self.assertTrue(all(name in commands[3] for name in ("ravet_aax_transport", "ravet_aax_transport_fast", "ravet_aax_transport_model", "ravet_aax_transport_model_fast", "ravet_aax_audio", "ravet_aax_audio_fast", "ravet_aax_multibus", "ravet_aax_multibus_fast", "ravet_aax_midi_output", "ravet_aax_midi_output_fast", "ravet_aax_generated_model", "ravet_aax_generated_model_fast", "ravet_aax_timing_model", "ravet_aax_timing_model_fast")))
            self.assertEqual(commands[3][-2:], ["--parallel", "2"])
            self.assertEqual(Path(commands[-1][0]).name, "ravet_aax_native_registered_contracts")
            self.assertEqual(Path(commands[-1][1]), module)

    def test_explicit_internal64_native_paths_and_contracts_preserve_host_float32_modules(self):
        with tempfile.TemporaryDirectory() as temporary:
            for plugin, editor, directory in (("gain", False, "native64"), ("gain", True, "editor64"), ("performance", False, "performance64")):
                context = self.fixture(temporary, plugin=plugin, editor=editor, internal_precision=64)
                reference = bundle.build_reference(context)
                self.assertEqual(reference.build, context.root / f"target/aax/{directory}/release")
                self.assertEqual(reference.artifact.name, f"ravet_{plugin}_aax.aaxplugin")
                self.assertIn("-DRAVET_AAX_INTERNAL_PRECISION=64", context.runner.calls[2])
                expected = bundle.PRECISION_CONTRACTS if plugin == "gain" else bundle.PERFORMANCE_CONTRACTS
                self.assertTrue(all(name in context.runner.calls[3] for name in expected))
                if editor:
                    self.assertIn("ravet-gain-native/native-editor", context.runner.calls[0])
                bundle.validate_reference(reference)
                self.assertEqual(Path(context.runner.calls[-1][0]).name,
                                 "ravet_aax_editor_registered_contracts" if editor else
                                 "ravet_aax_native_registered_contracts" if plugin == "gain" else "ravet_aax_instrument_registered_contracts")

    def test_precision_rejects_bad_type_value_or_stock64_before_commands(self):
        with tempfile.TemporaryDirectory() as temporary:
            context = self.fixture(temporary)
            for precision, profile in ((0, "native"), (128, "native"), (32.0, "native"), (True, "native"), ("64", "native"), (64, "stock")):
                with self.assertRaises(ValueError):
                    bundle.prepare_context(context.root, context.sdk, context.aax, selected_platform="darwin",
                                           runner=context.runner, profile=profile, internal_precision=precision)
                self.assertEqual(context.runner.calls, [])
            self.assertEqual(context.internal_precision, 32)
            reference = bundle.build_reference(context)
            self.assertIn("-DRAVET_AAX_INTERNAL_PRECISION=32", context.runner.calls[2])
            self.assertFalse(any(name in context.runner.calls[3] for name in bundle.PRECISION_CONTRACTS))
            self.assertEqual(reference.build, context.root / "target/aax/release")

    def test_stock_profile_keeps_separate_artifact_and_actual_stock_regression(self):
        with tempfile.TemporaryDirectory() as temporary:
            context = self.fixture(temporary, "stock")
            reference = bundle.build_reference(context)
            self.assertEqual(reference.artifact, context.root / "target/aax/stock/release/AAXPLUGIN/Release/ravet_gain_aax.aaxplugin")
            self.assertIn("-DRAVET_AAX_REFERENCE_PROFILE=stock", context.runner.calls[2])
            self.assertNotIn("ravet-bus-routing-native", context.runner.calls[0])
            self.assertFalse(any("RAVET_BUS_ROUTING_LIBRARY" in value for value in context.runner.calls[2]))
            self.assertTrue(all(name in context.runner.calls[3] for name in bundle.PROFILES["stock"]))
            self.assertFalse(any(name in context.runner.calls[3] for name in ("ravet_aax_processor", "ravet_aax_model", "ravet_aax_transport", "ravet_aax_transport_model", "ravet_aax_audio", "ravet_aax_multibus", "ravet_aax_midi_output", "ravet_aax_generated_model", "ravet_aax_timing_model")))
            bundle.validate_reference(reference)
            self.assertEqual(Path(context.runner.calls[-1][0]).name, "ravet_aax_registered_contracts")

    def test_performance_native_pipeline_uses_its_archive_source_bundle_and_midi_gate(self):
        with tempfile.TemporaryDirectory() as temporary:
            context = self.fixture(temporary, plugin="performance")
            reference = bundle.build_reference(context)
            expected = context.root / "target/aax/performance/release/AAXPLUGIN/Release/ravet_performance_aax.aaxplugin"
            self.assertEqual(reference.artifact, expected)
            self.assertEqual(context.runner.calls[0], ["cargo", "build", "-p", "ravet-performance-native", "--release", "--locked"])
            self.assertIn(str(context.root / "examples/performance-aax"), context.runner.calls[2])
            self.assertTrue(all(name in context.runner.calls[3] for name in bundle.PERFORMANCE_CONTRACTS))
            self.assertFalse(any(name in context.runner.calls[3] for name in bundle.CONTRACTS))
            bundle.package_reference(reference)
            bundle.validate_reference(reference)
            self.assertEqual(context.runner.calls[-2][-2:], ["--plugin", "performance"])
            self.assertEqual(Path(context.runner.calls[-1][0]).name, "ravet_aax_instrument_registered_contracts")
            self.assertEqual(Path(context.runner.calls[-1][1]), expected / "Contents/MacOS/ravet_performance_aax")

    def test_midi_precision_pipeline_shares_archive_but_owns_factory_and_contracts(self):
        for precision in (32, 64):
            with self.subTest(precision=precision), tempfile.TemporaryDirectory() as temporary:
                base = self.fixture(temporary)
                context = bundle.prepare_context(base.root, base.sdk, base.aax, plugin="midi", internal_precision=precision, selected_platform="darwin", runner=base.runner)
                reference = bundle.build_reference(context)
                directory = "midi" if precision == 32 else "midi64"
                expected = context.root / f"target/aax/{directory}/release/AAXPLUGIN/Release/ravet_midi_aax.aaxplugin"
                self.assertEqual(reference.artifact, expected)
                self.assertEqual(context.runner.calls[0], ["cargo", "build", "-p", "ravet-performance-native", "--release", "--locked"])
                self.assertIn(str(context.root / "examples/midi-aax"), context.runner.calls[2])
                self.assertIn(f"-DRAVET_AAX_INTERNAL_PRECISION={precision}", context.runner.calls[2])
                self.assertTrue(all(name in context.runner.calls[3] for name in bundle.MIDI_CONTRACTS))
                self.assertFalse(any(name in context.runner.calls[3] for name in bundle.PERFORMANCE_CONTRACTS))
                bundle.package_reference(reference)
                self.assertEqual(context.runner.calls[-1], ["codesign", "--verify", "--strict", str(expected)])
                bundle.validate_reference(reference)
                self.assertEqual(context.runner.calls[-2][-2:], ["--plugin", "midi"])
                self.assertEqual(Path(context.runner.calls[-1][0]).name, "ravet_aax_midi_registered_contracts")
                self.assertEqual(Path(context.runner.calls[-1][1]), expected / "Contents/MacOS/ravet_midi_aax")
                context.runner.fail = "ravet_aax_midi_registered_contracts"
                with self.assertRaises(subprocess.CalledProcessError):
                    bundle.validate_reference(reference)

    def test_raw_routing32_64_owns_bus_archive_and_both_registered_gates(self):
        for precision in (32, 64):
            with self.subTest(precision=precision), tempfile.TemporaryDirectory() as temporary:
                context = self.fixture(temporary, plugin="raw-routing", internal_precision=precision)
                reference = bundle.build_reference(context)
                directory = "raw-routing" if precision == 32 else "raw-routing64"
                self.assertEqual(reference.build, context.root / f"target/aax/{directory}/release")
                self.assertEqual(reference.artifact.name, "ravet_raw_routing_aax.aaxplugin")
                self.assertEqual(context.runner.calls[0], ["cargo", "build", "-p", "ravet-bus-routing-native", "--release", "--locked"])
                self.assertIn(str(context.root / "examples/raw-routing-aax"), context.runner.calls[2])
                self.assertIn(f"-DRAVET_BUS_ROUTING_LIBRARY={context.root / 'target/release/libravet_bus_routing_native.a'}", context.runner.calls[2])
                self.assertTrue(all(name in context.runner.calls[3] for name in bundle.RAW_ROUTING_CONTRACTS))
                self.assertEqual(context.runner.calls[3][-2:], ["--parallel", "1"])
                self.assertFalse(any(name in context.runner.calls[3] for name in bundle.MIDI_CONTRACTS))
                bundle.package_reference(reference)
                bundle.validate_reference(reference)
                self.assertIn(["codesign", "--verify", "--strict", str(reference.artifact)], context.runner.calls)
                self.assertEqual(context.runner.calls[-3][-2:], ["--plugin", "raw-routing"])
                self.assertEqual(Path(context.runner.calls[-2][0]).name, "ravet_aax_raw_routing_registered_contracts_fast")
                self.assertEqual(Path(context.runner.calls[-1][0]).name, "ravet_aax_raw_routing_registered_contracts")
                self.assertEqual(context.runner.calls[-2][1], context.runner.calls[-1][1])
                context.runner.fail = "ravet_aax_raw_routing_registered_contracts"
                with self.assertRaises(subprocess.CalledProcessError):
                    bundle.validate_reference(reference)

    def test_signature_verification_refusal_stops_before_bundle_validation(self):
        with tempfile.TemporaryDirectory() as temporary:
            context = self.fixture(temporary, plugin="midi")
            reference = bundle.build_reference(context)
            context.runner.fail = "codesign-verify"
            with self.assertRaises(subprocess.CalledProcessError):
                bundle.package_reference(reference)
            self.assertEqual(context.runner.calls[-2][:4], ["codesign", "--force", "--sign", "-"])
            self.assertEqual(context.runner.calls[-1][:3], ["codesign", "--verify", "--strict"])
            self.assertFalse(any("check-bundle.py" in value for call in context.runner.calls for value in call))

    def test_unknown_plugin_or_stock_performance_rejects_before_commands(self):
        with tempfile.TemporaryDirectory() as temporary:
            context = self.fixture(temporary)
            for plugin, profile in (("unknown", "native"), ("performance", "stock"), ("midi", "stock"), ("raw-routing", "stock")):
                with self.assertRaises(ValueError):
                    bundle.prepare_context(context.root, context.sdk, context.aax, selected_platform="darwin", runner=context.runner, plugin=plugin, profile=profile)
            self.assertFalse(context.runner.calls)

    def test_unknown_profile_rejects_before_build_commands(self):
        with tempfile.TemporaryDirectory() as temporary:
            context = self.fixture(temporary)
            with self.assertRaises(ValueError):
                bundle.prepare_context(context.root, context.sdk, context.aax, profile="invented", selected_platform="darwin", runner=context.runner)
            self.assertFalse(context.runner.calls)

    def test_missing_sdk_or_wrong_platform_reject_before_commands(self):
        with tempfile.TemporaryDirectory() as temporary:
            context = self.fixture(temporary)
            (context.aax / "Interfaces/AAX_IParameter.h").unlink()
            for platform in ("darwin", "linux"):
                with self.assertRaises(ValueError):
                    bundle.prepare_context(context.root, context.sdk, context.aax, selected_platform=platform, runner=context.runner)
            self.assertFalse(context.runner.calls)

    def test_optional_editor_selects_feature_separate_bundle_and_actual_gui_gate(self):
        with tempfile.TemporaryDirectory() as temporary:
            context = self.fixture(temporary, editor=True)
            reference = bundle.build_reference(context)
            expected = context.root / "target/aax/editor/release/AAXPLUGIN/Release/ravet_gain_aax.aaxplugin"
            self.assertEqual(reference.artifact, expected)
            self.assertIn("ravet-gain-native/native-editor", context.runner.calls[0])
            self.assertIn("-DRAVET_AAX_EDITOR=ON", context.runner.calls[2])
            self.assertTrue(all(name in context.runner.calls[3] for name in bundle.EDITOR_CONTRACTS))
            bundle.validate_reference(reference)
            self.assertEqual(Path(context.runner.calls[-2][0]).name, "ravet_aax_native_registered_contracts")
            self.assertEqual(Path(context.runner.calls[-1][0]).name, "ravet_aax_editor_registered_contracts")
            context.runner.fail = "ravet_aax_editor_registered_contracts"
            with self.assertRaises(subprocess.CalledProcessError):
                bundle.validate_reference(reference)

    def test_editor_invalid_combinations_reject_before_any_commands(self):
        with tempfile.TemporaryDirectory() as temporary:
            context = self.fixture(temporary)
            for plugin, profile in (("performance", "native"), ("midi", "native"), ("raw-routing", "native"), ("gain", "stock")):
                with self.assertRaises(ValueError):
                    bundle.prepare_context(context.root, context.sdk, context.aax, selected_platform="darwin", runner=context.runner, plugin=plugin, profile=profile, editor=True)
            self.assertFalse(context.runner.calls)
            reference = bundle.build_reference(context)
            self.assertNotIn("ravet-gain-native/native-editor", context.runner.calls[0])
            self.assertIn("-DRAVET_AAX_EDITOR=OFF", context.runner.calls[2])
            self.assertFalse(any(name in context.runner.calls[3] for name in bundle.EDITOR_CONTRACTS))

    def test_contract_build_failure_stops_pipeline(self):
        with tempfile.TemporaryDirectory() as temporary:
            context = self.fixture(temporary)
            context.runner.fail = "ctest"
            with self.assertRaises(subprocess.CalledProcessError):
                bundle.build_reference(context)
            self.assertFalse(any(args[0] == "codesign" for args in context.runner.calls))

    def test_actual_registered_consumer_failure_propagates(self):
        with tempfile.TemporaryDirectory() as temporary:
            context = self.fixture(temporary)
            reference = bundle.build_reference(context)
            context.runner.fail = "ravet_aax_native_registered_contracts"
            with self.assertRaises(subprocess.CalledProcessError):
                bundle.validate_reference(reference)
            self.assertEqual(Path(context.runner.calls[-1][0]).name, "ravet_aax_native_registered_contracts")


if __name__ == "__main__":
    unittest.main()
