#!/usr/bin/env python3
"""Temporarily install and auval a test-only native AU mock; always remove it."""
import argparse
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("component", type=Path)
    parser.add_argument("--log", type=Path, help="Save exact auval output")
    args = parser.parse_args()
    source = args.component.resolve()
    with (source / "Contents/Info.plist").open("rb") as stream:
        plist = plistlib.load(stream)
    info = plist["AudioComponents"][0]
    if not plist["CFBundleIdentifier"].startswith("org.ravet.test.auv2.") or "AU TEST" not in info["name"] or \
            info["manufacturer"] != "RvTs" or (info["type"], info["subtype"]) not in {
                ("aufx", "RvT1"), ("aumu", "RvI1"), ("aumf", "RvM1")}:
        parser.error("only the RAVET AU TEST mock identities may be installed by this tool")
    destination = Path.home() / "Library/Audio/Plug-Ins/Components" / ("RAVET AU TEST " + info["subtype"] + ".component")
    if destination.exists():
        parser.error(f"refusing to overwrite {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, destination)
    try:
        command = ["auval", "-v", info["type"], info["subtype"], info["manufacturer"]]
        # AudioComponentRegistrar discovers directory changes asynchronously.
        # Retry only discovery failures; never hide an actual validator failure.
        for attempt in range(6):
            result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=120)
            if "didn't find the component" not in result.stdout or attempt == 5:
                break
            time.sleep(2)
        if args.log:
            args.log.parent.mkdir(parents=True, exist_ok=True)
            args.log.write_text(result.stdout)
        print(result.stdout, end="")
        print("Command:", " ".join(command), "exit:", result.returncode)
        return result.returncode
    finally:
        shutil.rmtree(destination)
        print("Removed test installation:", destination)


if __name__ == "__main__":
    sys.exit(main())
