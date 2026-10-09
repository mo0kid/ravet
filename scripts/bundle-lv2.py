#!/usr/bin/env python3
"""Build the public mono/stereo LV2 reference using external LV2 headers."""
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=pathlib.Path, required=True)
    args = parser.parse_args()
    if sys.platform not in ("darwin", "linux"):
        parser.error("current binary loading fixture supports macOS/Linux only")
    root = pathlib.Path(__file__).resolve().parents[1]
    sdk = args.sdk.resolve()
    if not (sdk / "include/lv2/core/lv2.h").is_file():
        parser.error("supply an external LV2 source SDK with include/lv2/core/lv2.h")
    subprocess.run(["cargo", "build", "-p", "ravet-gain-native", "--release", "--locked"], cwd=root, env=dict(os.environ, CARGO_BUILD_JOBS="2"), check=True)
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"], cwd=root))
    target = pathlib.Path(metadata["target_directory"])
    build = target / "lv2/release"
    subprocess.run(["cmake", "-S", str(root / "examples/gain-lv2"), "-B", str(build), "-DCMAKE_BUILD_TYPE=Release", f"-DRAVET_LV2_SDK={sdk}", f"-DRAVET_GAIN_LIBRARY={target / 'release/libravet_gain_native.a'}"], check=True)
    subprocess.run(["cmake", "--build", str(build), "--parallel", "2"], check=True)
    artifact = target / "lv2/ravet_gain.lv2"
    artifact.mkdir(parents=True, exist_ok=True)
    for name in ("manifest.ttl", "gain.ttl"):
        shutil.copy2(root / "examples/gain-lv2" / name, artifact / name)
    shutil.copy2(build / "ravet_gain_lv2.so", artifact / "ravet_gain_lv2.so")
    shutil.copy2(sdk / "COPYING", artifact / "LV2-SDK-LICENSE.txt")
    shutil.copy2(root / "LICENSE", artifact / "RAVET-LICENSE.txt")
    if sys.platform == "darwin":
        subprocess.run(["codesign", "--force", "--sign", "-", str(artifact / "ravet_gain_lv2.so")], check=True)
    subprocess.run([str(build / "ravet_lv2_contracts"), str(artifact / "ravet_gain_lv2.so")], check=True)
    print(artifact)


if __name__ == "__main__":
    main()
