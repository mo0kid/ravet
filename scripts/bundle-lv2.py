#!/usr/bin/env python3
"""Build a local public gain or performance LV2 bundle using external headers."""
import argparse
import pathlib
from lv2_bundle import build_reference, package_reference, prepare_context, validate_reference


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=pathlib.Path, required=True)
    parser.add_argument("--plugin", choices=("gain", "performance"), default="gain")
    args = parser.parse_args()
    root = pathlib.Path(__file__).resolve().parents[1]
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
