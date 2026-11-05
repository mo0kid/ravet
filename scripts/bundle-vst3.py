#!/usr/bin/env python3
"""Build a RAVET product as VST3 with an explicit external SDK; no installation."""
import argparse
from pathlib import Path
from generic_vst3_bundle import build_generic
from vst3_bundle import build_reference, package_reference, prepare_context, validate_reference


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--plugin", default="gain", help="Product name, e.g. gain or delay")
    parser.add_argument("--mock", action="store_true", help="Use the independent C++ table fixture")
    parser.add_argument("--package", help="Cargo product package (default ravet-<plugin>-plugin)")
    parser.add_argument("--manifest-path", type=Path, help="External product Cargo.toml")
    parser.add_argument("--legacy-reference", action="store_true", help="Use the 0.0.1 Gain/Performance wrapper")
    parser.add_argument("--profile", choices=["debug", "release"], default="release")
    parser.add_argument("--native-editor", action="store_true", help="Include the macOS Rust-owned Gain WebKit editor")
    parser.add_argument("--factory-programs", action="store_true", help="Include the optional macOS validated Gain program list/selector")
    args = parser.parse_args()
    if not (args.legacy_reference or args.plugin == "performance" or args.native_editor or args.factory_programs):
        try:
            artifacts = build_generic(Path(__file__).resolve().parents[1], args.sdk, args.plugin,
                                      args.profile, mock=args.mock, package=args.package,
                                      manifest_path=args.manifest_path)
        except ValueError as error:
            parser.error(str(error))
        for artifact in artifacts:
            print(artifact)
        return
    if args.mock or args.package or args.manifest_path:
        parser.error("mock/package/manifest-path require the generic adapter")
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
