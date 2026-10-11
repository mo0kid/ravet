#!/usr/bin/env python3
"""Explicit local Gain/Performance Rust/native parameter owner fixture; no SDK discovery."""
import argparse
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build', type=Path, required=True,
                        help='Explicit Release CMake directory containing ravet-vst3-host')
    parser.add_argument('--gain', type=Path, required=True)
    parser.add_argument('--performance', type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('This actual native composition fixture has macOS evidence only')
    root = Path(__file__).resolve().parents[2]
    build = args.build.resolve()
    sdk_libs = build / 'lib' / 'Release'
    libraries = ['sdk_hosting', 'sdk_common', 'base', 'pluginterfaces']
    required = [build / 'libravet-vst3-host.a',
                *(sdk_libs / ('lib' + name + '.a') for name in libraries)]
    for path in required:
        if not path.is_file():
            parser.error('Required explicit built archive missing: ' + str(path))
    modules = [args.gain.resolve(), args.performance.resolve()]
    for path in modules:
        if not path.is_dir() or path.suffix != '.vst3':
            parser.error('Selected explicit reference bundle missing: ' + str(path))
    # Local temporary output avoids executable loading delays on shared volumes;
    # this neither installs nor alters the selected plug-in bundles.
    with tempfile.TemporaryDirectory(prefix='ravet-vst3-parameter-owner-') as temporary:
        executable = Path(temporary) / 'parameter-owner'
        command = ['rustc', '--edition=2024',
                   str(root / 'tools/vst3-validation/parameter-owner.rs'),
                   '-A', 'dead_code', '-D', 'warnings', '-o', str(executable),
                   '-L', str(build), '-l', 'static=ravet-vst3-host', '-L', str(sdk_libs)]
        for name in libraries:
            command += ['-l', 'static=' + name]
        command += ['-l', 'dylib=c++', '-C', 'link-arg=-framework',
                    '-C', 'link-arg=Cocoa', '-C', 'link-arg=-framework',
                    '-C', 'link-arg=CoreFoundation']
        subprocess.run(command, check=True)
        subprocess.run([str(executable), *(str(path) for path in modules)], check=True)


if __name__ == '__main__':
    main()
