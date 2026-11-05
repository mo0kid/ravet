"""Cross-compile RAVET products and C++ plug-in adapters for x86_64 Windows.

Used by the bundle scripts' ``--target windows`` option on Linux (or macOS)
build machines. Rust is built with ``cargo xwin`` and the C++ adapters with
CMake, clang-cl and lld-link through ``scripts/cmake/windows-xwin.cmake``,
both against the Microsoft CRT and Windows SDK that cargo-xwin downloads (its
first use asks you to accept Microsoft's licence terms).

Both sides link the C runtime statically (Rust ``+crt-static``, C++ ``/MT``),
so the bundles and the cross-built validators run on a Windows installation
without the Visual C++ Redistributable.

Steps that run Windows programs at build time (LV2 Turtle generation) or the
adapter tests use Wine through ``CMAKE_CROSSCOMPILING_EMULATOR`` when it is
installed. Format validators are cross-built but run on Windows.
"""
from pathlib import Path
import re
import shutil

TRIPLE = "x86_64-pc-windows-msvc"
RUSTFLAGS = "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS"
STATIC_CRT = "-C target-feature=+crt-static"
# Variables taken from `cargo xwin env`: compiler/linker names on PATH and the
# location of cargo-xwin's own CMake toolchain, which windows-xwin.cmake wraps.
FROM_XWIN = ("PATH", "CMAKE_TOOLCHAIN_FILE_x86_64_pc_windows_msvc")


def parse_environment(text):
    """The ``export NAME="value";`` lines printed by ``cargo xwin env``."""
    return dict(re.findall(r'^export ([A-Za-z_][A-Za-z0-9_]*)="(.*)";\s*$', text, re.MULTILINE))


def host_tools(directory):
    """A no-op ``attrib`` for the VST3 SDK's Windows folder-icon step.

    The SDK copies PlugIn.ico and desktop.ini into the bundle and then marks
    them with Windows' ``attrib +s``, which does not exist on other hosts. Only
    the system attribute is skipped; Explorer then shows a plain folder.
    """
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    attrib = directory / "attrib"
    attrib.write_text("#!/bin/sh\n# RAVET Windows cross build: Windows attrib is not available here.\nexit 0\n")
    attrib.chmod(0o755)
    return directory


def prepare(runner, target, *, platform):
    """Point ``runner`` at the cargo-xwin toolchain; return CMake arguments.

    Needs cargo-xwin (``cargo install --locked cargo-xwin``); ``cargo xwin env``
    fails first without it.
    """
    if platform == "win32":
        raise ValueError("--target windows cross-compiles from Linux or macOS; build natively on Windows")
    found = parse_environment(runner.run(["cargo", "xwin", "env", "--target", TRIPLE], capture=True) or "")
    environment = runner.environment
    for name in FROM_XWIN:
        if name in found:
            environment[name] = found[name]
    tools = host_tools(Path(target) / "windows-cross" / "host-tools")
    environment["PATH"] = f"{tools}:{environment.get('PATH', '')}"
    static_crt(environment)
    return toolchain_arguments(Path(__file__).resolve().parent)


def static_crt(environment):
    """Add +crt-static to the Windows target's Rust flags in ``environment``."""
    flags = environment.get(RUSTFLAGS, "")
    if STATIC_CRT not in flags:
        environment[RUSTFLAGS] = f"{flags} {STATIC_CRT}".strip()
    return environment


def toolchain_arguments(scripts):
    arguments = [f"-DCMAKE_TOOLCHAIN_FILE={Path(scripts) / 'cmake/windows-xwin.cmake'}",
                 "-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded"]
    wine = shutil.which("wine")
    if wine:
        arguments.append(f"-DCMAKE_CROSSCOMPILING_EMULATOR={wine}")
    return arguments


def emulator_available(arguments):
    return any(argument.startswith("-DCMAKE_CROSSCOMPILING_EMULATOR=") for argument in arguments)


def cargo(subcommand, *arguments):
    """``cargo xwin <subcommand> --target x86_64-pc-windows-msvc ...``."""
    return ["cargo", "xwin", subcommand, "--target", TRIPLE, *arguments]


def output_directory(target, profile="release"):
    return Path(target) / TRIPLE / profile
