#!/usr/bin/env python3
"""Fetch the Microsoft.Web.WebView2 1.0.3650.58 NuGet package for hosted Windows CI.

The package is downloaded from nuget.org and verified against a SHA-256 pin
before it is opened. The pin was computed on 2026-11-05 from a download whose
SHA-512 matched the packageHash nuget.org publishes in its catalog entry for
this version. Only the headers, the x64 loader libraries and the licence/notice
files are extracted, each to a checked relative path. The SDK is BSD-3-Clause
licensed (its LICENSE.txt); RAVET's Windows editor builds link its static loader.
Use the extracted directory as RAVET_WEBVIEW2_SDK.
"""
import argparse
import hashlib
import io
from pathlib import Path, PurePosixPath
import sys
import urllib.request
import zipfile

VERSION = "1.0.3650.58"
URL = f"https://www.nuget.org/api/v2/package/Microsoft.Web.WebView2/{VERSION}"
SHA256 = "911a472128c82ac8baa0c486c23342cc9dd6e7dc50d754e676726642ca065c60"
# Archive members to keep: the include directory, the x64 loaders and notices.
KEEP_PREFIXES = ("build/native/include/",)
KEEP_FILES = (
    "build/native/x64/WebView2LoaderStatic.lib",
    "build/native/x64/WebView2Loader.dll",
    "build/native/x64/WebView2Loader.dll.lib",
    "LICENSE.txt",
    "NOTICE.txt",
)


def selected_members(archive):
    """Verify the complete download, then list the members to extract."""
    if hashlib.sha256(archive).hexdigest() != SHA256:
        raise ValueError("WebView2 SDK SHA-256 mismatch")
    with zipfile.ZipFile(io.BytesIO(archive)) as zipped:
        members = {}
        for name in zipped.namelist():
            path = PurePosixPath(name)
            if path.is_absolute() or ".." in path.parts or name.endswith("/"):
                continue
            if name in KEEP_FILES or any(name.startswith(prefix) for prefix in KEEP_PREFIXES):
                members[name] = zipped.read(name)
    if "build/native/include/WebView2.h" not in members:
        raise ValueError("WebView2 SDK archive has no build/native/include/WebView2.h")
    if "build/native/x64/WebView2LoaderStatic.lib" not in members:
        raise ValueError("WebView2 SDK archive has no x64 WebView2LoaderStatic.lib")
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
