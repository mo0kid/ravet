#!/usr/bin/env python3
"""Fetch Steinberg's VST 3 SDK 3.8.1 (build 84) for RAVET's VST3 adapter builds.

The archive is Steinberg's official download, vst-sdk_3.8.1_build-84_2026-08-11.zip,
which https://www.steinberg.net/vst3sdk redirected to on 2026-11-06. It is
verified against a SHA-256 pin before it is opened. The pin was computed from a
download whose extracted SDK matched a git checkout of steinbergmedia/vst3sdk
tag v3.8.1_build_84 (commit 3cdf9ca5d1f5b1b21e0a86832aa4abe55607bd96) with its
submodules at that tag. Only what RAVET builds from is extracted (the CMake
project, base, cmake, pluginterfaces, public.sdk and the licence), without the
archive's VST_SDK/vst3sdk prefix, each to a checked relative path. VSTGUI,
documentation, tutorials and prebuilt binaries are left out. The SDK is MIT
licensed (its LICENSE.txt). Use the extracted directory as RAVET_VST3_SDK (or
[sdks] vst3 in ravet-sdks.toml).
"""
import argparse
import hashlib
import io
from pathlib import Path, PurePosixPath
import sys
import urllib.request
import zipfile

VERSION = "3.8.1_build-84_2026-08-11"
URL = f"https://download.steinberg.net/sdk_downloads/vst-sdk_{VERSION}.zip"
SHA256 = "64965f1b74e08a6d4087a35af7a716f4dcff5852c66ad7ee13f1c47e79c1ab77"
ROOT = "VST_SDK/vst3sdk/"
# Paths below ROOT to keep.
KEEP_PREFIXES = ("base/", "cmake/", "pluginterfaces/", "public.sdk/")
KEEP_FILES = ("CMakeLists.txt", "LICENSE.txt", "README.md")
REQUIRED = (
    "CMakeLists.txt",
    "LICENSE.txt",
    "pluginterfaces/vst/ivstaudioprocessor.h",
    "public.sdk/source/vst/vstaudioeffect.cpp",
    "public.sdk/samples/vst-hosting/validator/CMakeLists.txt",
)


def selected_members(archive):
    """Verify the complete download, then map kept relative paths to their data."""
    if hashlib.sha256(archive).hexdigest() != SHA256:
        raise ValueError("VST3 SDK SHA-256 mismatch")
    with zipfile.ZipFile(io.BytesIO(archive)) as zipped:
        members = {}
        for info in zipped.infolist():
            name = info.filename
            path = PurePosixPath(name)
            if (path.is_absolute() or ".." in path.parts or name.endswith("/")
                    or not name.startswith(ROOT) or info.external_attr >> 28 == 0o12):
                continue  # Directories, links and anything outside the SDK root.
            relative = name[len(ROOT):]
            if relative in KEEP_FILES or any(relative.startswith(p) for p in KEEP_PREFIXES):
                members[relative] = zipped.read(info)
    for required in REQUIRED:
        if required not in members:
            raise ValueError(f"VST3 SDK archive has no {required}")
    return members


def install(destination):
    with urllib.request.urlopen(URL, timeout=600) as response:
        archive = response.read()
    members = selected_members(archive)
    destination = destination.resolve()
    for name, data in members.items():
        target = (destination / name).resolve()
        if destination not in target.parents:
            raise ValueError(f"refusing to write outside the destination: {name}")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    return destination


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args(argv)
    print(install(args.destination))


if __name__ == "__main__":
    sys.exit(main())
