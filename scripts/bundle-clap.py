#!/usr/bin/env python3
"""Build a selected local RAVET reference CLAP bundle (Python 3.11+; no global install).

--target windows cross-compiles the product for x86_64 Windows with cargo-xwin
(C runtime linked statically) into target/clap/windows/<profile>/<Name>.clap.
"""
import argparse
import json
import os
import pathlib
import subprocess
import sys
import tomllib
import windows_cross
from clap_bundle import package_product


# Reference plug-ins: bundle name, Cargo package and bundle identifier. "generic"
# (the gain reference) and "delay" are exported through the generic plug-in layer
# (ravet-plugin).
PLUGINS = {
    "gain": ("RAVET Gain", "ravet-gain-clap", "org.ravet.gain"),
    "sine": ("RAVET Sine", "ravet-sine-clap", "org.ravet.sine"),
    "sidechain": ("RAVET Sidechain", "ravet-sidechain-clap", "org.ravet.sidechain"),
    "generic": ("RAVET Gain Generic", "ravet-gain-plugin", "org.ravet.gain-generic"),
    "delay": ("RAVET Delay", "ravet-delay-plugin", "org.ravet.delay"),
}


def bundle(profile: str, plugin: str = "gain", cross: str | None = None) -> pathlib.Path:
    if plugin not in PLUGINS:
        raise ValueError("unknown reference plug-in")
    if cross not in (None, "windows") or (cross and sys.platform == "win32"):
        raise ValueError("--target windows cross-compiles from Linux or macOS")
    name, crate, identifier = PLUGINS[plugin]
    library_stem = crate.replace("-", "_")
    root = pathlib.Path(__file__).resolve().parents[1]
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"], cwd=root
    ))
    target = pathlib.Path(metadata["target_directory"])
    command = (windows_cross.cargo("build") if cross else ["cargo", "build"]) + ["-p", crate, "--locked", "--jobs", "2"]
    if profile == "release":
        command.append("--release")
    environment = windows_cross.static_crt(dict(os.environ)) if cross else None
    subprocess.run(command, cwd=root, check=True, env=environment)
    output = target / "clap" / ("windows" if cross else "") / profile
    output.mkdir(parents=True, exist_ok=True)
    platform = "win32" if cross else sys.platform
    filename = library_stem + ".dll" if platform == "win32" else "lib" + library_stem + (".dylib" if platform == "darwin" else ".so")
    directory = windows_cross.output_directory(target, profile) if cross else target / profile
    version = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    artifact = package_product(directory / filename, output / f"{name}.clap", {
        "name": name, "plugin_id": identifier, "version": version,
    }, platform)
    if platform == "darwin":
        subprocess.run(["codesign", "--force", "--sign", "-", str(artifact)], check=True)
    return artifact


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=["debug", "release"], default="release")
    parser.add_argument("--plugin", choices=sorted(PLUGINS), default="gain")
    parser.add_argument("--target", choices=["windows"], help="cross-compile for x86_64 Windows with cargo-xwin")
    args = parser.parse_args()
    try:
        print(bundle(args.profile, args.plugin, args.target))
    except ValueError as error:
        parser.error(str(error))
