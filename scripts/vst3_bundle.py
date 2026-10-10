"""Local public VST3 reference build, packaging and validation mechanics."""
from dataclasses import dataclass
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


GAIN_CONTRACTS = (
    "ravet_vst3_contracts", "ravet_vst3_schema_contracts",
    "ravet_vst3_parameter_input", "ravet_vst3_parameter_input_fast",
    "ravet_vst3_transport", "ravet_vst3_transport_fast",
    "ravet_vst3_performance_input", "ravet_vst3_performance_input_fast",
    "ravet_vst3_performance_output", "ravet_vst3_performance_output_fast",
    "ravet_vst3_native_audio", "ravet_vst3_native_audio_fast",
)


@dataclass(frozen=True)
class PluginProfile:
    name: str
    packages: tuple[str, ...]
    binary: str
    contracts: tuple[str, ...]


PROFILES = {
    "gain": PluginProfile("gain", ("ravet-gain-native",), "ravet_gain_vst3", GAIN_CONTRACTS),
    "performance": PluginProfile("performance", ("ravet-gain-native", "ravet-performance-native"),
                                 "ravet_performance_vst3", ("ravet_vst3_performance_instrument",)),
}


class CommandRunner:
    """Argument-list commands with repository cwd and two compiler workers."""
    def __init__(self, root):
        self.root = root
        self.environment = dict(os.environ, CARGO_BUILD_JOBS="2")

    def run(self, arguments, *, capture=False):
        return subprocess.run([str(item) for item in arguments], cwd=self.root,
                              env=self.environment, check=True, text=True,
                              stdout=subprocess.PIPE if capture else None).stdout


@dataclass(frozen=True)
class BundleContext:
    root: Path
    sdk: Path
    plugin: PluginProfile
    profile: str
    platform: str
    native_editor: bool
    factory_programs: bool
    runner: CommandRunner

    @property
    def config(self):
        return "Release" if self.profile == "release" else "Debug"


@dataclass(frozen=True)
class BuiltReference:
    context: BundleContext
    target: Path
    build: Path


def prepare_context(root, sdk, plugin="gain", profile="release", *, native_editor=False,
                    factory_programs=False, platform=None, runner=None):
    """Reject unavailable profiles/options/SDK before any build mutation."""
    platform = sys.platform if platform is None else platform
    if plugin not in PROFILES or profile not in ("debug", "release"):
        raise ValueError("select gain/performance and debug/release")
    if plugin != "gain" and (native_editor or factory_programs):
        raise ValueError("native editor and factory programs are Gain options only")
    if platform != "darwin" and (native_editor or factory_programs):
        raise ValueError("native editor and factory program validation currently require macOS")
    root, sdk = Path(root).resolve(), Path(sdk).resolve()
    for relative in ("LICENSE.txt", "CMakeLists.txt", "pluginterfaces/vst/ivstaudioprocessor.h"):
        if not (sdk / relative).is_file():
            raise ValueError(f"complete externally installed VST3 SDK is missing {relative}")
    return BundleContext(root, sdk, PROFILES[plugin], profile, platform,
                         native_editor, factory_programs, runner or CommandRunner(root))


def build_reference(context):
    """Build only the selected reference and its contract targets."""
    cargo = ["cargo", "build", "--locked"]
    for package in context.plugin.packages:
        cargo += ["-p", package]
    if context.native_editor:
        cargo += ["--features", "native-editor"]
    if context.profile == "release":
        cargo.append("--release")
    context.runner.run(cargo)
    metadata = json.loads(context.runner.run(["cargo", "metadata", "--no-deps", "--locked",
                                              "--format-version", "1"], capture=True))
    target = Path(metadata["target_directory"])
    build = target / "vst3"
    if context.plugin.name != "gain":
        build /= context.plugin.name
    build /= context.profile
    def library(name):
        filename = f"ravet_{name}_native.lib" if context.platform == "win32" else f"libravet_{name}_native.a"
        return target / context.profile / filename
    performance = library("performance") if context.plugin.name == "performance" else ""
    context.runner.run(["cmake", "-S", context.root / "examples/gain-vst3", "-B", build,
                        f"-DCMAKE_BUILD_TYPE={context.config}", f"-DRAVET_VST3_SDK={context.sdk}",
                        f"-DRAVET_GAIN_LIBRARY={library('gain')}", f"-DRAVET_PERFORMANCE_LIBRARY={performance}",
                        f"-DRAVET_GAIN_EDITOR={'ON' if context.native_editor else 'OFF'}",
                        f"-DRAVET_GAIN_PROGRAMS={'ON' if context.factory_programs else 'OFF'}"])
    targets = [context.plugin.binary, "validator", *context.plugin.contracts]
    if context.native_editor:
        targets.append("ravet_vst3_editor_contracts")
    if context.factory_programs:
        targets.append("ravet_vst3_program_contracts")
    context.runner.run(["cmake", "--build", build, "--config", context.config,
                        "--target", *targets, "--parallel", "2"])
    tests = ["ctest", "--test-dir", build, "-C", context.config, "--output-on-failure"]
    if context.plugin.name == "performance":
        tests += ["-R", "^ravet_vst3_performance_instrument$"]
    context.runner.run(tests)
    return BuiltReference(context, target, build)


def package_reference(reference):
    """Attach public notices and locally sign only generated selected bundles."""
    context = reference.context
    artifacts = sorted(reference.build.rglob(context.plugin.binary + ".vst3"))
    if not artifacts:
        raise RuntimeError("VST3 build produced no selected bundle")
    for artifact in artifacts:
        notices = artifact / "Contents/Resources/notices"
        notices.mkdir(parents=True, exist_ok=True)
        shutil.copy2(context.sdk / "LICENSE.txt", notices / "Steinberg-VST3-LICENSE.txt")
        shutil.copy2(context.root / "LICENSE", notices / "RAVET-LICENSE.txt")
        if context.platform == "darwin":
            context.runner.run(["codesign", "--force", "--sign", "-", artifact])
    return artifacts


def validate_reference(reference, artifacts):
    """Dynamically load selected bundles; propagate SDK/editor/program faults."""
    context = reference.context
    validator = reference.build / "bin" / context.config / ("validator.exe" if context.platform == "win32" else "validator")
    for artifact in artifacts:
        context.runner.run([validator, artifact])
        if context.native_editor:
            context.runner.run([reference.build / "ravet_vst3_editor_contracts", artifact])
        if context.factory_programs:
            context.runner.run([reference.build / "ravet_vst3_program_contracts", artifact])
