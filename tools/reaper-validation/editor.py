#!/usr/bin/env python3
"""Scripted REAPER check of a built RAVET Delay bundle's editor in one format.

Uses the same disposable REAPER resource directory as delay.py (an ini passed
with -cfgfile; the normal configuration is never used), runs editor.lua, which
opens the editor in a floating FX window, sets a parameter from the host, logs
every parameter change, closes and reopens the editor and removes the plug-in
with its editor open, then quits.

On Linux, --probe names a compiled x11-editor-probe: once the editor is open
the driver runs it to find the editor's plug window inside REAPER's window,
record the window chain and map state, capture only that window and inject
the --action clicks/drags (XTest), whose edits then appear in the host log as
CHANGE lines. It runs again (capture only) after the editor is reopened.

Windows hosts without Python: --windows-root works as in delay.py (the Lua
script holds the editor open for --seconds; capture it from outside).

Example (Linux):
  cc -O2 -o target/x11-editor-probe tools/reaper-validation/x11-editor-probe.c -lX11 -lXtst
  python3 tools/reaper-validation/editor.py --format vst3 \\
      --bundle target/plugins/release/ravet-delay-plugin.vst3 \\
      --probe target/x11-editor-probe --action 280,300
"""
from __future__ import annotations

import argparse
import datetime
import subprocess
import sys
import time
from pathlib import Path, PureWindowsPath

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import delay  # noqa: E402

EDITOR_SIZE = (560, 340)  # examples/delay-plugin/src/lib.rs


def configure(script: Path, resource: str, root: str, fx_name: str, seconds: float, settle: float,
              quit_after: bool, probe: bool) -> None:
    config = {'root': root, 'resource': resource, 'fx_name': fx_name, 'seconds': seconds, 'settle': settle,
              'quit': quit_after}
    if probe:
        config['probe_done'] = root + '/probe.done'
        config['reopen_done'] = root + '/reopen.done'
    script.write_text('RAVET_EDITOR = ' + delay.lua_value(config) + '\n' + (HERE / 'editor.lua').read_text())


def wait_for(log: Path, text: str, process: subprocess.Popen, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline and process.poll() is None:
        if log.exists() and text in log.read_text(errors='replace'):
            return True
        time.sleep(0.25)
    return False


def probe(arguments, output: Path, name: str, actions: list[str]) -> int:
    width, height = arguments.size or EDITOR_SIZE
    command = [str(arguments.probe), arguments.title, str(width), str(height), str(output / name), *actions]
    result = subprocess.run(command, capture_output=True, text=True, timeout=120)
    (output / f'{name}.log').write_text(' '.join(command) + '\n' + result.stdout + result.stderr)
    print(result.stdout + result.stderr, end='')
    for ppm in sorted(output.glob(f'{name}-*.ppm')):
        subprocess.run(['magick', str(ppm), str(ppm.with_suffix('.png'))], check=False)
    return result.returncode


def run(arguments: argparse.Namespace) -> int:
    fmt = arguments.format
    fx_name = arguments.fx_name or delay.FORMATS[fmt]['prefix']
    stamp = datetime.datetime.now().strftime('%Y%m%d-%H%M%S')
    work = (arguments.work or delay.ROOT / 'target' / 'reaper-editor' / f'{fmt}-{stamp}').resolve()
    windows_work = PureWindowsPath(arguments.windows_root) / work.name if arguments.windows_root else None
    ini, script, env = delay.prepare(work, fmt, arguments.bundle.resolve(), fx_name, True, arguments.ini,
                                     False, windows_work)
    output = script.parent
    host = (lambda p: str(windows_work.joinpath(*p.relative_to(work).parts))) if windows_work else (lambda p: p.as_posix())
    configure(script, host(work / 'resource'), host(output), fx_name, arguments.seconds, arguments.settle, True,
              arguments.probe is not None)
    if windows_work is not None:
        reaper = PureWindowsPath(str(arguments.reaper) if arguments.reaper else r'C:\REAPER\reaper.exe')
        runner = delay.windows_runner(work, windows_work, reaper, arguments.timeout, 20.0)
        print('work directory:', work)
        print('copy it to:', windows_work)
        print('then run there: powershell -NoProfile -ExecutionPolicy Bypass -File', windows_work / runner.name)
        return 0
    reaper = arguments.reaper or delay.default_reaper()
    if reaper is None:
        raise SystemExit('error: REAPER not found; pass --reaper')
    command = [str(delay.executable(reaper)), '-cfgfile', str(ini), '-nosplash', '-ignoreerrors', '-newinst',
               str(script)]
    print('work directory:', work)
    guarded = delay.user_resource_directory()
    before = delay.snapshot(guarded)
    log = output / 'delay-host.log'
    ok = True
    with open(work / 'reaper-stdout.log', 'wb') as stdout:
        process = subprocess.Popen(command, stdout=stdout, stderr=subprocess.STDOUT, env=env,
                                   stdin=subprocess.DEVNULL, start_new_session=True)
        try:
            if arguments.probe is not None:
                if wait_for(log, 'READY for probe', process, arguments.timeout):
                    ok &= probe(arguments, output, 'probe', arguments.action) == 0
                    (output / 'probe.done').write_text('done\n')
                    if wait_for(log, 'REOPENED editor', process, 60):
                        ok &= probe(arguments, output, 'reopen', []) == 0
                    else:
                        ok = False
                    (output / 'reopen.done').write_text('done\n')
                else:
                    ok = False
            if not wait_for(log, 'DONE ', process, arguments.timeout):
                ok = False
            process.wait(timeout=30)
        except subprocess.TimeoutExpired:
            ok = False
        finally:
            if process.poll() is None:
                delay.stop(process)
                process.wait(timeout=15)
    text = log.read_text(errors='replace') if log.exists() else ''
    print(text, end='')
    ok &= 'DONE editor check' in text and 'FAIL' not in text
    after = delay.snapshot(guarded)
    if before != after:
        ok = False
        print(f'FAIL the normal REAPER resource directory {guarded} changed')
    print(f'REAPER exited with {process.returncode}')
    print('RESULT PASS' if ok else 'RESULT FAIL')
    return 0 if ok else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--format', choices=['clap', 'lv2', 'vst3'], required=True)
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--reaper', type=Path)
    parser.add_argument('--work', type=Path)
    parser.add_argument('--fx-name')
    parser.add_argument('--title', default='RAVET Delay', help='floating window title substring for the probe')
    parser.add_argument('--probe', type=Path, help='Linux: compiled x11-editor-probe')
    parser.add_argument('--size', type=lambda text: tuple(int(v) for v in text.split('x')),
                        help='editor size WIDTHxHEIGHT the probe looks for (default: the Delay, 560x340)')
    parser.add_argument('--action', action='append', default=[], help='probe action: x,y or drag:x,y,dy')
    parser.add_argument('--seconds', type=float, default=30.0, help='longest hold with the editor open')
    parser.add_argument('--settle', type=float, default=3.0, help='seconds before the host edit')
    parser.add_argument('--timeout', type=float, default=180.0)
    parser.add_argument('--ini', action='append', default=[], metavar='[SECTION]KEY=VALUE')
    parser.add_argument('--windows-root', metavar='WINDOWS_DIR')
    return run(parser.parse_args(argv))


if __name__ == '__main__':
    sys.exit(main())
