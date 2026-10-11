#!/usr/bin/env python3
"""Build/run only owned native semantic protocol and Rust/WebKit fixtures."""
import argparse
import platform
from pathlib import Path
import shlex
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("target/native-semantics"))
    parser.add_argument("--runner", default="", help="Optional executable runner, parsed with shlex; no shell.")
    options = parser.parse_args()
    if sys.platform != "darwin" or platform.machine() not in ("arm64", "x86_64"):
        parser.error("These owned native fixtures require macOS and Xcode command-line tools")
    root = Path(__file__).resolve().parent.parent
    output = options.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    sources = [
        (root / "crates/ravet-surface/native/RavetWebView.mm", "webview"),
        (root / "crates/ravet-webview/native/StandaloneWindow.mm", "window"),
        (root / "crates/ravet-webview/native/tests/native-semantics-consumer.mm", "consumer"),
    ]
    compiler = ["xcrun", "--sdk", "macosx", "clang++", "-std=c++17", "-fobjc-arc",
                "-mmacosx-version-min=12.0", "-arch", platform.machine(), "-Wall", "-Wextra", "-Werror"]
    objects = []
    for source, name in sources:
        obj = output / (name + ".o")
        # Private revision exhaustion seam is excluded from normal library builds.
        subprocess.run([*compiler, "-DRAVET_ACCESSIBILITY_TESTING", "-c", str(source), "-o", str(obj)], check=True)
        objects.append(str(obj))
    library = output / "libravet_webview.a"
    subprocess.run(["xcrun", "ar", "rcs", str(library), *objects], check=True)
    native = output / "native-protocol"
    subprocess.run([*compiler, str(root / "crates/ravet-webview/native/tests/native-semantics-protocol.mm"),
                    objects[0], "-framework", "Cocoa", "-framework", "WebKit", "-o", str(native)], check=True)
    runner = shlex.split(options.runner)
    subprocess.run([*runner, str(native)], check=True)
    source = root / "crates/ravet-webview/native/tests/native-semantics-owner.rs"
    common = ["rustc", "--edition", "2024", "-D", "warnings", str(source),
              "-L", "native=" + str(output), "-l", "static=ravet_webview",
              "-l", "framework=Cocoa", "-l", "framework=WebKit", "-l", "c++"]
    units = output / "semantic-units"
    subprocess.run([*common, "--test", "-o", str(units)], check=True)
    subprocess.run([*runner, str(units)], check=True)
    executable = output / "semantic-consumer"
    subprocess.run([*common, "-o", str(executable)], check=True)
    subprocess.run([*runner, str(executable), "--fixture"], check=True)


if __name__ == "__main__":
    main()
