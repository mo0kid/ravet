"""Explicit local AUv2 packaging over the shared public VST3 build pipeline."""
from dataclasses import dataclass
from pathlib import Path
import platform
import shutil
import sys
import vst3_bundle


@dataclass(frozen=True)
class BundleContext:
    root: Path
    sdk: Path
    vst3: vst3_bundle.BundleContext
    native_editor: bool
    factory_programs: bool


@dataclass(frozen=True)
class BuiltReference:
    context: BundleContext
    build: Path
    component: Path
    embedded: Path
    validators: tuple[str, ...]


def prepare_context(root, vst3_sdk, audio_sdk, plugin="gain", *, native_editor=False,
                    factory_programs=False, selected_platform=None, runner=None):
    if (sys.platform if selected_platform is None else selected_platform) != "darwin":
        raise ValueError("AudioUnit build requires macOS and Xcode")
    root, audio_sdk = Path(root).resolve(), Path(audio_sdk).resolve()
    for relative in ("AudioUnitSDK.xcodeproj", "include/AudioUnitSDK/MusicDeviceBase.h", "LICENSE.txt"):
        if not (audio_sdk / relative).exists():
            raise ValueError(f"external AudioUnit SDK is missing {relative}")
    vst3 = vst3_bundle.prepare_context(root, vst3_sdk, plugin, "release", native_editor=native_editor,
                                      factory_programs=factory_programs, platform="darwin", runner=runner)
    return BundleContext(root, audio_sdk, vst3, native_editor, factory_programs)


def build_reference(context):
    """Reuse VST3 selection/build/validation, then build one AU profile."""
    vst3 = vst3_bundle.build_reference(context.vst3)
    artifacts = vst3_bundle.package_reference(vst3)
    vst3_bundle.validate_reference(vst3, artifacts)
    embedded = next((path for path in artifacts if path.parent.name == "Release"), artifacts[0])
    sdk_build = vst3.target / "au/sdk"
    context.vst3.runner.run(["xcodebuild", "-project", context.sdk / "AudioUnitSDK.xcodeproj",
                            "-target", "AudioUnitSDK", "-configuration", "Release", "-jobs", "2",
                            f"SYMROOT={sdk_build}", f"OBJROOT={vst3.target / 'au/sdk-obj'}",
                            f"ARCHS={platform.machine()}", "ONLY_ACTIVE_ARCH=YES", "CODE_SIGNING_ALLOWED=NO",
                            "MACOSX_DEPLOYMENT_TARGET=12.0"])
    if context.vst3.plugin.name == "performance":
        build = vst3.target / "au-performance/release"
        source = context.root / "examples/performance-au"
        target = "ravet_performance_au"
        validators = ("ravet_au_performance_contracts", "ravet_au_native_parameters_contracts", "ravet_au_native_audio_contracts", "ravet_au_native_transport_contracts", "ravet_au_native_transport_fast_contracts", "ravet_au_native_midi_output_contracts", "ravet_au_native_midi_output_fast_contracts", "ravet_au_synthetic_midi_output_contracts", "ravet_au_native_audio64_contracts", "ravet_au_native_audio64_fast_contracts", "ravet_au_synthetic_audio64_contracts")
        definitions = [f"-DRAVET_VST3_SDK={context.vst3.sdk}",
                       f"-DRAVET_GAIN_LIBRARY={vst3.target / 'release/libravet_gain_native.a'}",
                       f"-DRAVET_PERFORMANCE_LIBRARY={vst3.target / 'release/libravet_performance_native.a'}"]
    else:
        build, source, target = vst3.build, context.root / "examples/gain-vst3", "ravet_gain_au"
        validators = ("ravet_au_contracts",)
        if context.native_editor:
            validators += ("ravet_au_editor_contracts",)
        if context.factory_programs:
            validators += ("ravet_au_program_contracts",)
        definitions = []
    context.vst3.runner.run(["cmake", "-S", source, "-B", build, "-DCMAKE_BUILD_TYPE=Release",
                            f"-DRAVET_AUDIOUNIT_SDK={context.sdk}",
                            f"-DRAVET_AUDIOUNIT_LIBRARY={sdk_build / 'Release/libAudioUnitSDK.a'}", *definitions])
    context.vst3.runner.run(["cmake", "--build", build, "--target", target, *validators, "--parallel", "2"])
    return BuiltReference(context, build, build / f"AU/{target}.component", embedded, validators)


def package_reference(reference):
    """Replace only the generated component's own public embedded resource."""
    resources = reference.component / "Contents/Resources"
    resources.mkdir(parents=True, exist_ok=True)
    embedded = resources / "plugin.vst3"
    if embedded.exists():
        shutil.rmtree(embedded)
    shutil.copytree(reference.embedded, embedded)
    notices = resources / "notices"
    notices.mkdir(exist_ok=True)
    shutil.copy2(reference.context.sdk / "LICENSE.txt", notices / "Apple-AudioUnitSDK-LICENSE.txt")
    shutil.copy2(reference.context.vst3.sdk / "LICENSE.txt", notices / "Steinberg-VST3-LICENSE.txt")
    shutil.copy2(reference.context.root / "LICENSE", notices / "RAVET-LICENSE.txt")
    reference.context.vst3.runner.run(["codesign", "--force", "--sign", "-", reference.component])
    return reference.component


def validate_reference(reference):
    for validator in reference.validators:
        reference.context.vst3.runner.run([reference.build / validator, reference.component])
