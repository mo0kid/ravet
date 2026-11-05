"""Build the runtime-table VST3 adapter with a product cdylib or C++ fixture."""
import json
import re
import shutil
import sys
from pathlib import Path
from vst3_bundle import CommandRunner


def build_generic(root, sdk, plugin="gain", profile="release", *, mock=False,
                  manifest_path=None, package=None, runner=None, platform=None):
    """Build only the selected product; package locally and run SDK validation."""
    root, sdk = Path(root).resolve(), Path(sdk).resolve()
    platform = platform or sys.platform
    runner = runner or CommandRunner(root)
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
    metadata = json.loads(runner.run(metadata_args, capture=True))
    target = Path(metadata["target_directory"])
    config = "Release" if profile == "release" else "Debug"
    name = "ravet_mock_vst3" if mock else f"ravet_{plugin}_vst3"
    build = target / "vst3/generic" / ("mock" if mock else plugin) / profile
    product_library = None
    if not mock:
        package = package or f"ravet-{plugin}-plugin"
        matches = [p for p in metadata["packages"] if p["name"] == package]
        if len(matches) != 1:
            raise ValueError(f"product package {package!r} not found; use --package/--manifest-path")
        libraries = [t for t in matches[0]["targets"] if "cdylib" in t["crate_types"]]
        if len(libraries) != 1:
            raise ValueError("product must declare exactly one cdylib target")
        cargo = ["cargo", "build", "--locked", "-p", package]
        if manifest_path:
            cargo += ["--manifest-path", Path(manifest_path).resolve()]
        if profile == "release":
            cargo += ["--release"]
        runner.run(cargo)
        libname = libraries[0]["name"].replace("-", "_")
        filename = libname + ".dll" if platform == "win32" else "lib" + libname + (".dylib" if platform == "darwin" else ".so")
        product_library = target / profile / filename
    runner.run(["cmake", "-S", root / "crates/ravet-plugin/adapters/vst3", "-B", build,
                f"-DCMAKE_BUILD_TYPE={config}", f"-DRAVET_VST3_SDK={sdk}",
                f"-DRAVET_VST3_NAME={name}", f"-DRAVET_VST3_MOCK={'ON' if mock else 'OFF'}",
                f"-DRAVET_PRODUCT_LIBRARY={product_library or ''}"])
    targets = [name, "validator"] + (["ravet_generic_vst3_contracts"] if mock else [])
    runner.run(["cmake", "--build", build, "--config", config, "--target", *targets, "--parallel", "2"])
    if mock:
        runner.run(["ctest", "--test-dir", build, "-C", config, "--output-on-failure", "-R", "^ravet_generic_vst3_contracts$"])
    artifacts = sorted(p for p in build.rglob(name + ".vst3") if p.is_dir())
    if not artifacts:
        raise RuntimeError("generic VST3 build produced no bundle")
    for artifact in artifacts:
        notices = artifact / "Contents/Resources/notices"
        notices.mkdir(parents=True, exist_ok=True)
        shutil.copy2(sdk / "LICENSE.txt", notices / "Steinberg-VST3-LICENSE.txt")
        shutil.copy2(root / "LICENSE", notices / "RAVET-LICENSE.txt")
        if platform == "darwin":
            # Sign the nested product image before sealing the enclosing bundle.
            if product_library:
                runner.run(["codesign", "--force", "--sign", "-", artifact / "Contents/MacOS" / product_library.name])
            runner.run(["codesign", "--force", "--sign", "-", artifact])
        validator = build / "bin" / config / ("validator.exe" if platform == "win32" else "validator")
        runner.run([validator, artifact])
    return artifacts
