#!/usr/bin/env python3
"""Build a local LV2 bundle using external LV2 headers.

Generic products (the 0.1.0 export layer, any platform): --plugin generic (the
gain plug-in), --plugin delay, --plugin mock (the C++ test table), or
--crate <package> for any product crate built with ravet_plugin::export_plugin!.

0.0.1 references (macOS/Linux): --plugin gain (default), performance or
note-routing.
"""
import argparse
import pathlib
from lv2_bundle import PROFILES, build_reference, package_reference, prepare_context, validate_reference
from lv2_generic import PRODUCTS, build_generic, package_generic, prepare_generic, validate_generic


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--sdk", type=pathlib.Path, required=True)
    parser.add_argument("--plugin", choices=(*PROFILES, *PRODUCTS), default="gain")
    parser.add_argument("--crate", help="product crate exporting ravet_plugin_v1 (overrides --plugin)")
    args = parser.parse_args()
    root = pathlib.Path(__file__).resolve().parents[1]
    if args.crate or args.plugin in PRODUCTS:
        try:
            context = prepare_generic(root, args.sdk, args.plugin, args.crate)
        except ValueError as error:
            parser.error(str(error))
        target, build = build_generic(context)
        artifact = package_generic(context, target, build)
        validate_generic(context, artifact)
        print(artifact)
        return
    try:
        context = prepare_context(root, args.sdk, args.plugin)
    except ValueError as error:
        parser.error(str(error))
    reference = build_reference(context)
    artifact = package_reference(reference)
    validate_reference(reference, artifact)
    print(artifact)


if __name__ == "__main__":
    main()
