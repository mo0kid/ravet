# Support matrix

**Evidence as of: 5 November 2026.** This hand-maintained page summarizes
recorded runs of the generic export layer's **RAVET Delay** example on
`release/0.1.0` through `628af5d`, plus the local validator rerun in the docs/CI
work package. Evidence Synth and the macOS architecture work (based on `eb4d2f5`) are recorded separately below. It is not a certification of every product,
host version, device or deadline. Format adapters remain prototypes. Earlier
0.0.1 reference checks remain in [work status](WORK_STATUS.md) and
[host validation](host-validation.md); they do not qualify the new adapters.

Only named host runs appear in the first table. A validator or adapter test
does not establish host support. “Not recorded” means no such evidence is
recorded here, rather than a failed test. Host point versions not recorded in
the source entry are explicitly left unspecified.

## Format × platform: named host runs

Every run below is dated **5 November 2026**. Each evidence link names its
dated WORK_STATUS entry.

| Format | macOS arm64 | Windows 11 x64 VM | Arch Linux x86_64 |
|---|---|---|---|
| CLAP | REAPER 7.82/macOS-arm64 on macOS 27.0.1: fixed/synced renders, edits/readback and project recall ([run](WORK_STATUS.md#release-010-reaper-delay), [merged-harness reruns](WORK_STATUS.md#release-010-host-integration)) | REAPER 7.82/x64, WaveOut: same harness, three passing runs ([run](WORK_STATUS.md#release-010-reaper-win-linux)) | REAPER 7.82/linux-x86_64, JACK: same harness ([run](WORK_STATUS.md#release-010-reaper-delay), [rerun](WORK_STATUS.md#release-010-reaper-win-linux)) |
| VST3 | Ableton Live 12 Suite on macOS 27 arm64 (point version not recorded): manual fixed/synced exports match the strict sample checker ([run](WORK_STATUS.md#release-010-live-delay)) | REAPER 7.82/x64, WaveOut: edits/readback, project recall and fixed/synced renders, three passing runs ([run](WORK_STATUS.md#release-010-reaper-win-linux)) | REAPER 7.82/linux-x86_64, JACK: same harness, three passing runs ([run](WORK_STATUS.md#release-010-reaper-win-linux)) |
| LV2 | REAPER 7.82/macOS-arm64 on macOS 27.0.1: edits/readback, project recall and fixed/synced renders ([run](WORK_STATUS.md#release-010-reaper-delay)) | REAPER 7.82/x64, WaveOut: same harness, three passing runs ([run](WORK_STATUS.md#release-010-reaper-win-linux)) | REAPER 7.82/linux-x86_64, JACK: same harness ([run](WORK_STATUS.md#release-010-reaper-delay), [rerun](WORK_STATUS.md#release-010-reaper-win-linux)) |
| AUv2 | Logic Pro on macOS 27 arm64 (Logic version not recorded): manual fixed/synced bounces, typed values, editor reopening and saved-project recall ([run](WORK_STATUS.md#release-010-logic-delay)) | Not applicable | Not applicable |
| AAX | Pro Tools 26.4, macOS arm64 (OS version not recorded in this entry): manual fixed/synced bounces, mouse editor use; keyboard typing fails ([run](WORK_STATUS.md#release-010-pro-tools-delay)) | **Provisional**, owner decision on 5 November 2026: Windows bundle built and adapter-checked; **not run in Pro Tools for Windows** ([build/checks](WORK_STATUS.md#release-010-platform-builds), [decision recorded in release plan](releases/0.1.0.md#5-platform-and-host-evidence)) | Not applicable |

The REAPER harness checks 48 kHz offline master-mix renders, 63 host checks per
run, two project recalls and exact echo positions/mix/silence. It does not
exercise live playback, custom editors or automation. The VM's WaveOut and
Linux JACK settings identify the environment; they do not qualify a device.
The macOS merged-harness CLAP rerun also records one initial startup timeout.

The manual Live exports match the strict checker after unwarping the clip and
typing exact values; editor lifecycle and session recall were not recorded.
Logic's bounces include the metronome, so checks used exact delay sample
positions and amplitudes rather than a whole-file checker pass. In Pro Tools,
echo timing and sync were measured, but the strict checker fails amplitudes
because the dragged/reset values differ from exact targets; the fixed default
also has a small one-sample split from normalized parameter precision. The
release plan's previous blanket “sample-exactly” wording must not be read as a
strict checker pass for these manual runs.

## macOS x86_64 under Rosetta 2

**5 November 2026:** Apple M4 Pro, macOS 27.0.1 (26A434), REAPER 7.82/OSX64
launched with `arch -x86_64`. Thin x86_64 and universal Delay bundles built in
all five formats; every shipped image's slices were checked. These are
**translated x86_64 runs on Apple silicon**, not native Intel hardware evidence.
The [architecture receipt](WORK_STATUS.md#release-010-macos-architectures)
records exact builds, wrappers, logs, SDKs and limitations.

| Format | x86_64 validator / adapter result | REAPER 7.82/OSX64 result |
|---|---|---|
| CLAP | Pinned clap-validator 0.4.1 (`152b982`): 33 passed, 0 failed/warnings, 11 skipped | Universal bundle: 63 host checks and all three strict 48 kHz renders passed |
| VST3 | SDK 3.8.1 validator: 47 passed, 0 failed | Universal bundle: 63 host checks and all three renders passed; per-process sandbox excluded system VST3 directories after an initial startup-scan timeout |
| AUv2 | Exact signed bundle's contracts and private process-local `auvaltool` 1.10.0 passed, preserving original identity | Thin x86_64 component registered process-locally: discovery and 63 host checks passed; **strict renders failed**, fixed/recalled echoes 2,048 frames late and synced echoes 3,072 late |
| AAX | AAX SDK 2.8.0; RAVET bundle checker passes mono/stereo, parameters and state | Not applicable to REAPER; **Pro Tools x86_64 and Avid validator not run** |
| LV2 | Actual-module smoke 46 checks, 0 failures; CTest 2/2; `lv2_validate` 0 errors | Universal bundle: 63 host checks and all three strict renders passed |

All successful host runs preserved the normal REAPER resource directory.
The disposable configs selected Mac mini Speakers at 48 kHz/512 frames to
avoid an audio-init modal; hardware sends were removed. No live-device claim
follows. Optional lilv was disabled for universal/non-host builds because the
installed Homebrew library is arm64-only; **x86_64 lilv and lv2lint not run**.
Universal validators also ran their arm64 slices. arm64 AU uses system
registration, which can select an installed same-identity product; its exact
artifact is independently covered by in-process contracts. The x86_64 AU
validator uses explicit process-local registration of the new component.

### Editor runs on Windows and Linux

Separate from the render harness above: `tools/reaper-validation/editor.py`
opens the Delay's web editor in a floating REAPER 7.82 FX window, sets a
parameter from the host, closes and reopens the editor and removes the plug-in
with its editor open, in a disposable configuration
([run](WORK_STATUS.md#release-010-editors-win-linux)).

| Format | Windows 11 x64 VM (WebView2 156) | Arch Linux x86_64 (XWayland) |
|---|---|---|
| CLAP | Renders; host edit shown; close/reopen/remove; quits cleanly. Editor input not exercised | Renders inside REAPER's plain (non-XEmbed) child window; host edit shown; XTest click and drag reach the host; close/reopen/remove |
| VST3 | Same as CLAP | Same as CLAP |
| LV2 | Same as CLAP (`WindowsUI`) | Same as CLAP (`X11UI`) |
| AAX | Synthetic AAX host only (`ravet_aax_check --editor`, real HWND); **no Pro Tools** | Not applicable |

The Gain example's asset-served page also rendered in REAPER's CLAP host on
both platforms. These are scripted observations in one host per platform: no
keyboard input, accessibility, other DAW, display scaling above 100 % or
real-time claim.

## Validators and adapter checks (separate from hosts)

All entries below were recorded on **5 November 2026**. These are results from
the cited receipts. The macOS 0.4.1 row is a new local rerun for this docs/CI
work package; other rows summarize earlier receipts.

| Format | Platform and tool | Recorded result and evidence |
|---|---|---|
| CLAP | macOS arm64, clap-validator 0.3.2-127 | Delay: 33 passed, 0 failed, 11 skipped; skips include f64, preset discovery, port configurations and note tests ([run](WORK_STATUS.md#release-010-delay-example), [policy regression](WORK_STATUS.md#release-010-native-table)) |
| CLAP | macOS 27.0.1 arm64, pinned official clap-validator 0.4.1 (`152b982`) | Locally rebuilt Delay: 33 passed, 0 failed, 0 warnings, 11 skipped; not a hosted CI run ([run](WORK_STATUS.md#release-010-docs-ci)) |
| CLAP | Linux x86_64, clap-validator 0.4.1 (`152b982`) | 33 passed, 0 failed, 11 skipped, 0 warnings; repeated seven times ([run](WORK_STATUS.md#release-010-platform-builds)) |
| CLAP | Windows 11 x64 VM, clap-validator 0.4.1 (`152b982`, source-built with static CRT) | 32 passed, 0 failed, 12 skipped; denormal timing warning in 3 of 7 runs; the additional skip is Linux-only `scan-rtld-now` ([run](WORK_STATUS.md#release-010-platform-builds)) |
| VST3 | macOS arm64, Steinberg SDK 3.8.1 validator | Delay, Gain and mock: 47 passed, 0 failed each; mock CTest 1/1 ([run](WORK_STATUS.md#release-010-vst3-products), [merged Delay rerun](WORK_STATUS.md#release-010-adapter-integration)) |
| VST3 | Linux x86_64 and Windows 11 x64 VM, SDK 3.8.1 validator | Delay and mock: 47 passed, 0 failed on each OS; Windows mock contracts also run under Wine ([run](WORK_STATUS.md#release-010-platform-builds)) |
| AUv2 | macOS 27.0.1 arm64, AU Validation Tool 1.10.0 and RAVET component checks; AudioUnitSDK 1.4.0 | Effect/instrument/MIDI-effect mocks pass `auval`; merged Delay passes `auval -v aufx QFKS TYDS` and in-process state/render/automation/retained-view checks ([mock run](WORK_STATUS.md#release-010-auv2-adapter), [Delay run](WORK_STATUS.md#release-010-adapter-integration)) |
| LV2 | macOS arm64, LV2 1.18.10, lilv 0.28.0, sord 0.16.22 | Delay/Gain/mock adapter, lilv and render checks pass; `lv2_validate` reports 0 errors; real Cocoa editor smoke included ([run](WORK_STATUS.md#release-010-lv2-adapter)) |
| LV2 | Linux x86_64; Windows 11 x64 VM, LV2 1.18.10 | Linux CTest 4/4; Windows mock 100 and Delay smoke 32 checks with 0 failures; Windows CTest 2/2 under Wine. `lv2_validate` reports 0 Turtle errors. No Windows lilv; **lv2lint not run** ([run](WORK_STATUS.md#release-010-platform-builds)) |
| AAX | macOS arm64, AAX SDK 2.8.0 and RAVET checks | Mock CTest 2/2; Delay mono/stereo rendering, parameters, state and off-screen editor check pass. **Avid validator not run** ([run](WORK_STATUS.md#release-010-aax-adapter)) |
| AAX | Windows 11 x64 VM, AAX SDK 2.8 and RAVET checks | Delay mono/stereo, eight parameters, rendering and state corruption checks pass; mock passes; unsigned. **Avid validator not run** ([run](WORK_STATUS.md#release-010-platform-builds)) |

Windows bundles in these receipts were cross-built on Linux using cargo-xwin
and a static CRT. Native MSVC builds are not established by them. Hosted CI is
[configured](runtime-validation.md) separately and has not run for this change.

<a id="instrument-evidence"></a>

## Evidence Synth — 5 November 2026

The [deterministic instrument](synth-plugin.md) is a separate product from Delay.
The [receipt](WORK_STATUS.md#release-010-instrument-evidence) records exact build
commands, failed attempts and scope. Every passing REAPER case checks stereo
48 kHz samples throughout four renders: CC response, 32/0 latency readback and
PDC-aligned onsets, finite click decay and a second-instance MIDI route.

| Format | Linux x86_64 REAPER 7.82 | macOS 27.0.1 arm64 instrument evidence |
|---|---|---|
| CLAP | Four renders PASS; normal config unchanged | clap-validator 0.4.1: 34 pass, 0 fail/warnings, 10 skips; actual CLAP ABI sample/MIDI tests pass. REAPER startup timed out before script; no host render evidence. |
| VST3 | Four renders PASS, including controller-derived CC level and note output route; normal config unchanged | SDK 3.8.1 validator: 47 pass, 0 fail; real-product adapter-class MIDI/audio/timing CTest 1/1. REAPER skipped because of known third-party VST3 scan stall. |
| LV2 | Four renders PASS; normal config unchanged | CTest 4/4 including actual instrument Atom MIDI/audio/latency, lilv; `lv2_validate` 0 errors. REAPER startup timed out before script; no host render evidence. |
| AUv2 | Not applicable | Actual component: three note/CC, MIDI-output-byte/offset, audio/tail and latency-notification cycles PASS; `auval` 1.10.0 PASS. No Logic instrument run. |
| AAX | Not applicable | Actual unsigned bundle synthetic checker PASS: two MIDI nodes, CC audio, note output and latency notification. Finite tail verified in audio; SDK 2.8 has no native tail report. No Avid validator or Pro Tools instrument run. |

No Windows instrument run is recorded. REAPER's second-instance audio render
cannot by itself exclude host MIDI-through; actual CLAP/AU/AAX/LV2 fixtures
check generated bytes/offsets; a VST3 real-product adapter-class fixture checks controller/note conversion, output offsets and preserved IDs.
The processor declares a finite 96-sample default tail (64 with lookahead off).
CLAP, VST3 and AU expose timing to hosts; LV2 exposes latency only and AAX has
no tail report here. These checks do not qualify a physical MIDI/audio device,
custom editor, polyphony or live deadline.

## Known limits

- **Windows and Linux editors** are prototypes with REAPER-only evidence (see
  [editor runs](#editor-runs-on-windows-and-linux)). Keyboard input is not
  validated there; Windows editors do not negotiate display scaling; Linux
  needs X11/XWayland and refuses GTK 2/GTK 4 and Wayland GTK hosts.
- **Pro Tools keyboard typing failed** in the macOS web editor in the recorded
  run; mouse editing works. A fix (the web view takes key status while a text
  field has focus, [decision 0286](decisions/0286-editor-keyboard-focus.md))
  passes AppKit fixture tests but has not yet been confirmed in Pro Tools.
- Generic processing is **f32 and main bus only**. CLAP and LV2 use the first
  declared layout. This page does not qualify the separate f64/auxiliary
  reference adapters or AUv3/mobile integrations.
- Generic CLAP and the native adapters carry **bounded MIDI 1 short messages**
  in both directions. Generic CLAP advertises MIDI-dialect ports; native CLAP
  note expressions/wildcards and SysEx/UMP are outside this export scope.
  At most 1,024 processor input events fit a block; excess cached changes can
  wait until the next block. No physical MIDI device is qualified here.
- Product extra state is not saved by the generic exports. State and editor
  messages are bounded; the editor protocol accepts at most 64 KiB and its
  edit queue holds 256 edits. A concurrent automation reflection can retain
  an automated value over a concurrent state load.
- The default native table uses prepared audio scratch. Frozen bypass avoids
  dry scratch copies, switches on block boundaries and discards notes while
  bypassed; framework dry bypass is not latency-compensated.
- Delay accepts sample rates up to 768 kHz, allocates two two-second delay
  lines at prepare, reads tempo once per block, clamps synced time to two
  seconds and has no editor meters.
- Native Intel macOS hardware, standalone audio/MIDI on all three platforms
  with named devices, production signing for arbitrary vendors and real-time
  deadlines remain unestablished. The Rosetta AU render failure above is open. AAX vendor signing is separate from adapter checks.
  AAX applies automation once per block and reports no tail through SDK 2.8.
  VST2 is excluded.

The [feature registry](features.toml) and generated [catalogue](FEATURES.md)
contain the exact per-adapter limits; see also [plug-in exports](plugin-export.md),
[VST3](vst3.md), [AUv2](audio-unit.md), [AAX](aax.md) and [LV2](lv2.md).

## Maintaining this page

Keep this page hand-written: the WORK_STATUS receipts include qualifications,
partial manual checks and missing versions that cannot be inferred from a
feature's implemented/prototype status. When adding evidence, retain the date,
product, architecture, host/version, checked scope and a stable receipt link.
Keep validators separate and change the evidence-as-of date only after review.
The documentation checker validates local file links; semantic claims and
anchors require review under [maintenance](maintenance.md).
