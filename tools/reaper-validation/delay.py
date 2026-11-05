#!/usr/bin/env python3
"""Scripted REAPER check of a built RAVET Delay bundle in one plug-in format.

Creates a disposable REAPER resource directory (an ini passed with -cfgfile; the
normal REAPER configuration and projects are never used), copies the bundle into
it and points only that format's plug-in path there, writes a synthetic impulse
source and the ReaScript, launches a new REAPER instance, waits for it to quit,
then checks the rendered WAV files and host log (delay_checks.py).

Example (macOS):
  python3 tools/reaper-validation/delay.py --format clap \\
      --bundle "target/clap/release/RAVET Delay.clap" \\
      --reaper /Applications/REAPER.app

Formats are selected by name; new formats need only an entry in FORMATS. AU
components cannot be pointed at a private directory: REAPER scans the system
component folders, so --format au requires a bundle already installed there.
"""
from __future__ import annotations

import argparse
import datetime
import json
import os
import platform
import shutil
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import delay_checks  # noqa: E402

ROOT = HERE.parents[1]


def platform_key() -> str:
    if sys.platform == 'darwin':
        return 'mac-arm64' if platform.machine() == 'arm64' else 'mac-x86_64'
    if sys.platform == 'win32':
        return 'windows'
    return 'linux'


# Format name → REAPER FX name prefix, ini plug-in path keys per platform, and
# the conventional search-path environment variable. Windows key names are not
# yet verified on a Windows host; extra keys are harmless and --ini overrides.
FORMATS = {
    'clap': {
        'prefix': 'CLAP: RAVET Delay',
        'keys': {'mac-arm64': ['clap_path_macos-aarch64'], 'mac-x86_64': ['clap_path_macos-x86_64'],
                 'linux': ['clap_path_linux-x86_64'],
                 'windows': ['clap_path_win64', 'clap_path_windows-x86_64', 'clap_path_win-x86_64']},
        'env': 'CLAP_PATH',
    },
    'lv2': {
        'prefix': 'LV2: RAVET Delay',
        'keys': {'mac-arm64': ['lv2path_mac'], 'mac-x86_64': ['lv2path_mac'], 'linux': ['lv2path_linux'],
                 'windows': ['lv2path_win64', 'lv2path_win']},
        'env': 'LV2_PATH',
    },
    'vst3': {
        'prefix': 'VST3: RAVET Delay',
        'keys': {'mac-arm64': ['vstpath_arm64'], 'mac-x86_64': ['vstpath64'], 'linux': ['vstpath'],
                 'windows': ['vstpath64']},
        'env': None,
    },
    'au': {
        'prefix': 'AU: RAVET: RAVET Delay',
        'keys': {},
        'env': None,
        'system_installed': True,
    },
}

RATE = 48_000
TEMPO = 150.0
FIXED_MS = 100.0
DIVISION_EIGHTH = 6  # "1/8" in examples/delay-plugin/src/dsp.rs DIVISIONS
EIGHTH_BEATS = 0.5


def default_reaper() -> Path | None:
    candidates = {
        'darwin': ['/Applications/REAPER.app', '/Applications/REAPER64.app'],
        'win32': [r'C:\Program Files\REAPER (x64)\reaper.exe', r'C:\REAPER\reaper.exe'],
    }.get(sys.platform, ['/usr/bin/reaper', '/opt/REAPER/reaper', str(Path.home() / 'opt/REAPER/reaper')])
    for candidate in candidates:
        if Path(candidate).exists():
            return Path(candidate)
    found = shutil.which('reaper')
    return Path(found) if found else None


ISOLATED_BUNDLE_ID = 'org.ravet.reaper-harness'


def isolated_app(reaper: Path) -> Path:
    """Private macOS copy of REAPER.app with its own bundle identifier.

    macOS keys its crash/window-restoration prompts to the bundle identifier,
    so a terminated test instance would otherwise make the normal REAPER (and
    every later run) stop at a "reopen windows?" dialog. The copy is cached
    under target/reaper-delay and re-signed ad hoc. Only this private
    identifier's preferences and saved window state are ever changed.
    """
    import plistlib
    copy = ROOT / 'target' / 'reaper-delay' / 'app' / 'REAPER.app'
    stamp = copy.parent / 'source.txt'
    source_id = f'{reaper.resolve()} {(reaper / "Contents" / "MacOS" / "REAPER").stat().st_mtime}'
    if not copy.exists() or not stamp.exists() or stamp.read_text() != source_id:
        if copy.exists():
            shutil.rmtree(copy)
        copy.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(reaper, copy, symlinks=True)
        info = copy / 'Contents' / 'Info.plist'
        with info.open('rb') as handle:
            plist = plistlib.load(handle)
        plist['CFBundleIdentifier'] = ISOLATED_BUNDLE_ID
        with info.open('wb') as handle:
            plistlib.dump(plist, handle)
        subprocess.run(['codesign', '--force', '--deep', '--sign', '-', str(copy)], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        stamp.write_text(source_id)
    saved = Path.home() / 'Library' / 'Saved Application State' / f'{ISOLATED_BUNDLE_ID}.savedState'
    if saved.exists():
        shutil.rmtree(saved)
    for key, value in (('ApplePersistenceIgnoreState', 'YES'), ('NSQuitAlwaysKeepsWindows', 'NO')):
        subprocess.run(['defaults', 'write', ISOLATED_BUNDLE_ID, key, '-bool', value], check=False)
    return copy


def user_resource_directory() -> Path | None:
    """The normal REAPER resource directory, which the harness must not change."""
    if sys.platform == 'darwin':
        path = Path.home() / 'Library' / 'Application Support' / 'REAPER'
    elif sys.platform == 'win32':
        path = Path(os.environ.get('APPDATA', Path.home())) / 'REAPER'
    else:
        path = Path(os.environ.get('XDG_CONFIG_HOME', Path.home() / '.config')) / 'REAPER'
    return path if path.is_dir() else None


def snapshot(directory: Path | None) -> dict[str, tuple[int, int]]:
    """Top-level file sizes and modification times (REAPER writes its ini and caches there)."""
    if directory is None:
        return {}
    return {entry.name: (entry.stat().st_size, entry.stat().st_mtime_ns)
            for entry in directory.iterdir() if entry.is_file()}


def stop(process: subprocess.Popen, kill: bool = False) -> None:
    if process.poll() is not None:
        return
    if sys.platform == 'win32':
        subprocess.run(['taskkill', '/T', '/F', '/PID', str(process.pid)], check=False,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return
    import signal
    try:
        os.killpg(process.pid, signal.SIGKILL if kill else signal.SIGTERM)
    except ProcessLookupError:
        pass


def executable(reaper: Path) -> Path:
    if reaper.suffix == '.app':
        return reaper / 'Contents' / 'MacOS' / 'REAPER'
    return reaper


def cases() -> list[dict]:
    synced_ms = delay_checks.synced_delay_ms(EIGHTH_BEATS, TEMPO)
    return [
        {'name': 'fixed', 'sync': False, 'time_ms': FIXED_MS, 'division': DIVISION_EIGHTH, 'mix': 100.0,
         'delay_ms': FIXED_MS},
        {'name': 'recalled', 'sync': False, 'time_ms': FIXED_MS, 'division': DIVISION_EIGHTH, 'mix': 100.0,
         'delay_ms': FIXED_MS},
        {'name': 'synced', 'sync': True, 'time_ms': FIXED_MS, 'division': DIVISION_EIGHTH, 'mix': 50.0,
         'delay_ms': synced_ms},
    ]


def lua_value(value) -> str:
    if isinstance(value, bool):
        return 'true' if value else 'false'
    if isinstance(value, (int, float)):
        return repr(float(value)) if isinstance(value, float) else str(value)
    if isinstance(value, str):
        return '[==[' + value + ']==]'
    if isinstance(value, list):
        return '{' + ', '.join(lua_value(v) for v in value) + '}'
    if isinstance(value, dict):
        return '{' + ', '.join(f'{k} = {lua_value(v)}' for k, v in value.items()) + '}'
    raise TypeError(type(value))


def prepare(work: Path, fmt: str, bundle: Path | None, fx_name: str, quit_after: bool,
            extra_ini: list[str], env_path: bool = False) -> tuple[Path, Path, dict]:
    spec = FORMATS[fmt]
    work.mkdir(parents=True, exist_ok=False)
    resource = work / 'resource'
    output = work / 'output'
    resource.mkdir()
    output.mkdir()
    env = dict(os.environ)
    ini = {
        # Keep the disposable instance quiet: no autosave or last-project
        # reload; render dialogs close when done.
        'autosaveint': '0', 'autosavemode': '0', 'loadlastproj': '0', 'renderclosewhendone': '1',
        # Skip REAPER's startup VST scan (CLAP and LV2 are still scanned): the
        # default VST path always includes the system VST3 folders, where one
        # unresponsive third-party plug-in can stall startup. VST3 runs need it.
        'vst_scan': '0' if fmt == 'vst3' else '2',
    }
    # Point the VST path at an empty private directory unless VST3 is under
    # test (REAPER still appends the system VST3 folders).
    empty = work / 'plugins' / 'empty'
    empty.mkdir(parents=True)
    for key in FORMATS['vst3']['keys'].get(platform_key(), []):
        ini[key] = str(empty)
    if spec.get('system_installed'):
        if bundle is not None:
            print(f'note: {fmt} is scanned from the system plug-in folders; --bundle {bundle} is only recorded')
    else:
        if bundle is None or not bundle.exists():
            raise SystemExit(f'error: --bundle is required and must exist for {fmt}: {bundle}')
        plugins = work / 'plugins' / fmt
        plugins.mkdir(parents=True)
        destination = plugins / bundle.name
        if bundle.is_dir():
            shutil.copytree(bundle, destination, symlinks=True)
        else:
            shutil.copy2(bundle, destination)
        keys = spec['keys'].get(platform_key(), [])
        for key in keys:
            ini[key] = str(plugins)
        if spec['env'] and env_path:
            # Off by default: the variable also reaches any REAPER started with
            # another configuration, which would then scan this bundle too.
            env[spec['env']] = str(plugins)
    for item in extra_ini:
        key, _, value = item.partition('=')
        ini[key] = value
    lines = ['[REAPER]'] + [f'{k}={v}' for k, v in ini.items()]
    (resource / 'reaper.ini').write_text('\n'.join(lines) + '\n')
    delay_checks.write_input(output / 'input.wav', RATE)
    config = {
        'format': fmt, 'fx_name': fx_name, 'rate': RATE, 'tempo': TEMPO,
        'length': delay_checks.LENGTH_SECONDS, 'bundle': str(bundle) if bundle else None,
        'cases': cases(),
    }
    (output / 'cases.json').write_text(json.dumps(config, indent=2) + '\n')
    lua_config = {'root': output.as_posix(), 'resource': resource.as_posix(), 'format': fmt, 'fx_name': fx_name, 'rate': RATE,
                  'tempo': TEMPO, 'length': delay_checks.LENGTH_SECONDS, 'quit': quit_after,
                  'cases': config['cases']}
    script = output / 'ravet-delay-check.lua'
    script.write_text('RAVET_DELAY = ' + lua_value(lua_config) + '\n' + (HERE / 'delay.lua').read_text())
    return resource / 'reaper.ini', script, env


def run(arguments: argparse.Namespace) -> int:
    fmt = arguments.format
    fx_name = arguments.fx_name or FORMATS[fmt]['prefix']
    stamp = datetime.datetime.now().strftime('%Y%m%d-%H%M%S')
    work = (arguments.work or ROOT / 'target' / 'reaper-delay' / f'{fmt}-{stamp}').resolve()
    ini, script, env = prepare(work, fmt, arguments.bundle.resolve() if arguments.bundle else None, fx_name,
                               not arguments.no_quit, arguments.ini, arguments.env_path)
    output = script.parent
    reaper = arguments.reaper or default_reaper()
    if reaper is None:
        raise SystemExit('error: REAPER not found; pass --reaper')
    if reaper.suffix == '.app' and not arguments.no_isolate and not arguments.dry_run:
        reaper = isolated_app(reaper)
    command = [str(executable(reaper)), '-cfgfile', str(ini), '-nosplash', '-ignoreerrors', '-newinst', str(script)]
    print('work directory:', work)
    print('command:', ' '.join(f'"{c}"' if ' ' in c else c for c in command))
    if arguments.dry_run:
        return 0
    guarded = user_resource_directory()
    before = snapshot(guarded)
    started = time.monotonic()
    with open(work / 'reaper-stdout.log', 'wb') as stdout:
        # A new process group/session lets a timeout stop REAPER together with
        # any plug-in scan helpers it started.
        group = {'creationflags': subprocess.CREATE_NEW_PROCESS_GROUP} if sys.platform == 'win32' \
            else {'start_new_session': True}
        process = subprocess.Popen(command, stdout=stdout, stderr=subprocess.STDOUT, env=env,
                                   stdin=subprocess.DEVNULL, **group)
        log = output / 'delay-host.log'
        finished_at = None
        while process.poll() is None:
            elapsed = time.monotonic() - started
            text = log.read_text(errors='replace') if log.exists() else ''
            if 'ABORT ' in text:
                # REAPER is not using the disposable configuration: stop it
                # without letting it save any settings on quit.
                print('REAPER is not using the disposable resource directory; killing it')
                stop(process, kill=True)
                break
            if 'DONE ' in text and finished_at is None:
                finished_at = time.monotonic()
            if finished_at is not None and time.monotonic() - finished_at > arguments.quit_grace:
                print(f'REAPER did not quit {arguments.quit_grace:g} s after the script finished; terminating it')
                stop(process)
                break
            if elapsed > arguments.timeout:
                print(f'TIMEOUT after {arguments.timeout:g} s; host log so far:\n{text or "(none)"}')
                print('A blocking dialog (evaluation, audio device, first run or scan) is the usual cause.')
                stop(process)
                break
            time.sleep(0.5)
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            stop(process, kill=True)
    print(f'REAPER exited with {process.returncode} after {time.monotonic() - started:.1f} s')
    ok, lines = delay_checks.check_directory(output)
    after = snapshot(guarded)
    changed = sorted(name for name in set(before) | set(after) if before.get(name) != after.get(name))
    if changed:
        ok = False
        lines.append(f'FAIL the normal REAPER resource directory {guarded} changed during the run: {", ".join(changed)}')
    elif guarded is not None:
        lines.append(f'PASS normal REAPER resource directory unchanged ({guarded})')
    lines.append('RESULT PASS' if ok else 'RESULT FAIL')
    (output / 'render-check.log').write_text('\n'.join(lines) + '\n')
    host_log = output / 'delay-host.log'
    if host_log.exists():
        print(host_log.read_text(errors='replace'), end='')
    print('\n'.join(lines))
    return 0 if ok else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--format', required=True, choices=sorted(FORMATS))
    parser.add_argument('--bundle', type=Path, help='built bundle (.clap, .lv2, .vst3 directory or file)')
    parser.add_argument('--reaper', type=Path, help='REAPER executable or REAPER.app (default: platform search)')
    parser.add_argument('--work', type=Path, help='new work directory (default: target/reaper-delay/<format>-<time>)')
    parser.add_argument('--fx-name', help='REAPER FX name prefix to insert (default: per format)')
    parser.add_argument('--timeout', type=float, default=240.0, help='seconds before REAPER is terminated')
    parser.add_argument('--quit-grace', type=float, default=20.0, help='seconds to wait for quit after the script')
    parser.add_argument('--ini', action='append', default=[], metavar='KEY=VALUE',
                        help='extra [REAPER] ini entry for the disposable configuration')
    parser.add_argument('--no-isolate', action='store_true',
                        help='macOS: run REAPER.app itself instead of a private copy with its own bundle ID')
    parser.add_argument('--env-path', action='store_true',
                        help='also export CLAP_PATH/LV2_PATH for the format (off by default)')
    parser.add_argument('--no-quit', action='store_true', help='leave REAPER open after the script')
    parser.add_argument('--dry-run', action='store_true', help='prepare the work directory and print the command')
    return run(parser.parse_args(argv))


if __name__ == '__main__':
    sys.exit(main())
