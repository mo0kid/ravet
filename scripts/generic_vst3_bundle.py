"""Build the runtime-table VST3 adapter with a product cdylib or C++ fixture."""
import json
import re
import shutil
import sys
from pathlib import Path
import third_party_notices
from vst3_bundle import CommandRunner
import windows_cross


def build_generic(root, sdk, plugin="gain", profile="release", *, mock=False,
                  manifest_path=None, package=None, runner=None, platform=None, cross=None,
                  product_library=None, target_directory=None, validate=True, sign=True, run_validation=True, bundle_name=None, cmake_options=()):
    """Build only the selected product; package locally and run SDK validation.

    cross="windows" cross-compiles for x86_64 Windows (see windows_cross); the
    SDK validator and moduleinfotool are then built for Windows but not run.
    """
    root, sdk = Path(root).resolve(), Path(sdk).resolve()
    platform = platform or sys.platform
    runner = runner or CommandRunner(root)
    if cross not in (None, "windows"):
        raise ValueError("--target accepts only windows")
    if not re.fullmatch(r"[A-Za-z0-9_-]+", plugin):
        raise ValueError("plugin must be a simple product name")
    if profile not in ("debug", "release") or platform not in ("darwin", "linux", "win32"):
        raise ValueError("select debug/release on macOS, Windows or Linux")
    for relative in ("LICENSE.txt", "CMakeLists.txt", "pluginterfaces/vst/ivstaudioprocessor.h"):
        if not (sdk / relative).is_file():
            raise ValueError(f"complete external VST3 SDK is missing {relative}")
    metadata_args = ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"]
    if manifest_path:
        metadata_args += ["--manifest-path", Path(manifest_path).resolve()]
    metadata = ({"target_directory": str(target_directory)} if target_directory else
                json.loads(runner.run(metadata_args, capture=True)))
    target = Path(metadata["target_directory"])
    config = "Release" if profile == "release" else "Debug"
    name = bundle_name or ("ravet_mock_vst3" if mock else f"ravet_{plugin}_vst3")
    if not re.fullmatch(r"[A-Za-z0-9_-]+", name):
        raise ValueError("VST3 bundle name must be a simple product name")
    build = target / "vst3/generic" / ("mock" if mock else plugin) / profile
    toolchain = []
    if cross:
        toolchain = windows_cross.prepare(runner, target, platform=platform) + ["-DSMTG_USE_STATIC_CRT=ON"]
        platform, build = "win32", build.with_name(f"windows-{profile}")
    if not mock and product_library is None:
        package = package or f"ravet-{plugin}-plugin"
        matches = [p for p in metadata["packages"] if p["name"] == package]
        if len(matches) != 1:
            raise ValueError(f"product package {package!r} not found; use --package/--manifest-path")
        libraries = [t for t in matches[0]["targets"] if "cdylib" in t["crate_types"]]
        if len(libraries) != 1:
            raise ValueError("product must declare exactly one cdylib target")
        cargo = (windows_cross.cargo("build") if cross else ["cargo", "build"]) + ["--locked", "-p", package]
        if manifest_path:
            cargo += ["--manifest-path", Path(manifest_path).resolve()]
        if profile == "release":
            cargo += ["--release"]
        runner.run(cargo)
        libname = libraries[0]["name"].replace("-", "_")
        filename = libname + ".dll" if platform == "win32" else "lib" + libname + (".dylib" if platform == "darwin" else ".so")
        product_library = (windows_cross.output_directory(target, profile) if cross else target / profile) / filename
    runner.run(["cmake", "-S", root / "crates/ravet-plugin/adapters/vst3", "-B", build,
                f"-DCMAKE_BUILD_TYPE={config}", f"-DRAVET_VST3_SDK={sdk}",
                f"-DRAVET_VST3_NAME={name}", f"-DRAVET_VST3_MOCK={'ON' if mock else 'OFF'}",
                f"-DRAVET_PRODUCT_LIBRARY={product_library or ''}", *toolchain, *cmake_options])
    targets = [name] + (["validator"] if validate else []) + (["moduleinfotool"] if cross else []) + \
        (["ravet_generic_vst3_contracts"] if mock else [])
    runner.run(["cmake", "--build", build, "--config", config, "--target", *targets, "--parallel", "2"])
    if mock and (not cross or windows_cross.emulator_available(toolchain)):
        runner.run(["ctest", "--test-dir", build, "-C", config, "--output-on-failure", "-R", "^ravet_generic_vst3_contracts$"])
    artifacts = sorted(p for p in build.rglob(name + ".vst3") if p.is_dir())
    if not artifacts:
        raise RuntimeError("generic VST3 build produced no bundle")
    for artifact in artifacts:
        notices = artifact / "Contents/Resources/notices"
        notices.mkdir(parents=True, exist_ok=True)
        shutil.copy2(sdk / "LICENSE.txt", notices / "Steinberg-VST3-LICENSE.txt")
        shutil.copy2(root / "LICENSE", notices / "RAVET-LICENSE.txt")
        if product_library:
            third_party_notices.write(root, package, notices, windows_cross.TRIPLE if cross else None,
                                      manifest_path=Path(manifest_path).resolve() if manifest_path else None,
                                      run=runner.run)
        if platform == "darwin" and sign:
            # Sign the nested product image before sealing the enclosing bundle.
            if product_library:
                runner.run(["codesign", "--force", "--sign", "-", artifact / "Contents/MacOS" / product_library.name])
            runner.run(["codesign", "--force", "--sign", "-", artifact])
        if cross:
            # Single-configuration cross builds put the SDK tools in bin/.
            print(f"cross-built; run on Windows: {build / 'bin/validator.exe'} <bundle>", file=sys.stderr)
            continue
        if not validate or not run_validation:
            continue
        validator = build / "bin" / config / ("validator.exe" if platform == "win32" else "validator")
        runner.run([validator, artifact])
    return artifacts
