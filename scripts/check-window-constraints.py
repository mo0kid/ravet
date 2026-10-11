#!/usr/bin/env python3
"""Build and run only owned hidden Cocoa/WebKit content-constraint fixtures."""
import argparse
import platform
from pathlib import Path
import shlex
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("target/window-constraints"))
    parser.add_argument("--runner", default="", help="Optional executable runner command, parsed with shlex; no shell is used.")
    options = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("This explicit native-window fixture requires macOS and Xcode command-line tools")
    architecture = platform.machine()
    if architecture not in ("arm64", "x86_64"):
        parser.error("Unsupported native compiler architecture")
    root = Path(__file__).resolve().parent.parent
    output = options.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    sources = [
        (root / "crates/ravet-surface/native/RavetWebView.mm", "webview"),
        (root / "crates/ravet-webview/native/StandaloneWindow.mm", "window"),
        (root / "crates/ravet-webview/native/tests/content-constraints-observer.mm", "observer"),
    ]
    objects = []
    for source, name in sources:
        obj = output / (name + ".o")
        subprocess.run([
            "xcrun", "--sdk", "macosx", "clang++", "-std=c++17", "-fobjc-arc",
            "-mmacosx-version-min=12.0", "-arch", architecture,
            "-Wall", "-Wextra", "-Werror", "-c", str(source), "-o", str(obj),
        ], check=True)
        objects.append(str(obj))
    library = output / "libravet_webview.a"
    subprocess.run(["xcrun", "ar", "rcs", str(library), *objects], check=True)
    executable = output / "constraints-fixture"
    subprocess.run([
        "rustc", "--edition", "2024", "-D", "warnings",
        str(root / "crates/ravet-webview/native/tests/content-constraints.rs"),
        "-L", "native=" + str(output), "-l", "static=ravet_webview",
        "-l", "framework=Cocoa", "-l", "framework=WebKit", "-l", "c++",
        "-o", str(executable),
    ], check=True)
    subprocess.run([*shlex.split(options.runner), str(executable)], check=True)


if __name__ == "__main__":
    main()
