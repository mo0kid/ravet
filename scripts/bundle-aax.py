#!/usr/bin/env python3
"""Build a local AAX gain reference; no host installation or PACE signing."""
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vst3-sdk", type=pathlib.Path, required=True)
    parser.add_argument("--aax-sdk", type=pathlib.Path, required=True)
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("this build/entry-point probe currently requires macOS")
    root = pathlib.Path(__file__).resolve().parents[1]
    sdk, aax = args.vst3_sdk.resolve(), args.aax_sdk.resolve()
    if not (aax / "Interfaces/AAX.h").is_file():
        parser.error("supply a complete externally installed AAX SDK")
    subprocess.run(["cargo", "build", "-p", "ravet-gain-native", "--release", "--locked"], cwd=root, env=dict(os.environ, CARGO_BUILD_JOBS="2"), check=True)
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"], cwd=root))
    target = pathlib.Path(metadata["target_directory"])
    build = target / "aax/release"
    subprocess.run(["cmake", "-S", str(root / "examples/gain-vst3"), "-B", str(build), "-DCMAKE_BUILD_TYPE=Release", f"-DRAVET_VST3_SDK={sdk}", f"-DRAVET_AAX_SDK={aax}", f"-DRAVET_GAIN_LIBRARY={target / 'release/libravet_gain_native.a'}"], check=True)
    subprocess.run(["cmake", "--build", str(build), "--target", "ravet_gain_aax", "ravet_vst3_contracts", "--parallel", "2"], check=True)
    subprocess.run(["ctest", "--test-dir", str(build), "--output-on-failure"], check=True)
    artifact = build / "AAXPLUGIN/Release/ravet_gain_aax.aaxplugin"
    notices = artifact / "Contents/Resources/notices"
    notices.mkdir(parents=True, exist_ok=True)
    shutil.copy2(sdk / "LICENSE.txt", notices / "Steinberg-VST3-LICENSE.txt")
    shutil.copy2(root / "LICENSE", notices / "RAVET-LICENSE.txt")
    # The installed AAX SDK declares its terms in its headers, not a root licence file.
    (notices / "AAX-SDK-NOTICE.txt").write_text("Built against an externally supplied Avid AAX SDK. Review its header licence and your Avid agreement before distribution. This local artifact is not a production release.\n")
    subprocess.run(["codesign", "--force", "--sign", "-", str(artifact)], check=True)
    subprocess.run([sys.executable, str(root / "tools/aax-validation/check-bundle.py"), str(artifact)], check=True)
    print(artifact)


if __name__ == "__main__":
    main()
