"""Bundle any RAVET product library as LV2 through the generic adapter.

The product crate is built with Cargo, then crates/ravet-plugin/lv2 is built with
CMake against an external LV2 SDK. CMake generates the Turtle from the
library's ravet_plugin_v1 table, assembles the bundle and runs the contract
tests (the mock product's full contracts, the product's smoke test and, when
lilv is installed, the lilv host check). The bundle is copied to
target/lv2/<name>.lv2 with licence notices; macOS binaries are signed ad hoc.

cross="windows" (--target windows) cross-compiles for x86_64 Windows with
cargo-xwin and clang-cl (see windows_cross). Turtle generation and the contract
tests then run the Windows programs under Wine, which must be installed; the
bundle is copied to target/lv2/windows/<name>.lv2.
"""
from dataclasses import dataclass
import json
import pathlib
import shutil
import sys

from lv2_bundle import CommandRunner
import windows_cross

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
    cross: str | None = None


def sdk_include(sdk):
    """The directory holding lv2/core/lv2.h: official (include/) or flat layout."""
    for candidate in (sdk / "include", sdk):
        if (candidate / "lv2/core/lv2.h").is_file():
            return candidate
    return None


def prepare_generic(root, sdk, plugin=None, crate=None, *, platform=None, runner=None, cross=None):
    """Validate the selection and SDK before building anything."""
    selected = sys.platform if platform is None else platform
    if selected not in ("darwin", "linux", "win32"):
        raise ValueError("the generic LV2 adapter builds on macOS, Linux and Windows only")
    if cross not in (None, "windows"):
        raise ValueError("--target accepts only windows")
    if cross and selected == "win32":
        raise ValueError("--target windows cross-compiles from Linux or macOS; build natively on Windows")
    if cross and shutil.which("wine") is None:
        raise ValueError("--target windows needs Wine to generate the LV2 Turtle from the Windows library")
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
    return GenericContext(root, sdk, name, crate, selected, runner or CommandRunner(root), cross)


def library_name(stem, platform):
    if platform == "win32":
        return f"{stem}.dll"
    return f"lib{stem}.dylib" if platform == "darwin" else f"lib{stem}.so"


def module_suffix(platform):
    return {"win32": ".dll", "darwin": ".dylib"}.get(platform, ".so")


def build_generic(context, *, product_library=None, target_directory=None,
                  profile="release", validate=True):
    """Build the product library and the adapter; return (target, build directory)."""
    metadata = ({"target_directory": str(target_directory)} if target_directory else json.loads(context.runner.run(["cargo", "metadata", "--no-deps", "--locked",
                                              "--format-version", "1"], capture=True)))
    target = pathlib.Path(metadata["target_directory"])
    build = target / "lv2" / (f"windows/generic-{context.name}" if context.cross else f"generic-{context.name}")
    config = "Release" if profile == "release" else "Debug"
    if target_directory:
        build /= profile
    configure = ["cmake", "-S", context.root / "crates/ravet-plugin/lv2", "-B", build,
                 f"-DCMAKE_BUILD_TYPE={config}", f"-DRAVET_LV2_SDK={context.sdk}"]
    platform, release = context.platform, target / "release"
    if context.cross:
        configure += windows_cross.prepare(context.runner, target, platform=context.platform)
        platform, release = "win32", windows_cross.output_directory(target)
    if product_library:
        configure += [f"-DRAVET_PLUGIN_LIBRARY={product_library}", f"-DRAVET_LV2_BUNDLE_NAME={context.name}"]
    elif context.crate:
        cargo = windows_cross.cargo("build") if context.cross else ["cargo", "build"]
        context.runner.run([*cargo, "--release", "--locked", "-p", context.crate])
        library = release / library_name(context.crate.replace("-", "_"), platform)
        configure += [f"-DRAVET_PLUGIN_LIBRARY={library}", f"-DRAVET_LV2_BUNDLE_NAME={context.name}"]
    else:
        configure += ["-DRAVET_PLUGIN_LIBRARY=", "-DRAVET_LV2_BUNDLE_NAME="]
    context.runner.run(configure)
    context.runner.run(["cmake", "--build", build, "--config", config, "--parallel", "2"])
    if validate:
        context.runner.run(["ctest", "--test-dir", build, "-C", config, "--output-on-failure"])
    return target, build


def package_generic(context, target, build, *, sign=True):
    """Copy the assembled bundle, add notices and sign on macOS."""
    source = build / "bundles" / f"{context.name if context.crate else 'ravet_lv2_mock'}.lv2"
    artifact = target / "lv2" / ("windows" if context.cross else "") / \
        f"{context.name if context.crate else 'ravet_mock'}.lv2"
    artifact.parent.mkdir(parents=True, exist_ok=True)
    if artifact.exists():
        shutil.rmtree(artifact)
    shutil.copytree(source, artifact)
    shutil.copy2(context.sdk / "COPYING", artifact / "LV2-SDK-LICENSE.txt")
    shutil.copy2(context.root / "LICENSE", artifact / "RAVET-LICENSE.txt")
    shutil.copy2(context.root / "LICENSING.md", artifact / "RAVET-LICENSING.md")
    if sign and context.platform == "darwin" and not context.cross:
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
