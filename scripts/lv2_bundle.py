"""Shared local LV2 build, packaging and actual-binary validation mechanics."""
from dataclasses import dataclass
import json
import os
import pathlib
import shutil
import subprocess
import sys


@dataclass(frozen=True)
class PluginProfile:
    name: str
    packages: tuple[str, ...]
    source: str
    binary: str
    metadata: tuple[str, ...]
    validators: tuple[str, ...]


PROFILES = {
    "note-routing": PluginProfile("note-routing", ("ravet-gain-native", "ravet-bus-routing-native"), "note-routing-lv2", "ravet_note_routing_lv2",
                                 ("manifest.ttl", "routing.ttl"), ("ravet_lv2_output_contracts", "ravet_lv2_output_fast_contracts",
                                 "ravet_lv2_processor_output_contracts", "ravet_lv2_processor_output_fast_contracts", "ravet_lv2_routing_contracts", "ravet_lv2_origin_contracts")),
    "gain": PluginProfile("gain", ("ravet-gain-native",), "gain-lv2", "ravet_gain_lv2",
                          ("manifest.ttl", "gain.ttl"), ("ravet_lv2_contracts", "ravet_lv2_state_contracts", "ravet_lv2_origin_contracts")),
    "performance": PluginProfile("performance", ("ravet-gain-native", "ravet-performance-native"),
                                 "performance-lv2", "ravet_performance_lv2", ("manifest.ttl", "performance.ttl"),
                                 ("ravet_lv2_performance_contracts", "ravet_lv2_transport_contracts",
                                  "ravet_lv2_transport_fast_contracts", "ravet_lv2_processor_transport_contracts",
                                  "ravet_lv2_origin_contracts")),
}


class CommandRunner:
    """Argument-list execution with one repository cwd and two-worker policy."""
    def __init__(self, root):
        self.root = root
        self.environment = dict(os.environ, CARGO_BUILD_JOBS="2")

    def run(self, arguments, *, capture=False):
        return subprocess.run([str(argument) for argument in arguments], cwd=self.root,
                              env=self.environment, check=True, text=True,
                              stdout=subprocess.PIPE if capture else None).stdout


@dataclass(frozen=True)
class BundleContext:
    root: pathlib.Path
    sdk: pathlib.Path
    profile: PluginProfile
    platform: str
    runner: CommandRunner


@dataclass(frozen=True)
class BuiltReference:
    context: BundleContext
    target: pathlib.Path
    build: pathlib.Path


def prepare_context(root, sdk, plugin, *, platform=None, runner=None):
    """Validate explicit platform/SDK before producing local build artifacts."""
    selected_platform = sys.platform if platform is None else platform
    if selected_platform not in ("darwin", "linux"):
        raise ValueError("current binary loading fixture supports macOS/Linux only")
    root, sdk = pathlib.Path(root).resolve(), pathlib.Path(sdk).resolve()
    if plugin not in PROFILES:
        raise ValueError("select gain, performance or note-routing")
    for relative in ("include/lv2/core/lv2.h", "include/lv2/atom/atom.h", "include/lv2/state/state.h",
                     "include/lv2/urid/urid.h", "include/lv2/midi/midi.h", "include/lv2/time/time.h", "COPYING"):
        if not (sdk / relative).is_file():
            raise ValueError(f"external LV2 source SDK is missing {relative}")
    return BundleContext(root, sdk, PROFILES[plugin], selected_platform, runner or CommandRunner(root))


def build_reference(context):
    """Build the selected archive/module/validators through the shared pipeline."""
    metadata = json.loads(context.runner.run(["cargo", "metadata", "--no-deps", "--locked",
                                              "--format-version", "1"], capture=True))
    target = pathlib.Path(metadata["target_directory"])
    cargo = ["cargo", "build", "--release", "--locked"]
    for package in context.profile.packages:
        cargo += ["-p", package]
    context.runner.run(cargo)
    build = target / "lv2/release"
    performance = target / "release/libravet_performance_native.a" if context.profile.name == "performance" else ""
    context.runner.run(["cmake", "-S", context.root / "examples/gain-lv2", "-B", build,
                        "-DCMAKE_BUILD_TYPE=Release", f"-DRAVET_LV2_SDK={context.sdk}",
                        f"-DRAVET_GAIN_LIBRARY={target / 'release/libravet_gain_native.a'}",
                        f"-DRAVET_PERFORMANCE_LIBRARY={performance}",
                        f"-DRAVET_ROUTING_LIBRARY={target / 'release/libravet_bus_routing_native.a' if context.profile.name == 'note-routing' else ''}"])
    context.runner.run(["cmake", "--build", build, "--target", context.profile.binary,
                        *context.profile.validators, "--parallel", "2"])
    return BuiltReference(context, target, build)


def package_reference(reference):
    """Copy only selected public metadata/module/notices into its generated bundle."""
    context, profile = reference.context, reference.context.profile
    artifact = reference.target / f"lv2/ravet_{profile.name}.lv2"
    artifact.mkdir(parents=True, exist_ok=True)
    for name in profile.metadata:
        shutil.copy2(context.root / "examples" / profile.source / name, artifact / name)
    shutil.copy2(reference.build / f"{profile.binary}.so", artifact / f"{profile.binary}.so")
    shutil.copy2(context.sdk / "COPYING", artifact / "LV2-SDK-LICENSE.txt")
    shutil.copy2(context.root / "LICENSE", artifact / "RAVET-LICENSE.txt")
    shutil.copy2(context.root / "LICENSING.md", artifact / "RAVET-LICENSING.md")
    if context.platform == "darwin":
        context.runner.run(["codesign", "--force", "--sign", "-", artifact / f"{profile.binary}.so"])
    return artifact


def validate_reference(reference, artifact):
    """Exercise the actual packaged module, propagating every validator failure."""
    for validator in reference.context.profile.validators:
        reference.context.runner.run([reference.build / validator,
                                      artifact / f"{reference.context.profile.binary}.so"])
