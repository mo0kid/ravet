#!/usr/bin/env python3
"""Build the public gain VST3 prototype using an explicit external SDK; no installation."""
import argparse
import json
import os
import pathlib
import subprocess
import shutil
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=pathlib.Path, required=True)
    parser.add_argument("--profile", choices=["debug", "release"], default="release")
    parser.add_argument("--native-editor", action="store_true", help="Include the macOS Rust-owned WebKit editor")
    parser.add_argument("--factory-programs", action="store_true", help="Include the optional macOS validated gain program list/selector")
    args = parser.parse_args()
    if args.native_editor and sys.platform != "darwin":
        parser.error("native editor currently requires macOS")
    if args.factory_programs and sys.platform != "darwin":
        parser.error("factory program validation currently requires macOS")
    root = pathlib.Path(__file__).resolve().parents[1]
    sdk = args.sdk.resolve()
    if not (sdk / "LICENSE.txt").is_file() or not (sdk / "pluginterfaces/vst/ivstaudioprocessor.h").is_file():
        parser.error("SDK must be a complete externally installed VST3 SDK")
    build_env = dict(os.environ, CARGO_BUILD_JOBS="2")
    cargo = ["cargo", "build", "-p", "ravet-gain-native", "--locked"]
    if args.native_editor:
        cargo.extend(["--features", "native-editor"])
    if args.profile == "release":
        cargo.append("--release")
    subprocess.run(cargo, cwd=root, env=build_env, check=True)
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"], cwd=root))
    target = pathlib.Path(metadata["target_directory"])
    build = target / "vst3" / args.profile
    config = "Release" if args.profile == "release" else "Debug"
    library = target / args.profile / ("ravet_gain_native.lib" if sys.platform == "win32" else "libravet_gain_native.a")
    subprocess.run(["cmake", "-S", str(root / "examples/gain-vst3"), "-B", str(build), f"-DCMAKE_BUILD_TYPE={config}", f"-DRAVET_VST3_SDK={sdk}", f"-DRAVET_GAIN_LIBRARY={library}", f"-DRAVET_GAIN_EDITOR={'ON' if args.native_editor else 'OFF'}", f"-DRAVET_GAIN_PROGRAMS={'ON' if args.factory_programs else 'OFF'}"], check=True)
    targets = ["ravet_gain_vst3", "validator", "ravet_vst3_contracts"]
    if args.native_editor:
        targets.append("ravet_vst3_editor_contracts")
    if args.factory_programs:
        targets.append("ravet_vst3_program_contracts")
    subprocess.run(["cmake", "--build", str(build), "--config", config, "--target", *targets, "--parallel", "2"], check=True)
    subprocess.run(["ctest", "--test-dir", str(build), "-C", config, "--output-on-failure"], check=True)
    artifacts = list(build.rglob("ravet_gain_vst3.vst3"))
    if not artifacts:
        raise RuntimeError("VST3 build produced no bundle")
    validator = build / "bin" / config / ("validator.exe" if sys.platform == "win32" else "validator")
    for artifact in artifacts:
        notices = artifact / "Contents" / "Resources" / "notices"
        notices.mkdir(parents=True, exist_ok=True)
        shutil.copy2(sdk / "LICENSE.txt", notices / "Steinberg-VST3-LICENSE.txt")
        shutil.copy2(root / "LICENSE", notices / "RAVET-LICENSE.txt")
        if sys.platform == "darwin":
            subprocess.run(["codesign", "--force", "--sign", "-", str(artifact)], check=True)
        subprocess.run([str(validator), str(artifact)], check=True)
        if args.native_editor:
            editor_probe = build / "ravet_vst3_editor_contracts"
            subprocess.run([str(editor_probe), str(artifact)], check=True)
        if args.factory_programs:
            subprocess.run([str(build / "ravet_vst3_program_contracts"), str(artifact)], check=True)
        print(artifact)


if __name__ == "__main__":
    main()
