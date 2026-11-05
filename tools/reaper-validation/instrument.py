#!/usr/bin/env python3
"""Run deterministic instrument checks using delay.py's disposable REAPER runner.

Accepts the same --format/--bundle/--reaper/--work options; no normal REAPER
configuration is imported. MIDI output is checked by chaining a second instance
with the first instance's audio output disconnected.
"""
from pathlib import Path
import delay
import instrument_checks

_original_popen = delay.subprocess.Popen

def popen_in_work(command, *args, **kwargs):
    if "-cfgfile" in command:
        kwargs.setdefault("cwd", Path(command[command.index("-cfgfile") + 1]).parent.parent)
    return _original_popen(command, *args, **kwargs)

_original_prepare = delay.prepare

def prepare(work, fmt, bundle, fx_name, quit_after, extra_ini, env_path=False, windows_work=None,
            mac_arch=None):
    if windows_work is not None:
        raise SystemExit('instrument harness supports native macOS/Linux runs, not --windows-root')
    ini, script, env = _original_prepare(work, fmt, bundle, fx_name, quit_after, extra_ini, env_path,
                                         None, mac_arch)
    config = {'root': script.parent.as_posix(), 'resource': ini.parent.as_posix(),
              'format': fmt, 'rate': 48000, 'length': 2, 'quit': quit_after}
    script.write_text('RAVET_SYNTH = ' + delay.lua_value(config) + '\n' +
                      Path(__file__).with_suffix('.lua').read_text())
    return ini, script, env

if __name__ == '__main__':
    delay.__doc__ = __doc__
    delay.FORMATS = {key: value for key, value in delay.FORMATS.items() if key in ('clap', 'vst3', 'lv2')}
    delay.ISOLATED_BUNDLE_ID = "org.ravet.reaper-instrument-harness"
    delay.subprocess.Popen = popen_in_work
    delay.prepare = prepare
    delay.delay_checks = instrument_checks
    # The runner's source writer/config fields are unused by this instrument.
    raise SystemExit(delay.main())
