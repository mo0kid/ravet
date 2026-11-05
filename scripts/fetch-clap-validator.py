#!/usr/bin/env python3
"""Fetch clap-validator 0.4.1 for hosted macOS/Windows CI; verify before unpacking.

SHA-256 pins match the release asset digests published by GitHub's release API
for https://github.com/free-audio/clap-validator/releases/tag/0.4.1 on 2026-11-05.
Only the named executable is copied; archive paths are never extracted.
"""
import argparse
import hashlib
import io
from pathlib import Path
import sys
import tarfile
import urllib.request
import zipfile

RELEASE = "https://github.com/free-audio/clap-validator/releases/download/0.4.1/"
ASSETS = {
    "darwin": (
        "clap-validator-0.4.1-127-g152b982-macos-universal.zip",
        "bbec8cd7d18274e549d5d8c12ece3cec54be966129388dd2e742b9957f2ba9f1",
        "clap-validator",
    ),
    "win32": (
        "clap-validator-0.4.1-127-g152b982-windows.zip",
        "d935c3af0a45c3911ea2e900f4aa5d6709dac82bb485f0c4ce28648ab2cd0c10",
        "clap-validator.exe",
    ),
}


def executable_bytes(archive, platform):
    """Verify the complete download before opening either archive layer."""
    _, expected, binary = ASSETS[platform]
    if hashlib.sha256(archive).hexdigest() != expected:
        raise ValueError("clap-validator SHA-256 mismatch")
    with zipfile.ZipFile(io.BytesIO(archive)) as zipped:
        if platform == "win32":
            return zipped.read(binary)
        # Upstream's 0.4.1 universal ZIP contains a tarball named 0.3.2.
        nested = "clap-validator-0.3.2-127-g152b982-macos-universal.tar.gz"
        with tarfile.open(fileobj=io.BytesIO(zipped.read(nested)), mode="r:gz") as tar:
            member = tar.getmember("binaries/clap-validator")
            if not member.isfile():
                raise ValueError("clap-validator archive member is not a regular file")
            with tar.extractfile(member) as source:
                return source.read()


def install(destination, platform=sys.platform):
    if platform not in ASSETS:
        raise ValueError("this CI helper supports macOS and Windows only")
    asset, _, binary = ASSETS[platform]
    with urllib.request.urlopen(RELEASE + asset, timeout=60) as response:
        archive = response.read()
    payload = executable_bytes(archive, platform)
    destination.mkdir(parents=True, exist_ok=True)
    executable = destination / binary
    executable.write_bytes(payload)
    executable.chmod(0o755)
    return executable


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    print(install(args.destination))
