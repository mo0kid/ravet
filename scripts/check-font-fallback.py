#!/usr/bin/env python3
"""Compare caller-selected contextual fallback to an installed HarfBuzz C library."""
import argparse
import json
import math
import os
from pathlib import Path
import shlex
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--output', type=Path, default=Path('target/font-fallback'))
    parser.add_argument('--target-dir', type=Path)
    parser.add_argument('--surface-manifest', type=Path, help='Optional isolated validation manifest')
    parser.add_argument('--consumer-manifest', type=Path, help='Optional isolated consumer manifest')
    parser.add_argument('--consumer-bin', default='font_fallback')
    parser.add_argument('--consumer-features', default='font-fallback,live-ui')
    parser.add_argument('--fixtures', type=Path)
    parser.add_argument('--oracle-source', type=Path)
    parser.add_argument('--cargo-driver', default='cargo', help='Argument vector, parsed without a shell')
    parser.add_argument('--runner', default='', help='Optional executable argument prefix; no shell')
    parser.add_argument('--native', action='store_true', help='Also run the owned macOS hidden-window consumer')
    args = parser.parse_args()
    root = args.root.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    target = (args.target_dir or output / 'cargo').resolve()
    fixtures = (args.fixtures or root / 'crates/ravet-surface/tests/fixtures').resolve()
    source = (args.oracle_source or fixtures / 'fallback/harfbuzz-oracle.c').resolve()
    environment = dict(os.environ, CARGO_BUILD_JOBS='1', CARGO_TARGET_DIR=str(target))
    driver = shlex.split(args.cargo_driver)
    runner = shlex.split(args.runner)
    def invoke(arguments, *, capture=False):
        return subprocess.run(arguments, cwd=root, env=environment, check=True,
                              capture_output=capture, text=True)
    print('Independent HarfBuzz', invoke(['pkg-config', '--modversion', 'harfbuzz'], capture=True).stdout.strip(), flush=True)
    flags = shlex.split(invoke(['pkg-config', '--cflags', '--libs', 'harfbuzz'], capture=True).stdout)
    oracle = output / 'harfbuzz-context'
    invoke(['clang', '-std=c11', '-Wall', '-Wextra', '-Werror', str(source), '-o', str(oracle), *flags])
    package = ['--manifest-path', str(args.surface_manifest.resolve())] if args.surface_manifest else ['-p', 'ravet-surface']
    invoke([*driver, 'build', *package, '--features', 'text', '--example', 'fallback-oracle', '--locked'])
    raw = invoke([*runner, str(target / 'debug/examples/fallback-oracle')], capture=True).stdout
    if len(raw.encode('utf8')) > 1024 * 1024:
        raise RuntimeError('Rust oracle output capacity')
    (output / 'rust-runs.json').write_text(raw)
    models = json.loads(raw)
    if len(models) != 10:
        raise RuntimeError('Expected ten explicit source profiles')
    total = 0
    for model in models:
        source_text = bytes.fromhex(model['source']).decode('utf8')
        accumulated = [0., 0.]
        for run in model['runs']:
            reference = json.loads(invoke([*runner, str(oracle), str(fixtures / run['font']), source_text,
                str(run['range'][0]), str(run['range'][1]), model['direction'], model['script'], model['language']], capture=True).stdout)
            if len(run['glyphs']) != len(reference['glyphs']):
                raise RuntimeError(f"Glyph-count mismatch: {model['name']}")
            pen = [0., 0.]
            scale = model['size'] / reference['upem']
            for actual, glyph in zip(run['glyphs'], reference['glyphs']):
                if actual['id'] != glyph['id'] or actual['cluster'] != glyph['cluster']:
                    raise RuntimeError(f"Glyph/source mismatch: {model['name']}")
                for key, expected in [('x', pen[0] + glyph['xo'] * scale), ('y', pen[1] + glyph['yo'] * scale),
                                      ('xa', glyph['xa'] * scale), ('ya', glyph['ya'] * scale)]:
                    if not math.isclose(actual[key], expected, rel_tol=1e-10, abs_tol=1e-10):
                        raise RuntimeError(f"Position mismatch: {model['name']} {key}")
                pen[0] += glyph['xa'] * scale
                pen[1] += glyph['ya'] * scale
            if not all(math.isclose(a, b, rel_tol=1e-10, abs_tol=1e-10) for a, b in zip(run['origin'], accumulated)):
                raise RuntimeError('Visual-run origin mismatch')
            accumulated = [a + b for a, b in zip(accumulated, pen)]
            total += 1
        if not all(math.isclose(a, b, rel_tol=1e-10, abs_tol=1e-10) for a, b in zip(model['advance'], accumulated)):
            raise RuntimeError('Complete directional advance mismatch')
        print(model['name'], 'independent context/flags/glyph/source/positions PASS', flush=True)
    print(len(models), 'profiles', total, 'exact matched-face runs PASS', flush=True)
    if args.native:
        package = ['--manifest-path', str(args.consumer_manifest.resolve())] if args.consumer_manifest else ['-p', 'ravet-examples']
        features = args.consumer_features
        invoke([*driver, 'build', *package, '--features', features, '--bin', args.consumer_bin, '--locked'])
        result = invoke([*runner, str(target / 'debug' / args.consumer_bin), '--fixture'], capture=True)
        (output / 'native-fixture.log').write_text(result.stdout + result.stderr)
        print(result.stdout, end='', flush=True)


if __name__ == '__main__':
    main()
