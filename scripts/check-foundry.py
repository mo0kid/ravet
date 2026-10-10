#!/usr/bin/env python3
"""Exercise Foundry's CLI in disposable external consumers (Python 3.11+).

--build adds offline consumer builds, the runnable offline fixture and the macOS
standalone native UI/audio fixture. This does not install plug-ins or start devices.
SDK options add native packages, metadata/signature checks and an optional VST3
validator. Build Foundry first; pass --executable for a separate Cargo target directory.
"""
import argparse
import ctypes
import plistlib
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib


def run(command, cwd=None):
    print("Running:", " ".join(map(str, command)), flush=True)
    subprocess.run(command, cwd=cwd, check=True)


def check(executable: Path, build: bool, sdks: dict[str, Path], auv3: bool,
          vst3_validator: Path | None):
    with tempfile.TemporaryDirectory(prefix="ravet-foundry-consumers-") as directory:
        parent = Path(directory)
        for kind in ("offline", "standalone", "plugin", "instrument"):
            template = "plugin" if kind == "instrument" else kind
            destination = parent / f"{kind} consumer"
            name = f"foundry-{kind}-fixture"
            formats = ["clap"]
            if kind in ("plugin", "instrument"):
                if "vst3" in sdks:
                    formats.append("vst3")
                    if "audio-unit" in sdks and sys.platform == "darwin":
                        formats.append("au")
                    if "aax" in sdks and kind == "plugin" and sys.platform in ("darwin", "win32"):
                        formats.append("aax")
                if "lv2" in sdks:
                    formats.append("lv2")
                if auv3 and kind == "plugin" and sys.platform == "darwin":
                    formats.append("auv3")
            options = (["--type", "instrument"] if kind == "instrument" else [])
            if kind in ("plugin", "instrument"):
                options += ["--formats", ",".join(formats)]
                for sdk, path in sdks.items():
                    options += [f"--{sdk}-sdk", str(path)]
            run([str(executable), "new", str(destination), name, template, "GPL-3.0-only"] + options)
            run([str(executable), "open", str(destination / "ravet-project.toml")])
            source = destination / "src" / ("lib.rs" if kind in ("plugin", "instrument") else "main.rs")
            edited_source = source.read_text() + "\n// Consumer edit retained through regeneration.\n"
            source.write_text(edited_source)
            settings = destination / "ravet-project.toml"
            settings.write_text(settings.read_text().replace('version = "0.1.0"', 'version = "0.2.0"'))
            run([str(executable), "regenerate", str(destination)])
            assert source.read_text() == edited_source
            manifest = tomllib.loads((destination / "Cargo.toml").read_text())
            assert manifest["package"]["version"] == "0.2.0"
            assert manifest["workspace"] == {}
            if build:
                if kind == "standalone" and sys.platform != "darwin":
                    print("Standalone macOS build/fixture skipped on this platform", flush=True)
                else:
                    command = [str(executable), "build", str(destination), "--offline"]
                    if kind in ("plugin", "instrument"):
                        command.append("--bundle")
                    run(command)
                    if kind == "offline":
                        run([str(destination / "target/debug" / (name + (".exe" if sys.platform == "win32" else "")))])
                    if kind == "standalone":
                        run([str(destination / "target/debug" / name), "--ui-test", str(parent / "standalone-ui")])
                    if kind in ("plugin", "instrument"):
                        assert (destination / "target/clap/debug" / f"{name}.clap").exists()
                        for export in formats[1:]:
                            run([str(executable), "build", str(destination), "--offline", "--format", export])
                            suffix = {"vst3":"vst3", "au":"component", "auv3":"app", "aax":"aaxplugin", "lv2":"lv2"}[export]
                            artifact = destination / "target/plugins/debug" / f"{name}.{suffix}"
                            assert artifact.is_dir()
                            if export == "vst3" and vst3_validator:
                                run([str(vst3_validator), str(artifact)])
                            if sys.platform == "darwin" and export != "lv2":
                                run(["codesign", "--verify", "--strict", str(artifact)])
                            if export == "au":
                                info = plistlib.loads((artifact / "Contents/Info.plist").read_bytes())
                                assert info["CFBundleVersion"] == "0.2.0"
                                assert info["AudioComponents"][0]["type"] == ("aumu" if kind == "instrument" else "aufx")
                                assert (artifact / "Contents/Resources/plugin.vst3").is_dir()
                            if export == "lv2":
                                assert f"urn:com.example.{name}:mono" in (artifact / "manifest.ttl").read_text()
                                assert (artifact / "foundry_plugin_lv2.so").is_file()
                            if export == "aax" and sys.platform == "darwin":
                                library = ctypes.CDLL(str(artifact / "Contents/MacOS/foundry_plugin_aax"))
                                for entry in ("ACFRegisterPlugin", "ACFRegisterComponent", "ACFGetClassFactory", "ACFCanUnloadNow", "ACFStartup", "ACFShutdown", "ACFGetSDKVersion", "GetPluginFactory"):
                                    getattr(library, entry)
        run([str(executable), "doctor"])
    print("Foundry external consumer workflow PASS", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    default = Path(__file__).resolve().parents[1] / "target/debug" / ("ravet-foundry.exe" if sys.platform == "win32" else "ravet-foundry")
    parser.add_argument("--executable", type=Path, default=default)
    parser.add_argument("--build", action="store_true")
    for sdk in ("vst3", "audio-unit", "aax", "lv2"):
        parser.add_argument(f"--{sdk}-sdk", type=Path)
    parser.add_argument("--auv3", action="store_true", help="Build the macOS effect extension/container")
    parser.add_argument("--vst3-validator", type=Path, help="Optional official SDK validator executable")
    args = parser.parse_args()
    sdks = {key: value.resolve(strict=True) for key in ("vst3", "audio-unit", "aax", "lv2")
            if (value := getattr(args, key.replace("-", "_") + "_sdk"))}
    if (sdks or args.auv3 or args.vst3_validator) and not args.build:
        parser.error("Native validation options require --build")
    check(args.executable.resolve(strict=True), args.build, sdks, args.auv3,
          args.vst3_validator.resolve(strict=True) if args.vst3_validator else None)
