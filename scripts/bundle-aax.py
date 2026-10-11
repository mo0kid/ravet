#!/usr/bin/env python3
"""Build a local AAX Gain, Performance, MidiSine, RawRouting or opt-in GainMeters reference; no host installation or PACE signing."""
import argparse
from pathlib import Path
import aax_bundle


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--internal-precision", type=int, choices=(32, 64), default=32,
                        help="Native DSP precision behind unchanged AAX float32 audio")
    parser.add_argument("--editor", action="store_true", help="Build optional macOS native Gain custom editor")
    parser.add_argument("--plugin", choices=aax_bundle.PLUGINS, default="gain")
    parser.add_argument("--profile", choices=tuple(aax_bundle.PROFILES), default="native")
    parser.add_argument("--vst3-sdk", type=Path, required=True)
    parser.add_argument("--aax-sdk", type=Path, required=True)
    args = parser.parse_args()
    try:
        context = aax_bundle.prepare_context(Path(__file__).resolve().parents[1], args.vst3_sdk, args.aax_sdk, profile=args.profile, plugin=args.plugin, editor=args.editor, internal_precision=args.internal_precision)
    except ValueError as error:
        parser.error(str(error))
    reference = aax_bundle.build_reference(context)
    artifact = aax_bundle.package_reference(reference)
    aax_bundle.validate_reference(reference)
    print(artifact)


if __name__ == "__main__":
    main()
