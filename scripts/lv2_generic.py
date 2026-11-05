"""Bundle any RAVET product library as LV2 through the generic adapter.

The product crate is built with Cargo, then crates/ravet-plugin/lv2 is built with
CMake against an external LV2 SDK. CMake generates the Turtle from the
library's ravet_plugin_v1 table, assembles the bundle and runs the contract
tests (the mock product's full contracts, the product's smoke test and, when
lilv is installed, the lilv host check). The bundle is copied to
target/lv2/<name>.lv2 with licence notices; macOS binaries are signed ad hoc.
"""
from dataclasses import dataclass
import json
import pathlib
import shutil
import sys

from lv2_bundle import CommandRunner

# Product crates exported with ravet_plugin::export_plugin!. "mock" is the C++
# stand-in table built by CMake itself.
PRODUCTS = {
    "generic": "ravet-gain-plugin",
    "delay": "ravet-delay-plugin",
    "mock": None,
}
HEADERS = ("core/lv2.h", "atom/atom.h", "atom/util.h", "buf-size/buf-size.h", "midi/midi.h",
           "options/options.h", "state/state.h", "time/time.h", "ui/ui.h", "urid/urid.h")


@dataclass(frozen=True)
class GenericContext:
    root: pathlib.Path
    sdk: pathlib.Path
    name: str
    crate: str | None
    platform: str
    runner: CommandRunner


def sdk_include(sdk):
    """The directory holding lv2/core/lv2.h: official (include/) or flat layout."""
    for candidate in (sdk / "include", sdk):
        if (candidate / "lv2/core/lv2.h").is_file():
            return candidate
    return None


def prepare_generic(root, sdk, plugin=None, crate=None, *, platform=None, runner=None):
    """Validate the selection and SDK before building anything."""
    selected = sys.platform if platform is None else platform
    if selected not in ("darwin", "linux", "win32"):
        raise ValueError("the generic LV2 adapter builds on macOS, Linux and Windows only")
    root, sdk = pathlib.Path(root).resolve(), pathlib.Path(sdk).resolve()
    if crate:
        name = crate.replace("-", "_")
    elif plugin in PRODUCTS:
        crate, name = PRODUCTS[plugin], f"ravet_{plugin}"
    else:
        raise ValueError("select a generic product (generic, delay, mock) or pass --crate")
    include = sdk_include(sdk)
    if include is None:
        raise ValueError("external LV2 SDK needs include/lv2/core/lv2.h or lv2/core/lv2.h")
    for header in HEADERS:
        if not (include / "lv2" / header).is_file():
            raise ValueError(f"external LV2 SDK is missing lv2/{header}")
    if not (sdk / "COPYING").is_file():
        raise ValueError("external LV2 SDK is missing COPYING")
    return GenericContext(root, sdk, name, crate, selected, runner or CommandRunner(root))


def library_name(stem, platform):
    if platform == "win32":
        return f"{stem}.dll"
    return f"lib{stem}.dylib" if platform == "darwin" else f"lib{stem}.so"


def module_suffix(platform):
    return {"win32": ".dll", "darwin": ".dylib"}.get(platform, ".so")


def build_generic(context):
    """Build the product library and the adapter; return (target, build directory)."""
    metadata = json.loads(context.runner.run(["cargo", "metadata", "--no-deps", "--locked",
                                              "--format-version", "1"], capture=True))
    target = pathlib.Path(metadata["target_directory"])
    configure = ["cmake", "-S", context.root / "crates/ravet-plugin/lv2",
                 "-B", target / "lv2" / f"generic-{context.name}",
                 "-DCMAKE_BUILD_TYPE=Release", f"-DRAVET_LV2_SDK={context.sdk}"]
    if context.crate:
        context.runner.run(["cargo", "build", "--release", "--locked", "-p", context.crate])
        library = target / "release" / library_name(context.crate.replace("-", "_"), context.platform)
        configure += [f"-DRAVET_PLUGIN_LIBRARY={library}", f"-DRAVET_LV2_BUNDLE_NAME={context.name}"]
    else:
        configure += ["-DRAVET_PLUGIN_LIBRARY=", "-DRAVET_LV2_BUNDLE_NAME="]
    build = target / "lv2" / f"generic-{context.name}"
    context.runner.run(configure)
    context.runner.run(["cmake", "--build", build, "--config", "Release", "--parallel", "2"])
    context.runner.run(["ctest", "--test-dir", build, "-C", "Release", "--output-on-failure"])
    return target, build


def package_generic(context, target, build):
    """Copy the assembled bundle, add notices and sign on macOS."""
    source = build / "bundles" / f"{context.name if context.crate else 'ravet_lv2_mock'}.lv2"
    artifact = target / "lv2" / f"{context.name if context.crate else 'ravet_mock'}.lv2"
    if artifact.exists():
        shutil.rmtree(artifact)
    shutil.copytree(source, artifact)
    shutil.copy2(context.sdk / "COPYING", artifact / "LV2-SDK-LICENSE.txt")
    shutil.copy2(context.root / "LICENSE", artifact / "RAVET-LICENSE.txt")
    shutil.copy2(context.root / "LICENSING.md", artifact / "RAVET-LICENSING.md")
    if context.platform == "darwin":
        suffix = module_suffix(context.platform)
        for binary in ("ravet_lv2", "ravet_lv2_ui", "ravet_product"):
            context.runner.run(["codesign", "--force", "--sign", "-", artifact / f"{binary}{suffix}"])
    return artifact


def validate_generic(context, artifact):
    """Validate the packaged Turtle with lv2_validate (sord) when it is installed."""
    validator = shutil.which("lv2_validate")
    if validator is None:
        print("lv2_validate not found: Turtle validation skipped", file=sys.stderr)
        return False
    context.runner.run([validator, artifact / "manifest.ttl", artifact / "plugin.ttl"])
    return True
