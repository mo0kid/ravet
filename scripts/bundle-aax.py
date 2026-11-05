#!/usr/bin/env python3
"""Build a local, unsigned .aaxplugin; no host installation and no PACE signing.

Products exported with ravet_plugin::export_plugin! use the generic AAX adapter
(crates/ravet-plugin/aax): --plugin delay|generic|mock, or --package <crate> for
any other product crate. Only --aax-sdk is needed.

The 0.0.1 references (--plugin gain|performance|midi|raw-routing|gain-meters)
keep their own native profiles and also need --vst3-sdk.
"""
import argparse
from pathlib import Path
import aax_bundle
import aax_plugin


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--plugin", choices=(*aax_plugin.PRODUCTS, *aax_bundle.PLUGINS), default=None,
                        help="product (generic adapter) or 0.0.1 reference; default gain, or the --package crate")
    parser.add_argument("--package", help="product crate built through the generic adapter")
    parser.add_argument("--name", help="bundle name for --package (default: the crate name)")
    parser.add_argument("--aax-sdk", type=Path, required=True, help="external Avid AAX SDK root")
    parser.add_argument("--vst3-sdk", type=Path, help="external VST3 SDK (0.0.1 references only)")
    parser.add_argument("--internal-precision", type=int, choices=(32, 64), default=32,
                        help="0.0.1 references: native DSP precision behind float32 AAX audio")
    parser.add_argument("--editor", action="store_true", help="0.0.1 Gain reference: macOS native editor")
    parser.add_argument("--profile", choices=tuple(aax_bundle.PROFILES), default="native",
                        help="0.0.1 Gain reference profile")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]

    if args.package or (args.plugin in aax_plugin.PRODUCTS):
        try:
            selected = aax_plugin.product(args.plugin, package=args.package, name=args.name)
            built = aax_plugin.build(root, args.aax_sdk, selected)
        except ValueError as error:
            parser.error(str(error))
        print(built.artifact)
        return

    if not args.vst3_sdk:
        parser.error("the 0.0.1 AAX references need --vst3-sdk")
    try:
        context = aax_bundle.prepare_context(root, args.vst3_sdk, args.aax_sdk, profile=args.profile,
                                             plugin=args.plugin or "gain", editor=args.editor,
                                             internal_precision=args.internal_precision)
    except ValueError as error:
        parser.error(str(error))
    reference = aax_bundle.build_reference(context)
    artifact = aax_bundle.package_reference(reference)
    aax_bundle.validate_reference(reference)
    print(artifact)


if __name__ == "__main__":
    main()
