#!/usr/bin/env python3
"""Create an explicit offline starter outside the RAVET repository (Python 3.11+)."""
import argparse
import json
from pathlib import Path
import re
import shutil


def create_project(destination: Path, name: str, license_id: str) -> Path:
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
        quote = lambda value: json.dumps(value, ensure_ascii=False)
        manifest = f'''[package]
name = {quote(name)}
version = "0.1.0"
edition = "2024"
publish = false
license = {quote(license_id)}

[workspace]

[dependencies]
ravet = {{ path = {quote(str(root / "crates/ravet"))} }}
ravet-gain = {{ path = {quote(str(root / "examples/gain"))} }}

[lints.rust]
unsafe_code = "forbid"
'''
        (destination / "Cargo.toml").write_text(manifest, encoding="utf-8")
        (destination / "src").mkdir()
        shutil.copyfile(root / "templates/offline-gain/main.rs", destination / "src/main.rs")
        (destination / ".gitignore").write_text("/target/\n", encoding="utf-8")
        (destination / "README.md").write_text(f'''# {name}

An independent offline RAVET starter with explicit local framework dependencies.

```sh
cargo generate-lockfile --offline
CARGO_BUILD_JOBS=2 cargo run --locked
```

The fixture processes four synthetic stereo frames and checks gain=2. It opens
no audio/MIDI device, window, network endpoint or host. Keep Cargo.lock with this
project. The absolute framework dependency paths require updating if RAVET moves.
This manifest's licence is caller-selected; RAVET dependency licensing stays
under its own terms. Format exports, device/UI integration and release policy
belong to subsequent consumer work. This project is outside the RAVET workspace.
''', encoding="utf-8")
    except BaseException:
        shutil.rmtree(destination)
        raise
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--name", required=True)
    parser.add_argument("--license", dest="license_id", required=True)
    args = parser.parse_args()
    try:
        destination = create_project(args.destination, args.name, args.license_id)
    except (ValueError, OSError) as error:
        parser.error(str(error))
    print(destination)


if __name__ == "__main__":
    main()
