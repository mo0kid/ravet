#!/usr/bin/env python3
"""Validate embedded contracts on this host; optional library-only cross builds."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PACKAGES = ["-p", "ravet-foundation", "-p", "ravet-embedded", "-p", "ravet-embedded-gain"]
TARGETS = ["thumbv6m-none-eabi", "thumbv7em-none-eabihf",
           "aarch64-unknown-linux-gnu", "armv7-unknown-linux-gnueabihf"]


def run(command):
    subprocess.run(command, cwd=ROOT, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cross", action="store_true", help="requires the documented rustup targets")
    args = parser.parse_args()
    run(["cargo", "test", *PACKAGES, "--locked"])
    run(["cargo", "test", *PACKAGES, "--no-default-features", "--locked"])
    run(["cargo", "rustc", "-p", "ravet-embedded-gain", "--release", "--locked", "--lib", "--crate-type", "staticlib"])
    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target_dir.is_absolute():
        target_dir = ROOT / target_dir
    with tempfile.TemporaryDirectory(prefix="ravet-embedded-contract-") as temporary:
        cpp = str(Path(temporary) / "arduino")
        run([os.environ.get("CXX", "c++"), "-std=c++11", "-Wall", "-Wextra", "-Werror",
             "-fno-exceptions", "-fno-rtti", "embedded/tests/arduino.cpp", "-o", cpp])
        run([cpp])
        bridge = str(Path(temporary) / "bridge")
        run([os.environ.get("CC", "cc"), "-std=c11", "-Wall", "-Wextra", "-Werror",
             "embedded/tests/bridge.c", str(target_dir / "release/libravet_embedded_gain.a"),
             "-lpthread", "-ldl", "-lm", "-o", bridge])
        run([bridge])
    if args.cross:
        for target in TARGETS:
            run(["cargo", "check", *PACKAGES, "--no-default-features", "--target", target, "--locked"])
        # Standalone C consumer archive, with the explicitly selected panic policy.
        run(["cargo", "rustc", "-p", "ravet-embedded-gain", "--no-default-features",
             "--features", "firmware-panic", "--target", "thumbv7em-none-eabihf",
             "--release", "--locked", "--lib", "--crate-type", "staticlib", "--", "-C", "panic=abort"])
    print("Embedded host contracts passed; cross builds passed." if args.cross else "Embedded host contracts passed.")


if __name__ == "__main__":
    main()
