#!/usr/bin/env python3
"""Create an independent offline/embedded/Arduino/Pi starter outside RAVET (Python 3.11+)."""
import argparse
import json
from pathlib import Path
import re
import shutil
import tomllib

FRAMEWORK_GIT = "https://github.com/mo0kid/ravet"
SOURCE_NOTE = {
    "path": "RAVET dependencies are absolute paths to a local checkout; update them if it moves.",
    "tag": "RAVET dependencies use one release tag of the public repository; the tag must be fetchable.",
}


def framework_tag(root: Path) -> str:
    """Release tag matching the checkout's workspace version, e.g. v0.1.0."""
    with open(root / "Cargo.toml", "rb") as manifest:
        return "v" + tomllib.load(manifest)["workspace"]["package"]["version"]


def ravet_dep(root: Path, relative: str, source: str, extra: str = "") -> str:
    """Inline table for one RAVET package; every dependency shares one source."""
    quote = lambda value: json.dumps(value, ensure_ascii=False)
    if source == "tag":
        return f"{{ git = {quote(FRAMEWORK_GIT)}, tag = {quote(framework_tag(root))}{extra} }}"
    return f"{{ path = {quote(str(root / relative))}{extra} }}"


def create_project(destination: Path, name: str, license_id: str, kind: str = "offline",
                   source: str = "path") -> Path:
    if kind not in {"offline", "embedded-rust", "arduino", "pi-linux"}:
        raise ValueError("unknown project kind")
    if source not in {"path", "tag"}:
        raise ValueError("RAVET source must be path or tag")
    root = Path(__file__).resolve().parents[1]
    destination = destination.resolve()
    if destination.is_relative_to(root):
        raise ValueError("consumer projects must stay outside the RAVET repository")
    if not re.fullmatch(r"[a-z][a-z0-9_-]{0,63}", name):
        raise ValueError("name must be 1..64 lowercase ASCII package characters")
    if not license_id or len(license_id) > 256 or any(ord(c) < 32 for c in license_id):
        raise ValueError("provide a nonempty explicit Cargo licence identifier")
    if not destination.parent.is_dir():
        raise ValueError("destination parent must already be a directory")
    # mkdir is exclusive: never overwrite or remove an existing project.
    destination.mkdir()
    try:
        if kind != "offline":
            write_embedded_project(root, destination, name, license_id, kind, source)
            return destination
        quote = lambda value: json.dumps(value, ensure_ascii=False)
        manifest = f'''[package]
name = {quote(name)}
version = "0.1.0"
edition = "2024"
publish = false
license = {quote(license_id)}

[workspace]

[dependencies]
ravet = {ravet_dep(root, "crates/ravet", source)}
ravet-gain = {ravet_dep(root, "examples/gain", source)}

[lints.rust]
unsafe_code = "forbid"
'''
        (destination / "Cargo.toml").write_text(manifest, encoding="utf-8")
        (destination / "src").mkdir()
        shutil.copyfile(root / "templates/offline-gain/main.rs", destination / "src/main.rs")
        (destination / ".gitignore").write_text("/target/\n", encoding="utf-8")
        (destination / "README.md").write_text(f'''# {name}

An independent offline RAVET starter. {SOURCE_NOTE[source]}

```sh
cargo generate-lockfile --offline
CARGO_BUILD_JOBS=2 cargo run --locked
```

The fixture processes four synthetic stereo frames and checks gain=2. It opens
no audio/MIDI device, window, network endpoint or host. Keep Cargo.lock with this
project.
This manifest's licence is caller-selected; RAVET dependency licensing stays
under its own terms. Format exports, device/UI integration and release policy
belong to subsequent consumer work. This project is outside the RAVET workspace.
''', encoding="utf-8")
    except BaseException:
        shutil.rmtree(destination)
        raise
    return destination


def write_embedded_project(root: Path, destination: Path, name: str, license_id: str, kind: str,
                           source: str = "path"):
    quote = lambda value: json.dumps(value, ensure_ascii=False)
    if kind == "arduino":
        shutil.copytree(root / "embedded/arduino", destination / "libraries/RAVETEmbedded")
        (destination / "OfflineGain").mkdir()
        shutil.copyfile(root / "embedded/arduino/examples/OfflineGain/OfflineGain.ino",
                        destination / "OfflineGain/OfflineGain.ino")
        commands = "Open OfflineGain/OfflineGain.ino in Arduino IDE, and install libraries/RAVETEmbedded as a local library."
        (destination / ".gitignore").write_text("/build/\n", encoding="utf-8")
    else:
        manifest = f'''[package]
name = {quote(name)}
version = "0.1.0"
edition = "2024"
publish = false
license = {quote(license_id)}

[workspace]

[dependencies]
ravet-foundation = {ravet_dep(root, "crates/ravet-foundation", source, ", default-features = false")}
ravet-embedded = {ravet_dep(root, "crates/ravet-embedded", source)}
ravet-embedded-gain = {ravet_dep(root, "examples/embedded-gain", source, ", default-features = false")}

[lints.rust]
unsafe_code = "forbid"
'''
        (destination / "Cargo.toml").write_text(manifest, encoding="utf-8")
        (destination / "src").mkdir()
        shutil.copyfile(root / "templates/embedded/lib.rs", destination / "src/lib.rs")
        if kind == "pi-linux":
            (destination / "src/main.rs").write_text(
                f'''fn main() {{
    assert!({name.replace("-", "_")}::offline_fixture());
    println!("RAVET embedded stereo gain PASS");
}}
''', encoding="utf-8")
            commands = "cargo generate-lockfile --offline\ncargo run --locked"
        else:
            commands = "rustup target add thumbv7em-none-eabihf\ncargo generate-lockfile --offline\ncargo build --lib --target thumbv7em-none-eabihf --locked"
        (destination / ".gitignore").write_text("/target/\n", encoding="utf-8")
    (destination / "README.md").write_text(f'''# {name}

Independent {kind} RAVET starter, consumer licence selection: {license_id}.
RAVET dependencies retain their own licence terms. {SOURCE_NOTE[source]}
Copied Arduino sources must be updated when RAVET changes; keep generated Rust
Cargo.lock with the project.

```sh
{commands}
```

This is a headless synthetic fixture, not flashed firmware or live device audio.
The Rust bare-metal starter is a no_std library; board firmware must supply its
entry point, panic handler, linker/memory map, HAL, codec, DMA and scheduling.
Choose the target for the actual MCU/FPU and match C/Rust float ABIs when linking.
The Arduino starter uses native C++11 and does not link arbitrary Rust processors.
On Pi Linux, build natively using the target matching the installed OS bitness.
No device, window or network endpoint is opened. Keep proprietary consumer code,
assets and wrappers outside RAVET. See {root / "docs/embedded-projects.md"} for
Arduino, STM32 and Raspberry Pi integration limits and validation commands.
''', encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--kind", choices=["offline", "embedded-rust", "arduino", "pi-linux"], default="offline")
    parser.add_argument("--name", required=True)
    parser.add_argument("--license", dest="license_id", required=True)
    parser.add_argument("--ravet-source", choices=["path", "tag"], default="path",
                        help="local checkout paths (default) or the release tag matching this checkout")
    args = parser.parse_args()
    try:
        destination = create_project(args.destination, args.name, args.license_id, args.kind,
                                     args.ravet_source)
    except (ValueError, OSError) as error:
        parser.error(str(error))
    print(destination)


if __name__ == "__main__":
    main()
