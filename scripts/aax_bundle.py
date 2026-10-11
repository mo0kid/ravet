"""Local external-SDK AAX reference profiles, packaging and actual binary contracts."""
from dataclasses import dataclass
import json
from pathlib import Path
import shutil
import sys
import vst3_bundle


PROFILES = {
    "native": ("ravet_aax_native_parameters", "ravet_aax_native_parameters_fast",
               "ravet_aax_processor", "ravet_aax_processor_fast", "ravet_aax_model",
               "ravet_aax_model_fast", "ravet_aax_transport", "ravet_aax_transport_fast",
               "ravet_aax_transport_model", "ravet_aax_transport_model_fast",
               "ravet_aax_audio", "ravet_aax_audio_fast", "ravet_aax_multibus", "ravet_aax_multibus_fast", "ravet_aax_midi_output", "ravet_aax_midi_output_fast", "ravet_aax_generated_model", "ravet_aax_generated_model_fast", "ravet_aax_timing_model", "ravet_aax_timing_model_fast", "ravet_aax_native_registered_contracts"),
    "stock": ("ravet_aax_native_parameters", "ravet_aax_native_parameters_fast", "ravet_aax_registered_contracts"),
}
EDITOR_CONTRACTS = ("ravet_aax_editor_controls", "ravet_aax_editor_controls_fast", "ravet_aax_editor_lifecycle", "ravet_aax_editor_registered_contracts")
PRECISION_CONTRACTS = tuple(f"ravet_aax_precision_{group}_{mode}"
                            for group in ("precision", "owner", "model", "multibus", "generated", "timing", "faults", "maximum")
                            for mode in ("normal", "fast"))
CONTRACTS = PROFILES["native"]
PLUGINS = ("gain", "performance", "midi", "raw-routing", "gain-meters")
METER_CONTRACTS = ("ravet_aax_meters", "ravet_aax_meters_fast", "ravet_aax_meter_composition", "ravet_aax_meter_composition_fast", "ravet_aax_meters_registered_contracts")
RAW_ROUTING_CONTRACTS = ("ravet_aax_raw_routing", "ravet_aax_raw_routing_fast", "ravet_aax_raw_routing_registered_contracts", "ravet_aax_raw_routing_registered_contracts_fast")
MIDI_CONTRACTS = ("ravet_aax_short_midi", "ravet_aax_short_midi_fast", "ravet_aax_midi_telemetry", "ravet_aax_midi_telemetry_fast", "ravet_aax_midi_registered_contracts")
PERFORMANCE_CONTRACTS = ("ravet_aax_note_input", "ravet_aax_note_input_fast", "ravet_aax_instrument_model", "ravet_aax_instrument_model_fast", "ravet_aax_instrument_registered_contracts")


@dataclass(frozen=True)
class BundleContext:
    root: Path
    sdk: Path
    aax: Path
    runner: vst3_bundle.CommandRunner
    profile: str
    plugin: str = "gain"
    editor: bool = False
    internal_precision: int = 32


@dataclass(frozen=True)
class BuiltReference:
    context: BundleContext
    build: Path
    artifact: Path


def prepare_context(root, sdk, aax, *, profile="native", plugin="gain", editor=False, internal_precision=32, selected_platform=None, runner=None):
    if type(internal_precision) is not int or internal_precision not in (32, 64):
        raise ValueError("AAX internal precision must be 32 or 64")
    if profile == "stock" and internal_precision != 32:
        raise ValueError("Stock AAX profile only supports internal precision 32")
    if editor and (plugin != "gain" or profile != "native"):
        raise ValueError("AAX editor requires the macOS native Gain profile")
    if plugin not in PLUGINS:
        raise ValueError("AAX plugin must be gain, performance, midi, raw-routing or gain-meters")
    if plugin in ("performance", "midi", "raw-routing", "gain-meters") and profile != "native":
        raise ValueError("Selected AAX reference requires the native profile")
    if profile not in PROFILES:
        raise ValueError("AAX profile must be native or stock")
    if (sys.platform if selected_platform is None else selected_platform) != "darwin":
        raise ValueError("local AAX reference currently requires macOS")
    vst3 = vst3_bundle.prepare_context(root, sdk, platform="darwin", runner=runner)
    aax = Path(aax).resolve()
    for relative in ("Interfaces/AAX.h", "Interfaces/AAX_IParameter.h", "Interfaces/AAX_CParameterManager.h"):
        if not (aax / relative).exists():
            raise ValueError(f"complete external AAX SDK is missing {relative}")
    return BundleContext(vst3.root, vst3.sdk, aax, vst3.runner, profile, plugin, editor, internal_precision)


def build_reference(context):
    plugin = context.plugin
    package = "bus-routing" if plugin == "raw-routing" else "performance" if plugin == "midi" else "gain" if plugin == "gain-meters" else plugin
    packages = ["-p", f"ravet-{package}-native"]
    if plugin in ("gain", "gain-meters") and context.profile == "native":
        packages += ["-p", "ravet-bus-routing-native"]
    if plugin == "gain-meters":
        packages += ["-p", "ravet-performance-native"]
    features = ["--features", "ravet-gain-native/native-editor"] if context.editor else []
    context.runner.run(["cargo", "build", *packages, *features, "--release", "--locked"])
    metadata = json.loads(context.runner.run(["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"], capture=True))
    target = Path(metadata["target_directory"])
    directory = f"aax/{plugin}/release" if plugin in ("performance", "midi", "raw-routing", "gain-meters") else ("aax/release" if context.profile == "native" else "aax/stock/release")
    if context.editor:
        directory = "aax/editor/release"
    if context.internal_precision == 64:
        directory = ("aax/editor64/release" if context.editor else
                         f"aax/{plugin}64/release" if plugin in ("performance", "midi", "raw-routing", "gain-meters") else "aax/native64/release")
    build = target / directory
    configure = ["cmake", "-S", context.root / (f"examples/{plugin}-aax" if plugin in ("performance", "midi", "raw-routing", "gain-meters") else "examples/gain-vst3"), "-B", build,
                 "-DCMAKE_BUILD_TYPE=Release", f"-DRAVET_VST3_SDK={context.sdk}", f"-DRAVET_AAX_SDK={context.aax}",
                 f"-DRAVET_AAX_INTERNAL_PRECISION={context.internal_precision}"]
    if plugin == "gain-meters":
        configure += [f"-DRAVET_GAIN_LIBRARY={target / 'release/libravet_gain_native.a'}", f"-DRAVET_METER_PERFORMANCE_LIBRARY={target / 'release/libravet_performance_native.a'}", f"-DRAVET_METER_ROUTING_LIBRARY={target / 'release/libravet_bus_routing_native.a'}"]
        contracts, additional = METER_CONTRACTS, ()
    elif plugin == "raw-routing":
        configure += [f"-DRAVET_BUS_ROUTING_LIBRARY={target / 'release/libravet_bus_routing_native.a'}"]
        contracts, additional = RAW_ROUTING_CONTRACTS, ()
    elif plugin in ("performance", "midi"):
        configure += [f"-DRAVET_PERFORMANCE_LIBRARY={target / 'release/libravet_performance_native.a'}"]
        contracts = MIDI_CONTRACTS if plugin == "midi" else PERFORMANCE_CONTRACTS
        additional = ()
    else:
        configure += [f"-DRAVET_AAX_REFERENCE_PROFILE={context.profile}", f"-DRAVET_GAIN_LIBRARY={target / 'release/libravet_gain_native.a'}"]
        if context.profile == "native":
            configure += [f"-DRAVET_BUS_ROUTING_LIBRARY={target / 'release/libravet_bus_routing_native.a'}"]
        contracts = PROFILES[context.profile] + (EDITOR_CONTRACTS if context.editor else ())
        if context.internal_precision == 64:
            contracts += PRECISION_CONTRACTS
        configure += [f"-DRAVET_AAX_EDITOR={'ON' if context.editor else 'OFF'}"]
        additional = vst3_bundle.GAIN_CONTRACTS
    context.runner.run(configure)
    context.runner.run(["cmake", "--build", build, "--target", f"ravet_{plugin.replace('-', '_')}_aax", *additional, *contracts, "--parallel", "1" if plugin in ("raw-routing", "gain-meters") else "2"])
    context.runner.run(["ctest", "--test-dir", build, "--output-on-failure"])
    return BuiltReference(context, build, build / f"AAXPLUGIN/Release/ravet_{plugin.replace('-', '_')}_aax.aaxplugin")


def package_reference(reference):
    context = reference.context
    notices = reference.artifact / "Contents/Resources/notices"
    notices.mkdir(parents=True, exist_ok=True)
    shutil.copy2(context.sdk / "LICENSE.txt", notices / "Steinberg-VST3-LICENSE.txt")
    shutil.copy2(context.root / "LICENSE", notices / "RAVET-LICENSE.txt")
    (notices / "AAX-SDK-NOTICE.txt").write_text("Built against an externally supplied Avid AAX SDK. Review its header licence and your Avid agreement before distribution. This local artifact is not a production release.\n")
    context.runner.run(["codesign", "--force", "--sign", "-", reference.artifact])
    context.runner.run(["codesign", "--verify", "--strict", reference.artifact])
    return reference.artifact


def validate_reference(reference):
    context = reference.context
    context.runner.run([sys.executable, context.root / "tools/aax-validation/check-bundle.py", reference.artifact, "--plugin", context.plugin])
    contracts = METER_CONTRACTS if context.plugin == "gain-meters" else RAW_ROUTING_CONTRACTS if context.plugin == "raw-routing" else (MIDI_CONTRACTS if context.plugin == "midi" else PERFORMANCE_CONTRACTS) if context.plugin in ("performance", "midi") else PROFILES[context.profile]
    context.runner.run([reference.build / contracts[-1],
                        reference.artifact / f"Contents/MacOS/ravet_{context.plugin.replace('-', '_')}_aax"])
    if context.plugin == "raw-routing":
        context.runner.run([reference.build / "ravet_aax_raw_routing_registered_contracts",
                            reference.artifact / "Contents/MacOS/ravet_raw_routing_aax"])
    if context.editor:
        context.runner.run([reference.build / "ravet_aax_editor_registered_contracts",
                            reference.artifact / "Contents/MacOS/ravet_gain_aax"])
