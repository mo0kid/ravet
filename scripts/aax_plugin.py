"""Build an .aaxplugin for any product through the generic AAX adapter (decision 0281).

The product crate (one that calls ravet_plugin::export_plugin!) is compiled as a
static library with `cargo rustc --crate-type staticlib`, then
crates/ravet-plugin/aax is configured with CMake against an external AAX SDK and
linked with it. The adapter's own mock table is used when no crate is given.
Builds are unsigned: PACE/Avid wrapping is the plug-in vendor's step, outside
RAVET. Nothing is installed into host folders.

cross="windows" (--target windows) cross-compiles the Windows .aaxplugin from
Linux or macOS with cargo-xwin and clang-cl (see windows_cross), with the C
runtime linked statically. The bundle check runs under Wine when installed.
"""
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tomllib
import windows_cross

# Known products: crate (None = the adapter's C++ mock), bundle name, bundle ID.
PRODUCTS = {
    "mock": (None, "RAVET Mock", "org.ravet.mock.aax"),
    "delay": ("ravet-delay-plugin", "RAVET Delay", "org.ravet.delay.aax"),
    "generic": ("ravet-gain-plugin", "RAVET Gain Generic", "org.ravet.gain-generic.aax"),
}
SDK_FILES = ("Interfaces/AAX.h", "Interfaces/AAX_Exports.cpp", "Interfaces/ACF/CACFClassFactory.cpp",
             "Libs/AAXLibrary/source/AAX_CEffectParameters.cpp")


class Runner:
    """Argument-list commands in the repository with two compiler workers."""

    def __init__(self, root):
        self.root = root
        self.environment = dict(os.environ, CARGO_BUILD_JOBS="2")

    def run(self, arguments, *, capture=False, capture_errors=False):
        result = subprocess.run([str(item) for item in arguments], cwd=self.root, env=self.environment,
                                check=True, text=True,
                                stdout=subprocess.PIPE if capture else None,
                                stderr=subprocess.PIPE if capture_errors else None)
        return result.stderr if capture_errors else result.stdout


@dataclass(frozen=True)
class Product:
    key: str
    crate: str | None
    name: str
    identifier: str


@dataclass(frozen=True)
class Built:
    build: Path
    artifact: Path


def product(key, package=None, name=None, identifier=None):
    if package:
        stem = package.removeprefix("ravet-").removesuffix("-plugin")
        return Product(key or stem, package, name or package, identifier or f"org.ravet.{stem}.aax")
    if key not in PRODUCTS:
        raise ValueError(f"unknown AAX product {key!r}; use --package <crate> for another product crate")
    crate, default_name, default_identifier = PRODUCTS[key]
    return Product(key, crate, name or default_name, identifier or default_identifier)


def check_sdk(aax):
    aax = Path(aax).resolve()
    missing = [relative for relative in SDK_FILES if not (aax / relative).exists()]
    if missing:
        raise ValueError(f"{aax} is not a complete AAX SDK; missing {', '.join(missing)}")
    return aax


def native_libraries(stderr):
    """Linker inputs from rustc's `--print native-static-libs` note."""
    found = re.findall(r"native-static-libs:\s*(.*)", stderr)
    return found[-1].strip() if found else ""


def static_library(target, crate, platform):
    stem = crate.replace("-", "_")
    return target / "release" / (f"{stem}.lib" if platform == "win32" else f"lib{stem}.a")


def build(root, aax, selected, *, platform=None, runner=None, jobs=2, cross=None,
          product_library=None, link_libraries="", target_directory=None,
          profile="release", validate=True, version=None):
    platform = platform or sys.platform
    if cross not in (None, "windows"):
        raise ValueError("--target accepts only windows")
    if platform not in ("darwin", "win32") and not cross:
        raise ValueError("AAX plug-ins are built on macOS and Windows only (or --target windows)")
    root = Path(root).resolve()
    aax = check_sdk(aax)
    runner = runner or Runner(root)
    metadata = ({"target_directory": str(target_directory)} if target_directory else
                json.loads(runner.run(["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"], capture=True)))
    target = Path(metadata["target_directory"])
    version = version or tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    config = "Release" if profile == "release" else "Debug"
    build_dir = target / "aax-plugin" / (f"windows-{selected.key}" if cross else selected.key)
    toolchain, release, cargo = [], target, ["cargo", "rustc"]
    if cross:
        toolchain = windows_cross.prepare(runner, target, platform=platform) + \
            [f"-DCMAKE_BUILD_TYPE={config}", "-DRAVET_AAX_STATIC_CRT=ON"]
        platform, release, cargo = "win32", target / windows_cross.TRIPLE, windows_cross.cargo("rustc")
    if target_directory:
        build_dir /= profile
    configure = ["cmake", "-S", root / "crates/ravet-plugin/aax", "-B", build_dir,
                 f"-DRAVET_AAX_SDK={aax}", f"-DRAVET_AAX_NAME={selected.name}",
                 f"-DRAVET_AAX_BUNDLE_ID={selected.identifier}", f"-DRAVET_AAX_VERSION={version}"]
    if product_library:
        configure += [f"-DRAVET_PLUGIN_LIBRARY={product_library}",
                      f"-DRAVET_PLUGIN_LINK_LIBRARIES={link_libraries}"]
    elif selected.crate:
        notes = runner.run([*cargo, "-p", selected.crate, "--lib", "--release", "--locked",
                            "--crate-type", "staticlib", "--", "--print", "native-static-libs"], capture_errors=True)
        library = static_library(release, selected.crate, platform)
        configure += [f"-DRAVET_PLUGIN_LIBRARY={library}", f"-DRAVET_PLUGIN_LINK_LIBRARIES={native_libraries(notes or '')}"]
    if cross:
        configure += toolchain
    else:
        configure += ["-A", "x64"] if platform == "win32" else [f"-DCMAKE_BUILD_TYPE={config}"]
    runner.run(configure)
    runner.run(["cmake", "--build", build_dir, "--config", config, "--parallel", str(jobs)])
    if validate and (not cross or windows_cross.emulator_available(toolchain)):
        runner.run(["ctest", "--test-dir", build_dir, "-C", config, "--output-on-failure"])
    artifact = build_dir / f"{selected.name}.aaxplugin"
    notices = artifact / "Contents" / "Resources" / "notices"
    notices.mkdir(parents=True, exist_ok=True)
    shutil.copy2(root / "LICENSE", notices / "RAVET-LICENSE.txt")
    (notices / "AAX-SDK-NOTICE.txt").write_text(
        "Built against an externally supplied Avid AAX SDK. This build is not PACE-signed: the plug-in "
        "vendor signs (wraps) it under their own Avid agreement. Unsigned builds load only in Pro Tools "
        "Developer. Review the AAX SDK licence and your Avid agreement before distribution.\n")
    return Built(build_dir, artifact)
