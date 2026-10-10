#!/usr/bin/env python3
"""Build public VST3 references with an explicit external SDK; no installation."""
import argparse
from pathlib import Path
from vst3_bundle import build_reference, package_reference, prepare_context, validate_reference


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--plugin", choices=["gain", "performance"], default="gain")
    parser.add_argument("--profile", choices=["debug", "release"], default="release")
    parser.add_argument("--native-editor", action="store_true", help="Include the macOS Rust-owned Gain WebKit editor")
    parser.add_argument("--factory-programs", action="store_true", help="Include the optional macOS validated Gain program list/selector")
    args = parser.parse_args()
    try:
        context = prepare_context(Path(__file__).resolve().parents[1], args.sdk, args.plugin,
                                  args.profile, native_editor=args.native_editor,
                                  factory_programs=args.factory_programs)
    except ValueError as error:
        parser.error(str(error))
    reference = build_reference(context)
    artifacts = package_reference(reference)
    validate_reference(reference, artifacts)
    for artifact in artifacts:
        print(artifact)


if __name__ == "__main__":
    main()
