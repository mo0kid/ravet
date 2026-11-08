#!/usr/bin/env python3
"""Fetch Apple's AudioUnitSDK 1.4.0 source for RAVET's AUv2 adapter builds.

The archive is GitHub's zip of the apple/AudioUnitSDK tag AudioUnitSDK-1.4.0
(commit bd98b31feff57a15989fcfab4cd86dc63382b1ac). It is verified against a
SHA-256 pin before it is opened. The pin was computed on 2026-11-06 from a
download whose extracted files matched a git checkout of that commit. Only
include/AudioUnitSDK, src/AudioUnitSDK and LICENSE.txt are extracted, without
the archive's top-level directory, each to a checked relative path. The SDK is
Apache-2.0 licensed (its LICENSE.txt). Use the extracted directory as
RAVET_AUDIO_UNIT_SDK (or [sdks] audio_unit in ravet-sdks.toml).
"""
import argparse
import hashlib
import io
from pathlib import Path, PurePosixPath
import sys
import urllib.request
import zipfile

VERSION = "1.4.0"
URL = f"https://github.com/apple/AudioUnitSDK/archive/refs/tags/AudioUnitSDK-{VERSION}.zip"
SHA256 = "4e7072ae21062671d621c39c41d0feff7cd0bf0f7d8c255b0db0f14e73fa2d62"
# Paths below the archive's single top-level directory to keep.
KEEP_PREFIXES = ("include/AudioUnitSDK/", "src/AudioUnitSDK/")
KEEP_FILES = ("LICENSE.txt",)
# Present only from 1.4, which the adapter requires.
REQUIRED = ("include/AudioUnitSDK/AUScopeElement.h", "src/AudioUnitSDK/AUBase.cpp")


def selected_members(archive):
    """Verify the complete download, then map kept relative paths to their data."""
    if hashlib.sha256(archive).hexdigest() != SHA256:
        raise ValueError("AudioUnitSDK SHA-256 mismatch")
    with zipfile.ZipFile(io.BytesIO(archive)) as zipped:
        members = {}
        for name in zipped.namelist():
            path = PurePosixPath(name)
            if path.is_absolute() or ".." in path.parts or name.endswith("/") or len(path.parts) < 2:
                continue
            relative = PurePosixPath(*path.parts[1:]).as_posix()
            if relative in KEEP_FILES or any(relative.startswith(p) for p in KEEP_PREFIXES):
                members[relative] = zipped.read(name)
    for required in REQUIRED:
        if required not in members:
            raise ValueError(f"AudioUnitSDK archive has no {required}")
    if b"GetBufferListOrError" not in members["include/AudioUnitSDK/AUScopeElement.h"]:
        raise ValueError("AudioUnitSDK archive is older than 1.4 (no GetBufferListOrError)")
    return members


def install(destination):
    with urllib.request.urlopen(URL, timeout=120) as response:
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
