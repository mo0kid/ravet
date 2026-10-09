#!/usr/bin/env python3
"""Build a selected local RAVET reference CLAP bundle (Python 3.11+; no global install)."""
import argparse
import json
import pathlib
import plistlib
import shutil
import subprocess
import sys
import tomllib


def bundle(profile: str, plugin: str = "gain") -> pathlib.Path:
    if plugin not in {"gain", "sine", "sidechain"}:
        raise ValueError("unknown reference plug-in")
    name = f"RAVET {plugin.title()}"
    crate = f"ravet-{plugin}-clap"
    library_stem = crate.replace("-", "_")
    root = pathlib.Path(__file__).resolve().parents[1]
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"], cwd=root
    ))
    target = pathlib.Path(metadata["target_directory"])
    command = ["cargo", "build", "-p", crate, "--locked", "--jobs", "2"]
    if profile == "release":
        command.append("--release")
    subprocess.run(command, cwd=root, check=True)
    output = target / "clap" / profile
    output.mkdir(parents=True, exist_ok=True)
    if sys.platform == "darwin":
        artifact = output / f"{name}.clap"
        contents = artifact / "Contents"
        executable = contents / "MacOS" / library_stem
        executable.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(target / profile / f"lib{library_stem}.dylib", executable)
        version = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
        with (contents / "Info.plist").open("wb") as destination:
            plistlib.dump({
                "CFBundleExecutable": executable.name,
                "CFBundleIdentifier": f"org.ravet.{plugin}",
                "CFBundleName": name,
                "CFBundlePackageType": "BNDL",
                "CFBundleVersion": version,
                "CFBundleShortVersionString": version,
            }, destination)
        subprocess.run(["codesign", "--force", "--sign", "-", str(artifact)], check=True)
    else:
        library = f"{library_stem}.dll" if sys.platform == "win32" else f"lib{library_stem}.so"
        artifact = output / f"{name}.clap"
        shutil.copy2(target / profile / library, artifact)
    return artifact


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=["debug", "release"], default="release")
    parser.add_argument("--plugin", choices=["gain", "sine", "sidechain"], default="gain")
    args = parser.parse_args()
    print(bundle(args.profile, args.plugin))
