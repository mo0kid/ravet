#!/usr/bin/env python3
"""Build a native AUv2 from any RAVET product; no host installation."""
import argparse
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vst3-sdk", type=Path, help="Use the historical gain/performance VST3 wrapper reference")
    parser.add_argument("--audio-unit-sdk", type=Path, required=True)
    parser.add_argument("--plugin", default="gain", help="Product in examples/<plugin>-plugin (e.g. delay), or a legacy reference with --vst3-sdk")
    parser.add_argument("--manifest-path", type=Path, help="Cargo.toml of an external product crate")
    parser.add_argument("--product-library", type=Path, help="Already built trusted dylib exporting ravet_plugin_v1")
    parser.add_argument("--build-dir", type=Path)
    parser.add_argument("--mock", choices=("effect", "instrument", "midi"), help="Build a test-only C++ product instead of Rust")
    parser.add_argument("--validate", action="store_true", help="Run in-process native component contracts (does not install)")
    parser.add_argument("--native-editor", action="store_true", help="Legacy gain editor option; native products advertise their table's editor automatically")
    parser.add_argument("--factory-programs", action="store_true", help="Include correctly indexed complete gain factory presets")
    args = parser.parse_args()
    try:
        if not args.vst3_sdk:
            if args.factory_programs:
                raise ValueError("--factory-programs belongs to the legacy --vst3-sdk reference")
            from au_native_bundle import build
            build(Path(__file__).resolve().parents[1], args)
            return
        if args.mock or args.product_library or args.manifest_path:
            raise ValueError("native product options cannot be combined with the legacy --vst3-sdk reference")
        from au_bundle import build_reference, package_reference, prepare_context, validate_reference
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
