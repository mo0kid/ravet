#!/usr/bin/env python3
"""Check actual macOS reference AAX load/entry points; not Pro Tools validation."""
import ctypes
import pathlib
import sys


def check(artifact):
    artifact = pathlib.Path(artifact).resolve()
    library = ctypes.CDLL(str(artifact / "Contents/MacOS/ravet_gain_aax"))
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
    check(sys.argv[1])
