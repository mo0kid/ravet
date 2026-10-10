#!/usr/bin/env python3
"""Build a local AUv2 reference over public VST3/Rust DSP; no host installation."""
import argparse
from pathlib import Path
from au_bundle import build_reference, package_reference, prepare_context, validate_reference


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vst3-sdk", type=Path, required=True)
    parser.add_argument("--audio-unit-sdk", type=Path, required=True)
    parser.add_argument("--plugin", choices=("gain", "performance"), default="gain")
    parser.add_argument("--native-editor", action="store_true", help="Embed the macOS gain WebKit editor and validate its AU Cocoa view")
    parser.add_argument("--factory-programs", action="store_true", help="Include correctly indexed complete gain factory presets")
    args = parser.parse_args()
    try:
        context = prepare_context(Path(__file__).resolve().parents[1], args.vst3_sdk, args.audio_unit_sdk,
                                  args.plugin, native_editor=args.native_editor, factory_programs=args.factory_programs)
    except ValueError as error:
        parser.error(str(error))
    reference = build_reference(context)
    artifact = package_reference(reference)
    validate_reference(reference)
    print(artifact)


if __name__ == "__main__":
    main()
