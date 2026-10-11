#!/usr/bin/env python3
"""Check actual macOS reference AAX load/entry points; not Pro Tools validation."""
import ctypes
import pathlib
import argparse


def check(artifact, plugin="gain"):
    if plugin not in ("gain", "performance", "midi", "raw-routing", "gain-meters"):
        raise ValueError("expected an explicit public AAX reference profile")
    artifact = pathlib.Path(artifact).resolve()
    library = ctypes.CDLL(str(artifact / f"Contents/MacOS/ravet_{plugin.replace('-', '_')}_aax"))
    for name in ("ACFRegisterPlugin", "ACFRegisterComponent", "ACFGetClassFactory",
                 "ACFCanUnloadNow", "ACFStartup", "ACFShutdown",
                 "ACFGetSDKVersion", "GetPluginFactory"):
        getattr(library, name)
    query = library.ACFGetSDKVersion
    query.argtypes = [ctypes.POINTER(ctypes.c_uint64)]
    query.restype = ctypes.c_int32
    version = ctypes.c_uint64()
    if query(ctypes.byref(version)) != 0 or not version.value:
        raise RuntimeError("AAX SDK version entry failed")
    print(f"AAX bundle loaded; required exports present; SDK version {version.value:#x}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact", type=pathlib.Path)
    parser.add_argument("--plugin", choices=("gain", "performance", "midi", "raw-routing", "gain-meters"), default="gain")
    args = parser.parse_args()
    check(args.artifact, args.plugin)
