# REAPER host validation

The public gain CLAP bundle has automated ABI/native-loop fixtures. Actual DAW
checks use a separate disposable REAPER resource directory and generated public
stereo audio, with results recorded in [work status](WORK_STATUS.md). A successful
run supports only the tested host/version/platform/paths; it does not establish
all hosts, every UI interaction, production signing or callback deadlines.

## Prepare a disposable session

Build the bundle with `python3 scripts/bundle-clap.py`. Use an official REAPER
installation or evaluation build. Give the test copy its own resource directory
via REAPER's `-cfgfile /absolute/path/reaper.ini` option. Configure its CLAP path
to the absolute `target/clap/release` directory. Keep test settings and files in
`target/reaper-validation`; do not use an existing production project or licence
fixtures. REAPER may automatically append system VST3 paths and perform a startup
scan even when explicit test paths are provided.

Generate the synthetic four-second stereo PCM source:

```sh
python3 tools/reaper-validation/prepare.py target/reaper-validation/config
```

In an empty disposable REAPER project, use Actions → Show action list → ReaScript
Load to select `tools/reaper-validation/validate.lua`, then run it. The script
asserts the project has no tracks before mutation. It mutes master volume for
hardware output, instantiates RAVET Gain, checks parameters and window creation,
plays automation, saves wet/dry/automation projects and reopens saved sessions.
The generated files and `host-validation.log` go into
`<REAPER resource directory>/ravet-validation`. The script uses REAPER's public
ReaScript API and does not change the normal user resource directory.

The script identifies actual plug-in parameters separately from REAPER's
index-prefixed wrapper identifiers, such as `2:bypass`, `3:wet` and `4:delta`. Host window handle existence verifies
creation/reopening, not visual layout, GUI gesture recording or live meter
rendering. If the host prompts about audio configuration or evaluation, complete
its normal startup before running the script.

## Check actual DAW audio renders

The script requests selected-track stem rendering so master hardware volume can
remain zero. In REAPER 7.82, reopening these generated projects or using the
“most recent render settings” action selected Master mix during this experiment.
Always check the actual render dialog before starting: select **Selected tracks
(stems)**, keep the validation track selected, set 48 kHz stereo PCM, four-second
custom bounds and no tail/dither/postprocessing. Render `wet.rpp`, `dry.rpp`, and
`automation.rpp` separately, using their corresponding output names. A master mix
render is silent because hardware output is deliberately muted. The tested path
uses Render to File; command-line rendering remains unverified.

```sh
python3 tools/reaper-validation/check-renders.py target/reaper-validation/config/ravet-validation
```

The checker verifies four-second, 48 kHz stereo renders against known gain,
bypass and stepped-automation output, excluding source fade and smoother settling
regions. Source amplitudes are +0.25 left and −0.5 right. Wet gain is 1.5, dry
bypass is on, and automation holds gain 0.5 before two seconds and 2 afterwards.
These are actual DAW-render checks; they do not measure real-time deadlines.

## Visual and interaction checks

Load and run `tools/reaper-validation/ui-check.lua` after the session harness.
It opens the generated wet project, keeps master output silent, loops the source
and uses Write mode to record gain edits. It opens the FX-chain editor; if REAPER
restores a floating wrapper, close it or use “Window float selected FX” to embed
the page. Resize/zoom the FX-chain window, drag gain away from 1.5, then enable
plug-in bypass. The observer checks dry stereo peaks and changed gain automation
points, stops playback and saves `ui-recording.rpp` and `ui-validation.log`.
Terminate the action from REAPER if abandoning the interaction check; it defers
until the gain/bypass conditions are met.

Compare input/output meters and reload a saved session.
Check that GUI changes reach host controls/automation, host changes reach the
page, and closing/reopening the editor during playback preserves audio. Record
exact observations and any missing UI automation evidence. Native fixture
screenshots alone cannot establish visual behavior inside REAPER.

## Executed scope

REAPER 7.82 on macOS 27.0.1 arm64 passed discovery, gain/bypass host edits, two
saved-session recalls, editor reopening, two automation playback values and live
stereo peaks. Actual 24-bit PCM stems matched every checked sample in wet, bypass
and both automation regions. Native interaction passed numeric gain, slider drag,
bypass audio/meter changes and recorded gain automation. FX-chain zoom expanded
the page to its bounded maximum and restoring the window returned its smaller
layout without clipping. Floating-wrapper corner dragging did not resize during
this test; it is not claimed as validated. See [work status](WORK_STATUS.md) for
test observations and remaining support limits.


## VST3 reference checks

Build the public VST3 bundle with `scripts/bundle-vst3.py`, prepare the same
synthetic `input.wav` under a separate disposable resource directory, and add
the built VST3 directory to that test host's plug-in paths. Finish startup and
select its audio device before loading `tools/reaper-validation/native-formats.lua`.
Do not run the action during startup: device/evaluation dialogs can interrupt
playback while deferred checks still execute. The script requires an empty
project, keeps master processing gain at unity, mutes every hardware-send volume
and identifies plug-in parameters
separately from REAPER's wrapper bypass/wet/delta.

This harness uses normalized gain values (physical gain divided by four), saves
and recalls complete wet/dry sessions, checks stereo live peaks, creates a gain
automation envelope and seeks between its two held values. It checks a generic
parameter-window handle; no custom VST3 editor is claimed. REAPER 7.82 on macOS
arm64 passed these paths after startup. A host-suspended bypassed edit exposed a
missing controller-state record; the adapter now persists current controller
controls and synchronizes validated controller restores with the processor.
The VST3 checks do not replace the separate CLAP render/GUI evidence above,
prove deadlines or establish other format/host/platform support.

An isolated test REAPER copy with a separate bundle identifier avoided macOS
restoring a crashed scanner's startup windows. Only scan metadata was reused
from the previous disposable resource directory; no licence fixtures or normal
user settings were copied. REAPER still appends system VST3 paths, so unrelated
plug-ins can block an initial scan even with an explicit test path.

## MIDI instrument checks

Build `scripts/bundle-clap.py --plugin sine`, add only the explicit target CLAP
path to the isolated test resource and finish startup/evaluation/device dialogs.
Run `tools/reaper-validation/sine-instrument.lua` only in an empty disposable
project. A rerun permits replacement only of its sole named synthetic test track.
The script silences every hardware send while retaining master processing gain,
inserts a generated timed A4 MIDI note, checks stereo live peaks and generic
controls, saves/restores level state and tests two held automation values. A bounded
transport-region wait keeps deferred UI timing from sampling outside the held note.
It leaves playback stopped. Generated projects/logs stay in the test resource.

Local REAPER 7.82/macOS arm64 passed all these checks on 9 October 2026, using the
already-running 96 kHz UMC1820 rate and silent hardware sends. Initial script
execution while startup/background audio was unavailable observed position zero;
that run failed and supplied no audio evidence. A subsequent preferences/device
reopen blocked inside Core Audio; restarting only the isolated copy and retaining
the existing device rate allowed the completed run. Do not change a production
project/device configuration or restart a global audio service to run this fixture.
These checks verify this host/version's MIDI/audio/state/automation path; they do
not prove physical audio quality, deadlines, custom instrument UI or other hosts.

## Sidechain auxiliary mapping and DAW samples

The [sidechain reference](sidechain-clap.md) uses a generated four-channel source,
stereo main pins and a mono detector routed from channel three. The guarded
`tools/reaper-validation/sidechain.lua` owns an empty/named disposable track,
silences hardware sends, sets the project media path, applies FX into a generated
take and records its exact file path. `check-sidechain-render.py` verifies the
first two rendered channels in four off/on regions; live peak display decay is
not used as a zero-sample assertion. REAPER's documented command-line `.lua`
runner works when native UI automation cannot access the host window. Use
explicit isolated app/configuration paths and inspect the actual script result;
`-nonewinst` may still create a separate process when an earlier host used
`-newinst` or a different path spelling. These are synthetic rendered samples,
not device/deadline or visual evidence.

## Native VST3 meter-forwarding harness

`prepare.py` also writes a public-fixture marker. In a new empty isolated REAPER
project, `native-meters.lua` requires that marker before making any host change.
It loads only the public RAVET gain VST3, mutes project hardware destinations,
plays the generated input and compares all twelve normalized peak/RMS/diagnostic
parameters. A floating-window handle is logged separately; it cannot prove
visual/assistive-technology behavior. It stops playback and closes its test FX
window, leaving the disposable project. It writes `native-meter-host.log` under
this resource directory's `ravet-validation` folder.

The first local attempt stalled in a startup modal while native UI automation
returned cgWindowNotFound. After owner readiness, the isolated REAPER 7.82/macOS
arm64 main window became inspectable. Loading/running the guarded script through
its action list passed all twelve normalized meter comparisons and the native
floating-window handle. Playback stopped and the editor closed afterward. This
is actual DAW forwarding evidence; no visual/assistive-tech/deadline claim. See
[work status](WORK_STATUS.md) for the retained historical attempts.
Guard fixtures execute with an optional installed Lua interpreter to verify
missing/incorrect marker and nonempty project rejection before host mutation.
See [decision 0101](decisions/0101-guarded-native-host-validation.md).

The native meter harness logs its enumerated FX count and public Gain candidate
names before the discovery assertion, making disposable-cache/path problems
visible without logging unrelated plug-in names. On macOS arm64, the native
REAPER VST path setting is `vstpath_arm64`; `vstpath64` alone does not configure
that host. Prefer its VST preferences for a committed path and scan.


## VST3 performance instrument harness

After building the [performance instrument](../examples/performance-vst3/README.md),
configure only its public bundle in a disposable REAPER resource with the
`prepare.py` marker. Run `performance-instrument.lua` in a new empty project.
Missing/incorrect markers and existing tracks reject before host mutation.
The harness mutes all project hardware sends, generates a timed A4 MIDI note,
renders stereo through the verified apply-FX action, recalls a saved Level state
and renders two held automation values. It saves synthetic projects/rendered
takes and `performance-host.log` inside the prepared resource. Playback remains
stopped. Run `python3 tools/reaper-validation/check-performance-render.py
/absolute/path/to/test-resource` to verify actual PCM stereo peaks/RMS, A4
frequency and pre/post-note silence. The checker rejects incomplete host logs
and file paths outside the synthetic resource.

The isolated REAPER 7.82/macOS arm64 fixture passed all three 48 kHz renders and
independent sample checks locally on 9 October 2026. An earlier live check stalled
with no advancing transport; it supplied no live audio evidence. Offline render
checks do not require functioning live device playback.

This fixture exercises REAPER's MIDI-to-VST3 note delivery. It does not establish
DAW note-expression editing, custom editor/accessibility behavior, physical audio
quality or deadline performance. SDK note-expression validation is separate.

## Scripted RAVET Delay check (any format, macOS/Linux/Windows)

`tools/reaper-validation/delay.py` runs the [tempo-synced delay](examples.md#tempo-synced-delay-plug-in)
through REAPER unattended, in one plug-in format per run, and checks the audio.
It needs no manual dialog handling when it works, and it never uses the normal
REAPER configuration or projects:

- It creates a new work directory (default `target/reaper-delay/<format>-<time>`)
  holding a disposable resource directory (`resource/reaper.ini`, passed with
  `-cfgfile`), a private copy of the bundle under `plugins/<format>`, the
  synthetic source and the generated ReaScript. Only that format's REAPER
  plug-in path points at the copy; the VST path points at an empty directory
  and the startup VST scan is skipped (`vst_scan=2`) unless VST3 is under test,
  because REAPER always appends the system VST3 folders and one unresponsive
  third-party plug-in there can stall startup.
- It launches `reaper -cfgfile <ini> -nosplash -ignoreerrors -newinst <script.lua>`
  in its own process group, waits for REAPER to quit (default timeout 240 s,
  after which REAPER and any scan helpers are stopped), then runs the checker.
- On macOS, `--reaper` may name `REAPER.app`; the driver then runs a cached
  private copy (`target/reaper-delay/app`) with its own bundle identifier,
  `org.ravet.reaper-harness`, re-signed ad hoc. macOS keys its "reopen windows
  after an unexpected quit" prompt to the bundle identifier, so a terminated
  test run would otherwise block every later launch, including the normal
  REAPER. `--no-isolate` disables the copy.
- Guards: the script refuses to touch the host unless
  `reaper.GetResourcePath()` is the disposable directory (it then logs `ABORT`
  and the driver kills REAPER without letting it save settings), and the driver
  fails the run if any top-level file in the normal REAPER resource directory
  (`~/Library/Application Support/REAPER`, `~/.config/REAPER`,
  `%APPDATA%\REAPER`) changed during it.

The ReaScript (`delay.lua`) logs REAPER's version and every installed
`RAVET Delay` entry, inserts the one whose name starts with the format prefix
(`CLAP: RAVET Delay`, `LV2: RAVET Delay`, `VST3: RAVET Delay`,
`AU: RAVET: RAVET Delay`; `--fx-name` overrides), finds the controls by name and
drives them in physical units when REAPER reports the physical range (CLAP,
LV2) or normalized units otherwise (VST3, AU). Every edit is read back after the
plug-in has flushed it. It renders the master mix (48 kHz, 4 s, no tail) to
`fixed.wav` (sync off, time 100 ms, feedback 0, mix 100), saves and reopens the
project and checks every control, renders `recalled.wav`, then sets sync on,
division 1/8 and mix 50 at 150 BPM, renders `synced.wav`, saves, reopens and
checks the controls and tempo again, and quits REAPER. Hardware sends of the
master are muted; nothing is played live.

`delay_checks.py` reads the renders. The source holds stereo impulses at frames
24000 and 96000 (+0.5 left, −0.25 right). Each render must hold the dry impulse
scaled by 1 − mix at those frames, one echo scaled by mix exactly 4800 frames
(100 ms) or 9600 frames (1/8 at 150 BPM = 200 ms) later, and silence (error ≤
1e-4) elsewhere. It prints `PASS`/`FAIL` lines and `RESULT PASS|FAIL`, also
written to `output/render-check.log`; the driver exits non-zero on failure.
`python3 tools/reaper-validation/delay_checks.py <work>/output` re-checks a run.

Build the bundle first (only the delay crate is built), then run per OS:

```sh
# macOS
python3 scripts/bundle-clap.py --plugin delay --profile release
python3 scripts/bundle-lv2.py --plugin delay --sdk "$(brew --prefix lv2)"
python3 tools/reaper-validation/delay.py --format clap \
    --bundle "target/clap/release/RAVET Delay.clap" --reaper /Applications/REAPER.app
python3 tools/reaper-validation/delay.py --format lv2 \
    --bundle target/lv2/ravet_delay.lv2 --reaper /Applications/REAPER.app

# Linux (from a desktop session, or export its variables over SSH, e.g.
# XDG_RUNTIME_DIR=/run/user/$(id -u) WAYLAND_DISPLAY=wayland-1 DISPLAY=:0)
python3 scripts/bundle-clap.py --plugin delay --profile release
python3 scripts/bundle-lv2.py --plugin delay --sdk /path/to/lv2-1.18.10
python3 tools/reaper-validation/delay.py --format clap \
    --bundle "target/clap/release/RAVET Delay.clap" --reaper /usr/bin/reaper
python3 tools/reaper-validation/delay.py --format lv2 \
    --bundle target/lv2/ravet_delay.lv2 --reaper /usr/bin/reaper
```

If the log shows `Installed RAVET Delay candidates: none`, add the right
plug-in path key with `--ini KEY=VALUE`, or pass `--env-path` to also export
`CLAP_PATH`/`LV2_PATH`. `--env-path` is off by default because the variable
reaches any REAPER process started from that environment.

VST3 and AUv2 runs use the same driver: `--format vst3 --bundle <path>.vst3`
(the startup VST scan then runs, including the system VST3 folders), and
`--format au` for an AU component already installed in a system Components
folder, since REAPER scans AUs only there. VST3 has run on Linux and Windows.
The AU Delay has run on macOS arm64 and under Rosetta 2
([run](WORK_STATUS.md#release-010-reaper-au-timing)). Do not use a
`sandbox-exec` wrapper for AU runs, because REAPER then sees none of the AU's
parameters.

```sh
# Linux VST3
python3 scripts/bundle-vst3.py --plugin delay --sdk /path/to/vst3sdk --profile release
python3 tools/reaper-validation/delay.py --format vst3 \
    --bundle target/vst3/generic/delay/release/VST3/Release/ravet_delay_vst3.vst3 --reaper /usr/bin/reaper
```

The ReaScript also logs REAPER's audio system (`Audio system JACK`,
`Audio system WaveOut`); the checks need one that is running.

### Windows

On Windows the driver writes `clap_path_win64`, `lv2path_win64` and
`vstpath64` (the keys REAPER 7.82 x64 names in `reaper.exe` and kept in the
disposable ini after a run) and `[audioconfig] mode=2` (WaveOut). Without an
audio setting, a fresh configuration stops at "You have not yet selected an
audio device". With no audio system running (`mode=5` on the tested VM), CLAP
parameter edits never reached the plug-in, because REAPER flushes them from
the audio thread. `verchk=0` (written on every platform) turns off the online
version check, whose "New Version Notification" dialog stopped three Windows
runs mid-script.

Windows bundles are cross-built on Linux with `--target windows`
([decision 0285](decisions/0285-linux-windows-plugin-builds.md)):

```sh
python3 scripts/bundle-clap.py --plugin delay --profile release --target windows
python3 scripts/bundle-vst3.py --plugin delay --sdk /path/to/vst3sdk --profile release --target windows
python3 scripts/bundle-lv2.py --plugin delay --sdk /path/to/lv2-1.18.10 --target windows
# → target/clap/windows/release/RAVET Delay.clap
#   target/vst3/generic/delay/windows-release/VST3/Release/ravet_delay_vst3.vst3
#   target/lv2/windows/ravet_delay.lv2
```

With Python on the Windows machine, the driver command is the same as
elsewhere (`py -3 tools\reaper-validation\delay.py --format clap --bundle
"…\RAVET Delay.clap" --reaper C:\REAPER\reaper.exe`). That form has not been
run: the tested Windows 11 VM has no Python. For such a machine, prepare on
another computer, run the generated PowerShell script on Windows, and check
on the first computer:

```sh
# 1. Prepare (Linux/macOS). Paths in the ini and script name the Windows copy.
python3 tools/reaper-validation/delay.py --format vst3 \
    --bundle target/vst3/generic/delay/windows-release/VST3/Release/ravet_delay_vst3.vst3 \
    --windows-root 'C:\Users\me\ravet-reaper' --work target/reaper-delay/win-vst3-1
# 2. Copy the work directory over (here with OpenSSH).
scp -r target/reaper-delay/win-vst3-1 me@vm:C:/Users/me/ravet-reaper/
# 3. Run it in the logged-on user's desktop session as a scheduled task, poll
#    for run-windows.done, then delete the task.
ssh me@vm "schtasks /create /tn ravet-reaper /tr 'powershell -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File C:\Users\me\ravet-reaper\win-vst3-1\run-windows.ps1' /sc once /st 23:59 /it /f; schtasks /run /tn ravet-reaper"
ssh me@vm "Test-Path C:\Users\me\ravet-reaper\win-vst3-1\run-windows.done"
ssh me@vm "schtasks /delete /tn ravet-reaper /f"
# 4. Copy back and check renders, host log and guard snapshots.
scp -r me@vm:C:/Users/me/ravet-reaper/win-vst3-1 target/reaper-delay/win-vst3-1.back
python3 tools/reaper-validation/delay.py --check-only target/reaper-delay/win-vst3-1.back
```

`--windows-root` only prepares. `--reaper` then names the Windows executable
(default `C:\REAPER\reaper.exe`), and `--timeout`/`--quit-grace` are written
into `run-windows.ps1`. That script records the user, session and REAPER
version in `run-windows.log`; writes `guard-before.tsv` and `guard-after.tsv`
(size, modification time and SHA-256 of each top-level file in
`%APPDATA%\REAPER` and in REAPER's program directory, where a portable install
keeps its configuration); launches the driver's command; stops at the same
`ABORT`, `DONE` + quit grace and timeout conditions; and kills only the REAPER
process it started (`taskkill /T`). `--check-only` runs the render checks and
fails if any guarded file changed.

The run must be in the interactive desktop session. Started directly from a
Windows OpenSSH login (session 0, not interactive), REAPER wrote no host log
in 120 s. A task created with `/it` by the logged-on user runs in that user's
console session (session 1, interactive) and works. On the tested VM REAPER
takes about 20 s to reach the script; runs took 28–36 s. If a run times out,
look at the screen (on a QEMU VM, the monitor's `screendump` command) and set
the blocking dialog's preference with `--ini` (`[SECTION]KEY=VALUE` writes
outside `[REAPER]`).

Executed results are recorded in [work status](WORK_STATUS.md). A pass covers
the tested REAPER version, OS, format and offline render path only: no live
playback, device, deadline, editor or automation claim.

## Generic instrument MIDI, CC and PDC

`tools/reaper-validation/instrument.py` reuses the disposable Delay runner for
RAVET Evidence Synth. It feeds a generated MIDI item, checks host latency
readback (32→0→32), renders known clicks with and without CC response, and
routes MIDI to a second instrument with the first instance's audio disconnected.
The checker validates every sample, including exact onsets and finite audio
tail. See [fixture commands and limits](synth-plugin.md#reaper-fixture) and the
[5 November 2026 receipt](WORK_STATUS.md#release-010-instrument-evidence).
Linux REAPER 7.82 CLAP/VST3/LV2 runs pass. macOS startup attempts did not reach
the script; those failures supply no instrument host evidence. VST3 on macOS
was skipped as requested because of its known third-party startup scan stall.
Format tests independently check MIDI output bytes/offsets; REAPER's audio
render alone cannot exclude host MIDI-through. No physical MIDI device,
editor or live callback deadline is qualified by this fixture.

## macOS Rosetta 2

`delay.py --arch x86_64` launches the selected REAPER executable with
`arch -x86_64` and writes the x86_64 CLAP/VST scan keys even when Python is
arm64. `--arch arm64` is also available. Both require macOS. The normal-resource
snapshot comparison and ReaScript foreign-resource abort remain enabled.
REAPER.app must contain the requested slice (`lipo -archs` its executable).

Build a universal product first, then run CLAP or LV2, for example:

```sh
python3 scripts/bundle-plugin.py --package ravet-delay-plugin \
  --formats clap,lv2 --arch universal --release
python3 tools/reaper-validation/delay.py --format clap --arch x86_64 \
  --bundle target/plugins/macos-universal/release/ravet-delay-plugin.clap \
  --reaper /Applications/REAPER.app \
  --ini 'coreaudiooutdevnew=Mac mini Speakers' \
  --ini 'coreaudioindevnew=Mac mini Speakers' \
  --ini coreaudiosrate=48000 --ini coreaudiosrateuse=1 \
  --ini coreaudiobs=512 --ini coreaudiobsuse=1
```

The named built-in device resolved an audio-initialization modal in this
recorded environment. Choose an available device for another machine; these
settings belong only to the disposable configuration. The harness removes the
master's hardware sends before rendering, so this is offline host evidence.

A Rosetta VST3 run initially stalled scanning system plug-ins. The successful
run used an executable shell wrapper passed as `--reaper`, restricting those
search paths for the private REAPER process with the built-in `sandbox-exec`:

```sh
#!/bin/sh
exec /usr/bin/sandbox-exec -p "(version 1)(allow default)(deny file-read* (subpath \"/Library/Audio/Plug-Ins/VST3\") (subpath \"$HOME/Library/Audio/Plug-Ins/VST3\"))" \
  /absolute/path/to/target/reaper-delay/app/REAPER.app/Contents/MacOS/REAPER "$@"
```

Point the wrapper at the **private copy** created by the earlier `.app` run;
use `--format vst3 --arch x86_64 --bundle <new-bundle>` and the same device
settings. Its private VST3 cache must name the selected bundle. This restriction
changes no installed plug-ins or system settings. Only CLAP/VST3/LV2 passed
all strict renders in the [5 November receipt](WORK_STATUS.md#release-010-macos-architectures).

An additional AU run used an executable shell wrapper with RAVET's process-local
registration helper, avoiding changes to the installed same-identity component:

```sh
#!/bin/sh
exec /usr/bin/env \
  RAVET_AUVAL_BUNDLE=/absolute/path/to/new.component \
  DYLD_INSERT_LIBRARIES=/absolute/path/to/build/libravet_auval_registration.dylib \
  /absolute/path/to/target/reaper-delay/app/REAPER.app/Contents/MacOS/REAPER "$@"
```

The helper is built by the AU builder with `--validate`; its location is under
`target/plugin-build/macos-<arch>/au-native/<crate_stem>/<profile>/`.
Run `--format au --arch x86_64 --bundle <new.component> --reaper <wrapper>`.
`--bundle` is recorded for AU; the wrapper actually registers its exact factory.
The default AU discovery prefix now matches REAPER 7.82's `AU: RAVET Delay`.
Discovery and 63 host edit/recall checks passed, but all three strict render
cases failed timing: fixed/recalled echoes were 2,048 frames late, synced echoes
3,072 late. That run is **not** an AU render pass. The cause was REAPER's
transport sample positions carrying Float64 residue, which the adapter rejected
block by block ([decision 0294](decisions/0294-au-transport-sample-residue.md)).
After the fix, a universal bundle installed in the user Components folder
passed all three strict renders under `arch -x86_64` and on arm64
([run](WORK_STATUS.md#release-010-reaper-au-timing)). All runs left the guarded
normal REAPER resource files unchanged. This is **Rosetta 2 on Apple silicon**,
not native Intel hardware, live-device, editor, or real-time qualification.

## Scripted RAVET editor check (REAPER, Linux/Windows)

`tools/reaper-validation/editor.py` reuses the Delay harness's disposable
configuration and runs `editor.lua`: insert the plug-in, open its editor in a
floating FX window, set Feedback from the host, log every parameter change,
close and reopen the editor, remove the plug-in with its editor open, save,
reopen the saved project (so quitting raises no prompt) and quit. It prints
`RESULT PASS` when the script reached `DONE editor check` without a failure and
the normal REAPER directory is unchanged.

On Linux, `--probe` adds `x11-editor-probe` (X11/XWayland, XTest), run only on
the REAPER window it started:

```sh
cc -O2 -o target/x11-editor-probe tools/reaper-validation/x11-editor-probe.c -lX11 -lXtst
python3 tools/reaper-validation/editor.py --format vst3 \
    --bundle target/plugins/release/ravet-delay-plugin.vst3 \
    --probe target/x11-editor-probe --seconds 20 \
    --action 214,293 --action drag:478,128,-40
```

The probe prints the window chain from the editor's plug to the floating
window, with map state and whether each window carries `_XEMBED_INFO` (the
plug does; an XEmbed socket parent would not be a plain host window), saves
PPM/PNG captures of that window only, and injects the clicks and drags, whose
edits appear as `CHANGE` lines in `output/delay-host.log`. Other products need
`--fx-name`, `--title` and `--size WIDTHxHEIGHT`. On Windows, `--windows-root`
prepares a work directory and `run-windows.ps1` exactly as for the Delay
check; the script holds the editor open for `--seconds` so it can be captured
from outside (for a QEMU VM, the monitor's `screendump`). Editor input is not
injected on Windows.
