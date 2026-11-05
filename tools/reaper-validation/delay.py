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

Windows hosts without Python: --windows-root C:\\dir prepares the work
directory on this machine with every path rewritten for <root>\\<work name>
and adds run-windows.ps1, which snapshots the normal REAPER directories,
launches REAPER, waits, stops only that process and snapshots again. Copy the
work directory there, run the script, copy it back and pass it to
--check-only, which runs the render checks and compares the snapshots.
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
from pathlib import Path, PureWindowsPath

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
# the conventional search-path environment variable. Extra keys are harmless
# and --ini overrides. The Windows keys are the ones REAPER 7.82 x64 names in
# reaper.exe and kept in its ini after a Windows run.
FORMATS = {
    'clap': {
        'prefix': 'CLAP: RAVET Delay',
        'keys': {'mac-arm64': ['clap_path_macos-aarch64'], 'mac-x86_64': ['clap_path_macos-x86_64'],
                 'linux': ['clap_path_linux-x86_64'],
                 'windows': ['clap_path_win64']},
        'env': 'CLAP_PATH',
    },
    'lv2': {
        'prefix': 'LV2: RAVET Delay',
        'keys': {'mac-arm64': ['lv2path_mac'], 'mac-x86_64': ['lv2path_mac'], 'linux': ['lv2path_linux'],
                 'windows': ['lv2path_win64']},
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
            extra_ini: list[str], env_path: bool = False,
            windows_work: PureWindowsPath | None = None) -> tuple[Path, Path, dict]:
    """Write the work directory. With `windows_work`, every path written into
    the ini and the script names that Windows location instead (the directory
    is copied there and run with run-windows.ps1)."""
    spec = FORMATS[fmt]
    key_platform = 'windows' if windows_work is not None else platform_key()

    def host(path: Path) -> str:
        if windows_work is None:
            return str(path)
        return str(windows_work.joinpath(*path.relative_to(work).parts))
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
        # No online version check: its "New Version Notification" dialog
        # stopped Windows runs mid-script.
        'verchk': '0',
        # Skip REAPER's startup VST scan (CLAP and LV2 are still scanned): the
        # default VST path always includes the system VST3 folders, where one
        # unresponsive third-party plug-in can stall startup. VST3 runs need it.
        'vst_scan': '0' if fmt == 'vst3' else '2',
    }
    # Point the VST path at an empty private directory unless VST3 is under
    # test (REAPER still appends the system VST3 folders).
    empty = work / 'plugins' / 'empty'
    empty.mkdir(parents=True)
    for key in FORMATS['vst3']['keys'].get(key_platform, []):
        ini[key] = host(empty)
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
        keys = spec['keys'].get(key_platform, [])
        for key in keys:
            ini[key] = host(plugins)
        if spec['env'] and env_path:
            # Off by default: the variable also reaches any REAPER started with
            # another configuration, which would then scan this bundle too.
            env[spec['env']] = str(plugins)
    sections: dict[str, dict[str, str]] = {'REAPER': ini}
    if key_platform == 'windows':
        # A fresh Windows configuration asks for an audio device at startup.
        # WaveOut (mode 2) opens without a dialog and keeps REAPER's audio
        # thread running, which CLAP parameter flushes need; with no audio
        # system, host edits never reached the plug-in.
        sections['audioconfig'] = {'mode': '2'}
    for item in extra_ini:
        # KEY=VALUE goes to [REAPER]; [SECTION]KEY=VALUE to another section.
        section = 'REAPER'
        if item.startswith('[') and ']' in item:
            section, _, item = item[1:].partition(']')
        key, _, value = item.partition('=')
        sections.setdefault(section, {})[key] = value
    lines = []
    for section, entries in sections.items():
        lines += [f'[{section}]'] + [f'{k}={v}' for k, v in entries.items()]
    (resource / 'reaper.ini').write_text('\n'.join(lines) + '\n')
    delay_checks.write_input(output / 'input.wav', RATE)
    config = {
        'format': fmt, 'fx_name': fx_name, 'rate': RATE, 'tempo': TEMPO,
        'length': delay_checks.LENGTH_SECONDS, 'bundle': str(bundle) if bundle else None,
        'cases': cases(),
    }
    if windows_work is not None:
        config['windows_work'] = str(windows_work)
    (output / 'cases.json').write_text(json.dumps(config, indent=2) + '\n')
    root = host(output) if windows_work is not None else output.as_posix()
    resource_name = host(resource) if windows_work is not None else resource.as_posix()
    lua_config = {'root': root, 'resource': resource_name, 'format': fmt, 'fx_name': fx_name, 'rate': RATE,
                  'tempo': TEMPO, 'length': delay_checks.LENGTH_SECONDS, 'quit': quit_after,
                  'cases': config['cases']}
    script = output / 'ravet-delay-check.lua'
    script.write_text('RAVET_DELAY = ' + lua_value(lua_config) + '\n' + (HERE / 'delay.lua').read_text())
    return resource / 'reaper.ini', script, env


# PowerShell runner for a Windows host without Python. It mirrors run(): the
# same command, timeout and quit grace, the ABORT guard, and before/after
# snapshots (size, modification time, SHA-256) of the top-level files in the
# normal REAPER resource directory and in REAPER's own program directory, which
# a portable install uses for its configuration. It stops only the process it
# started (and that process's children).
WINDOWS_RUNNER = r'''# Generated by tools/reaper-validation/delay.py --windows-root; do not edit.
param([double]$Timeout = __TIMEOUT__, [double]$QuitGrace = __GRACE__)
$ErrorActionPreference = 'Stop'
$work = Split-Path -Parent $MyInvocation.MyCommand.Path
$reaper = '__REAPER__'
$guarded = @((Join-Path $env:APPDATA 'REAPER'), (Split-Path -Parent $reaper))
$output = Join-Path $work 'output'
$hostLog = Join-Path $output 'delay-host.log'
$runLog = Join-Path $work 'run-windows.log'
function Note([string]$text) { Add-Content -LiteralPath $runLog -Value $text; Write-Output $text }
function Snapshot([string]$path) {
  $lines = foreach ($directory in $guarded) {
    if (Test-Path -LiteralPath $directory) {
      Get-ChildItem -LiteralPath $directory -File -Force | Sort-Object FullName | ForEach-Object {
        $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
        "{0}`t{1}`t{2}`t{3}" -f $_.FullName, $_.Length, $_.LastWriteTimeUtc.Ticks, $hash
      }
    } else { "{0}`tmissing" -f $directory }
  }
  Set-Content -LiteralPath $path -Value $lines -Encoding UTF8
}
Remove-Item -LiteralPath $runLog, (Join-Path $work 'run-windows.done') -ErrorAction SilentlyContinue
Note ('user {0}\{1} session {2} interactive {3}' -f $env:USERDOMAIN, $env:USERNAME,
  (Get-Process -Id $PID).SessionId, [Environment]::UserInteractive)
Note ('reaper {0} version {1}' -f $reaper, (Get-Item -LiteralPath $reaper).VersionInfo.ProductVersion)
Snapshot (Join-Path $work 'guard-before.tsv')
$arguments = '-cfgfile "__INI__" -nosplash -ignoreerrors -newinst "__SCRIPT__"'
Note ('command: "{0}" {1}' -f $reaper, $arguments)
$started = Get-Date
$process = Start-Process -FilePath $reaper -ArgumentList $arguments -PassThru `
  -RedirectStandardOutput (Join-Path $work 'reaper-stdout.log') -RedirectStandardError (Join-Path $work 'reaper-stderr.log')
$null = $process.Handle  # keep the handle so ExitCode is available after exit
Note ('started pid {0}' -f $process.Id)
$finished = $null
$outcome = 'exited'
while (-not $process.HasExited) {
  $elapsed = ((Get-Date) - $started).TotalSeconds
  $text = if (Test-Path -LiteralPath $hostLog) { Get-Content -LiteralPath $hostLog -Raw -ErrorAction SilentlyContinue } else { '' }
  if ($text -match 'ABORT ') { $outcome = 'abort'; break }
  if ($text -match 'DONE ' -and $null -eq $finished) { $finished = Get-Date }
  if ($null -ne $finished -and ((Get-Date) - $finished).TotalSeconds -gt $QuitGrace) { $outcome = 'quit-grace'; break }
  if ($elapsed -gt $Timeout) { $outcome = 'timeout'; break }
  Start-Sleep -Milliseconds 500
}
if (-not $process.HasExited) {
  Note ('stopping pid {0} ({1}); host log so far:' -f $process.Id, $outcome)
  if (Test-Path -LiteralPath $hostLog) { Note (Get-Content -LiteralPath $hostLog -Raw) } else { Note '(none)' }
  & taskkill.exe /T /F /PID $process.Id | Out-Null
  $process.WaitForExit(15000) | Out-Null
}
Note ('outcome {0}; REAPER exited with {1} after {2:N1} s' -f $outcome, $process.ExitCode, ((Get-Date) - $started).TotalSeconds)
Snapshot (Join-Path $work 'guard-after.tsv')
Set-Content -LiteralPath (Join-Path $work 'run-windows.done') -Value $outcome
'''


def windows_runner(work: Path, windows_work: PureWindowsPath, reaper: PureWindowsPath, timeout: float,
                   grace: float) -> Path:
    for value in (str(windows_work), str(reaper)):
        if "'" in value or '"' in value:
            raise SystemExit(f'error: quotes are not supported in Windows paths: {value}')
    text = (WINDOWS_RUNNER.replace('__TIMEOUT__', f'{timeout:g}').replace('__GRACE__', f'{grace:g}')
            .replace('__REAPER__', str(reaper))
            .replace('__INI__', str(windows_work / 'resource' / 'reaper.ini'))
            .replace('__SCRIPT__', str(windows_work / 'output' / 'ravet-delay-check.lua')))
    path = work / 'run-windows.ps1'
    # CRLF and a BOM: Windows PowerShell 5.1 reads BOM-less scripts as ANSI.
    path.write_bytes(b'\xef\xbb\xbf' + text.replace('\n', '\r\n').encode('utf-8'))
    return path


def read_snapshot(path: Path) -> dict[str, str]:
    entries = {}
    for line in path.read_text(encoding='utf-8-sig').splitlines():
        if line.strip():
            name, _, rest = line.partition('\t')
            entries[name] = rest
    return entries


def compare_snapshots(work: Path) -> tuple[bool, list[str]]:
    """Compare guard-before.tsv and guard-after.tsv written by run-windows.ps1."""
    before_path, after_path = work / 'guard-before.tsv', work / 'guard-after.tsv'
    if not before_path.exists() or not after_path.exists():
        return False, [f'FAIL guard snapshots missing in {work} (run-windows.ps1 did not finish)']
    before, after = read_snapshot(before_path), read_snapshot(after_path)
    if not before:
        return False, ['FAIL guard snapshot is empty']
    changed = sorted(name for name in set(before) | set(after) if before.get(name) != after.get(name))
    if changed:
        return False, [f'FAIL normal REAPER files changed during the run: {", ".join(changed)}']
    directories = sorted({str(PureWindowsPath(name).parent) for name in before if not before[name] == 'missing'})
    return True, [f'PASS normal REAPER files unchanged ({len(before)} files: size, mtime, SHA-256) in '
                  + ', '.join(directories)]


def check_only(work: Path) -> int:
    """Check a work directory run elsewhere (for example with run-windows.ps1)."""
    output = work / 'output'
    ok, lines = delay_checks.check_directory(output)
    if (work / 'run-windows.ps1').exists():
        guard_ok, guard_lines = compare_snapshots(work)
        ok = ok and guard_ok
        lines += guard_lines
        run_log = work / 'run-windows.log'
        if run_log.exists():
            print(run_log.read_text(encoding='utf-8-sig', errors='replace'), end='')
    lines.append('RESULT PASS' if ok else 'RESULT FAIL')
    (output / 'render-check.log').write_text('\n'.join(lines) + '\n')
    host_log = output / 'delay-host.log'
    if host_log.exists():
        print(host_log.read_text(errors='replace'), end='')
    print('\n'.join(lines))
    return 0 if ok else 1


def run(arguments: argparse.Namespace) -> int:
    fmt = arguments.format
    fx_name = arguments.fx_name or FORMATS[fmt]['prefix']
    stamp = datetime.datetime.now().strftime('%Y%m%d-%H%M%S')
    work = (arguments.work or ROOT / 'target' / 'reaper-delay' / f'{fmt}-{stamp}').resolve()
    windows_work = PureWindowsPath(arguments.windows_root) / work.name if arguments.windows_root else None
    ini, script, env = prepare(work, fmt, arguments.bundle.resolve() if arguments.bundle else None, fx_name,
                               not arguments.no_quit, arguments.ini, arguments.env_path, windows_work)
    output = script.parent
    if windows_work is not None:
        reaper = PureWindowsPath(str(arguments.reaper) if arguments.reaper else r'C:\REAPER\reaper.exe')
        runner = windows_runner(work, windows_work, reaper, arguments.timeout, arguments.quit_grace)
        print('work directory:', work)
        print('copy it to:', windows_work)
        print('then run there: powershell -NoProfile -ExecutionPolicy Bypass -File', windows_work / runner.name)
        print('copy it back and check: python3 tools/reaper-validation/delay.py --check-only', work)
        return 0
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
    parser.add_argument('--format', choices=sorted(FORMATS), help='plug-in format (required unless --check-only)')
    parser.add_argument('--bundle', type=Path, help='built bundle (.clap, .lv2, .vst3 directory or file)')
    parser.add_argument('--reaper', type=Path, help='REAPER executable or REAPER.app (default: platform search)')
    parser.add_argument('--work', type=Path, help='new work directory (default: target/reaper-delay/<format>-<time>)')
    parser.add_argument('--fx-name', help='REAPER FX name prefix to insert (default: per format)')
    parser.add_argument('--timeout', type=float, default=240.0, help='seconds before REAPER is terminated')
    parser.add_argument('--quit-grace', type=float, default=20.0, help='seconds to wait for quit after the script')
    parser.add_argument('--ini', action='append', default=[], metavar='[SECTION]KEY=VALUE',
                        help='extra ini entry for the disposable configuration ([REAPER] unless a section is given)')
    parser.add_argument('--no-isolate', action='store_true',
                        help='macOS: run REAPER.app itself instead of a private copy with its own bundle ID')
    parser.add_argument('--env-path', action='store_true',
                        help='also export CLAP_PATH/LV2_PATH for the format (off by default)')
    parser.add_argument('--no-quit', action='store_true', help='leave REAPER open after the script')
    parser.add_argument('--dry-run', action='store_true', help='prepare the work directory and print the command')
    parser.add_argument('--windows-root', metavar='WINDOWS_DIR',
                        help='prepare only, for a Windows host without Python: paths name WINDOWS_DIR\\<work name>, '
                             'run-windows.ps1 runs REAPER there; --reaper is then the Windows path '
                             '(default C:\\REAPER\\reaper.exe)')
    parser.add_argument('--check-only', type=Path, metavar='WORK',
                        help='check an existing work directory (renders, host log, Windows guard snapshots)')
    arguments = parser.parse_args(argv)
    if arguments.check_only:
        return check_only(arguments.check_only.resolve())
    if not arguments.format:
        parser.error('--format is required')
    return run(arguments)


if __name__ == '__main__':
    sys.exit(main())
