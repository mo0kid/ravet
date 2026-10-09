#!/usr/bin/env python3
"""Build a local AUv2 gain component over shared VST3/Rust DSP; no host installation."""
import argparse
import pathlib
import platform
import shutil
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vst3-sdk", type=pathlib.Path, required=True)
    parser.add_argument("--audio-unit-sdk", type=pathlib.Path, required=True)
    parser.add_argument("--native-editor", action="store_true", help="Embed the macOS gain WebKit editor and validate its AU Cocoa view")
    parser.add_argument("--factory-programs", action="store_true", help="Include correctly indexed complete gain factory presets")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("AudioUnit build requires macOS and Xcode")
    root = pathlib.Path(__file__).resolve().parents[1]
    vst3 = args.vst3_sdk.resolve()
    ausdk = args.audio_unit_sdk.resolve()
    if not (ausdk / "AudioUnitSDK.xcodeproj").is_dir():
        parser.error("AudioUnit SDK must include its external Xcode project")
    vst3_command = [sys.executable, str(root / "scripts/bundle-vst3.py"), "--sdk", str(vst3)]
    if args.native_editor:
        vst3_command.append("--native-editor")
    if args.factory_programs:
        vst3_command.append("--factory-programs")
    subprocess.run(vst3_command, check=True)
    sdk_build = root / "target/au/sdk"
    subprocess.run(["xcodebuild", "-project", str(ausdk / "AudioUnitSDK.xcodeproj"), "-target", "AudioUnitSDK", "-configuration", "Release", "-jobs", "2", f"SYMROOT={sdk_build}", f"OBJROOT={root / 'target/au/sdk-obj'}", f"ARCHS={platform.machine()}", "ONLY_ACTIVE_ARCH=YES", "CODE_SIGNING_ALLOWED=NO", "MACOSX_DEPLOYMENT_TARGET=12.0"], check=True)
    build = root / "target/vst3/release"
    subprocess.run(["cmake", "-S", str(root / "examples/gain-vst3"), "-B", str(build), f"-DRAVET_AUDIOUNIT_SDK={ausdk}", f"-DRAVET_AUDIOUNIT_LIBRARY={sdk_build / 'Release/libAudioUnitSDK.a'}"], check=True)
    targets = ["ravet_gain_au", "ravet_au_contracts"]
    if args.native_editor:
        targets.append("ravet_au_editor_contracts")
    if args.factory_programs:
        targets.append("ravet_au_program_contracts")
    subprocess.run(["cmake", "--build", str(build), "--target", *targets, "--parallel", "2"], check=True)
    artifact = build / "AU/ravet_gain_au.component"
    resources = artifact / "Contents/Resources"
    resources.mkdir(parents=True, exist_ok=True)
    native_plugin = build / "VST3/Release/ravet_gain_vst3.vst3"
    embedded = resources / "plugin.vst3"
    # Replace only this generated nested reference artifact.
    if embedded.exists():
        shutil.rmtree(embedded)
    shutil.copytree(native_plugin, embedded)
    notices = resources / "notices"
    notices.mkdir(exist_ok=True)
    shutil.copy2(ausdk / "LICENSE.txt", notices / "Apple-AudioUnitSDK-LICENSE.txt")
    shutil.copy2(vst3 / "LICENSE.txt", notices / "Steinberg-VST3-LICENSE.txt")
    shutil.copy2(root / "LICENSE", notices / "RAVET-LICENSE.txt")
    subprocess.run(["codesign", "--force", "--sign", "-", str(artifact)], check=True)
    subprocess.run([str(build / "ravet_au_contracts"), str(artifact)], check=True)
    if args.native_editor:
        subprocess.run([str(build / "ravet_au_editor_contracts"), str(artifact)], check=True)
    if args.factory_programs:
        subprocess.run([str(build / "ravet_au_program_contracts"), str(artifact)], check=True)
    print(artifact)


if __name__ == "__main__":
    main()
