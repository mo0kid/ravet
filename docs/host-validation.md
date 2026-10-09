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
