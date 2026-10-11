#!/usr/bin/env python3
"""Explicit local AUv3 host provider and Rust owner fixture; no registration or installation."""
import argparse
from pathlib import Path
import subprocess
import sys
import tempfile

FRAMEWORKS = ['AudioToolbox', 'AVFoundation', 'Foundation', 'CoreMIDI', 'Security']


def commands(root, build, gain, executable):
    """Return the exact native build, CTest, Rust compile and run commands."""
    source = root / 'crates/ravet-host/native/au'
    rust = ['rustc', '--edition=2024', str(root / 'tools/au-host-validation/host-owner.rs'),
            '-A', 'dead_code', '-D', 'warnings', '-o', str(executable),
            '-L', str(build), '-l', 'static=ravet_au_host_fixture', '-l', 'static=ravet_au_host',
            '-L', str(gain.parent), '-l', 'static=' + gain.stem.removeprefix('lib'),
            '-l', 'dylib=c++', '-l', 'dylib=iconv']
    for name in FRAMEWORKS:
        rust += ['-C', 'link-arg=-framework', '-C', 'link-arg=' + name]
    return [
        ['cmake', '-S', str(source), '-B', str(build), '-DCMAKE_BUILD_TYPE=Release',
         '-DCMAKE_OSX_DEPLOYMENT_TARGET=12.0', '-DBUILD_TESTING=ON',
         '-DRAVET_GAIN_LIBRARY=' + str(gain)],
        ['cmake', '--build', str(build), '--config', 'Release'],
        ['ctest', '--test-dir', str(build), '--output-on-failure', '-j1'],
        rust,
        [str(executable)],
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build', type=Path, required=True,
                        help='Explicit caller-owned CMake build directory')
    parser.add_argument('--gain-library', type=Path, required=True,
                        help='Explicit caller-built libravet_gain_native.a archive')
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('This actual AUv3 host composition fixture has macOS evidence only')
    root = Path(__file__).resolve().parents[2]
    gain = args.gain_library.resolve()
    if not gain.is_file() or gain.suffix != '.a' or not gain.name.startswith('lib'):
        parser.error('Selected explicit Gain native archive missing: ' + str(gain))
    # Local temporary output avoids executable loading delays on shared volumes.
    # The fixture registers the public Gain subclass only inside its own process.
    with tempfile.TemporaryDirectory(prefix='ravet-au-host-owner-') as temporary:
        for command in commands(root, args.build.resolve(), gain,
                                Path(temporary) / 'au-host-owner'):
            subprocess.run(command, check=True)


if __name__ == '__main__':
    main()
