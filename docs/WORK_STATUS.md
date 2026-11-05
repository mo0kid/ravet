# RAVET work status

Updated: 13 October 2026. Owner: current implementation session.
Starting point: four documentation-only crates; no Git repository.
Local Git has now been initialized and an initial commit pushed to a private
GitHub remote. See [publication policy](publication.md): the initial private
history captured a consumer scaffold and must be sanitized before going public.

<a id="release-010-device-crate-review"></a>

## Device crate panic and unsafe review — 5 November 2026

Release-plan §6 review of the device crates used by standalone applications,
on branch `review/device-crates` from `release/0.1.0` (`cf913d4`). Decision
[0295](decisions/0295-device-crate-robustness.md) records the policy and the
full findings table (renumber at merge if 0295 is taken). This completes the
§6 panic/unsafe item together with decisions 0287 and 0292.

| Crate | Non-test panic sites | Made fallible | Documented (a / c) | Unsafe sites |
|---|---|---|---|---|
| `ravet-device` | 88 | 57 (callback and worker paths) | 29 / 2 (control and retirement worker) | 4 CoreAudio blocks, 2 `unsafe impl Sync`, 3 guard blocks, 2 extern blocks |
| `ravet-midi-device` | 1 (+3 indexing/arithmetic) | 1 | 3 / 0 | none |
| `ravet-core-midi` | 0 | — | — | 12 blocks, 1 extern block, 9 native entry points made `noexcept` |

No reachable panic was found. Five sites sat outside any unwind containment, so a
violated invariant would have aborted (CoreAudio/CoreMIDI `extern "C"`
callbacks) or ended the CPAL callback thread: the reclaimable output, clocked
input and clocked output guards, plain input capture and UMP batch delivery. They now silence or drop
input. The clocked converter can no longer spin on a stalled step. All three
crates deny `clippy::undocumented_unsafe_blocks`; one CoreMIDI block lacked a
comment in the position the lint requires.

New regression tests break ownership invariants directly and check for silence
or underflow, counted faults and no allocation:
`callback_tests::missing_output_owner_faults_with_silence_instead_of_panicking`,
`input::tests::missing_duplex_owners_fault_or_underflow_instead_of_panicking`,
`graph_events::tests::corrupt_private_framing_reports_no_batch_instead_of_panicking`,
`reclaimable::tests::data_guard_without_renderer_silences_instead_of_panicking`,
`reclaimable_clocked::tests::callback_guards_without_owners_do_not_panic`.
No test opens a real audio or MIDI device, and default devices were not changed.

Checks:
- macOS 27 arm64: `cargo test` for the three crates with all features
  (`ravet-device` 51 unit + 73 integration, `ravet-midi-device` 7,
  `ravet-core-midi` 1); Clippy with warnings denied (all targets, all
  features) for `aarch64-apple-darwin` and `x86_64-apple-darwin`; `cargo fmt
  --check`; rustdoc with `-D warnings`; `cargo +1.88.0 check` (all targets,
  all features).
- Arch Linux x86_64 (omarchy): the same tests (`ravet-device` 50 unit + 73
  integration, `ravet-midi-device` 7) and Clippy with warnings denied pass.
- Windows x64: `cargo xwin clippy --target x86_64-pc-windows-msvc` with
  warnings denied passes with `ravet-device/resampled-playback`. The
  `vorbis-recording`/`mp3-recording` C encoders in `ravet-audio-file` fail to
  cross-compile under cargo-xwin (aoTuV `_mm_prefetch` pointer types, LAME
  `configure`); those features add no code to these crates. Compile evidence
  only.

<a id="release-010-webview-review"></a>

## Web view, standalone window and notice review — 5 November 2026

Release-plan §6 review of the surface decision 0287 left out, on branch
`review/webview` from `release/0.1.0` (`eb4d2f5`). Decision
[0292](decisions/0292-webview-robustness-and-system-notices.md) records the
policy (renumber at merge if 0292 is taken).

Rust sites reviewed in `ravet-webview` (non-test, all features):

| Module | Panic sites | Unsafe blocks | Fixed | Classified |
|---|---|---|---|---|
| `native.rs` | 3 `expect` | 16 | 3 `expect` become `InvalidInput`; null context/message/destination pointers refused in the three callbacks | 4 SAFETY comments added |
| `editor_abi.rs` | 0 | 9 | misaligned handles rejected by every operation and by destroy | 7 SAFETY comments added |
| `standalone.rs` | 0 | 14 | `embed` error path left a dangling editor pointer in the Windows/Linux window (latent: no backend closes during attach) | 1 SAFETY comment added |
| `fonts/macos.rs` (`system-fonts`) | 4 `unwrap` | 9 | — | (a) fixed-length, bounds-checked slice-to-array conversions; comment added |
| `build.rs` | 6 | 0 | — | (c) build-time failure with a message (missing SDK or tool) |

`#![deny(clippy::undocumented_unsafe_blocks)]` now covers `ravet-webview`.

Native code (Objective-C++ and C++), three fixed host risks:
- A page posting a message body JSON cannot encode (`Date`, `NaN`) terminated
  the process with `NSInvalidArgumentException` in both the editor bridge
  (`RavetWebView.mm`) and the standalone settings handler
  (`StandaloneWindow.mm`). Both now drop such bodies.
- Exceptions could unwind from the allocating C entry points, WebView2 COM
  handlers, Win32 window procedures and GLib callbacks into C, COM or Rust
  frames. These are now `noexcept` (deterministic termination on allocation
  failure). Eight `NSCAssert` at the Cocoa C ABI, which raise into Rust, became
  wrong-thread refusals.
- WebView2: a handler could be released while its `Invoke` ran (callback
  destroying the editor), and the loader DLL was freed when the last state
  reference dropped, possibly inside a WebView2 callback. Handlers now hold a
  reference through `Invoke`; the loader is no longer freed. The Windows and
  Linux UI-thread ids are atomic. The Cocoa bridge now stays alive through a
  script-message callback that destroys its editor.

Classified (c), unchanged: CoUninitialize in the last state release may run
inside a callback (balanced per view; hosts keep their own COM
initialization); the Windows/Linux UI thread is adopted by the first creation
(documented); Objective-C exceptions from AppKit/WebKit calls other than JSON
encoding are not caught (none are expected for the validated arguments).

New tests, each failing before its fix:
- `crates/ravet-surface/native/tests/lifecycle.mm`: a posted `Date` body is
  dropped and the next message still arrives. Without the fix the probe
  terminated with `NSInvalidArgumentException`.
- `crates/ravet-webview/native/tests/window-lifecycle.rs`: the same for the
  standalone `ravetSettings` handler; without the fix the fixture terminated.
- `native.rs`: `null_native_arguments_are_refused_without_dereference`.
- `editor_abi.rs`: `misaligned_handles_are_refused_before_any_dereference`.

Pre-existing breakage fixed: `scripts/check-window-lifecycle.py`,
`check-window-constraints.py` and the Rust unit step of
`check-native-semantics.py` failed to compile (`unresolved import
crate::accessibility`), because their fixtures still included `src/macos.rs`,
renamed to `src/native.rs`. The fixtures now include `native.rs` and
`accessibility.rs`.

Notices (decision 0292, [dependency notices](dependency-notices.md)):
`ravet-webview` links no third-party crate; the standalone device crates add
23 (macOS), 27 (Windows, 6 of them build-time procedural macros and their
dependencies) and 11 (Linux, plus build-only `pkg-config`) crates, all
permissive. Ten publish no licence file; the generator
now writes a reconstructed MIT notice for those with an MIT option and prints
a reminder. It also writes the WebView2 SDK licence and notice (Windows) and
`SYSTEM-LIBRARIES-LGPL.txt` (Linux: GTK 3, WebKitGTK 4.1, JSON-GLib, GLib,
ALSA) when those components are linked. The WebView2 SDK
(`Microsoft.Web.WebView2` 1.0.4314.35) licence is BSD-3-Clause-style. The
static MSVC CRT is covered by Microsoft's terms accepted through cargo-xwin.
Release plan §6: the notice-audit item is ticked; the panic/unsafe item stays
open for `ravet-device`, `ravet-midi-device` and `ravet-core-midi` (about 85
non-test panic-class sites and 30 unsafe sites, not reviewed).

Checks (macOS 27 arm64 unless stated):
- `cargo test -p ravet-webview` (default 19, all features 26 tests, doctests
  1), Clippy with warnings denied (all targets, default and all features),
  `cargo fmt --check`, rustdoc with `-D warnings`, `cargo +1.88.0 check`
  (all targets, all features) pass; `cargo test -p ravet-plugin --features
  clap,native,editor` passes.
- Native web-view CMake/CTest: 37 of 39 pass. `ravet-web-first-mouse` and
  `ravet-web-keyboard-focus` fail identically on unmodified `release/0.1.0` in
  this session, which cannot take key-window status; no focus result is claimed.
- `check-native-semantics.py` (24 groups), `check-window-lifecycle.py` and
  `check-window-constraints.py` pass.
- 125 script tests pass.
- `bundle-plugin.py --package ravet-delay-plugin --formats clap,vst3,au,aax,lv2
  --release --validate` with `RAVET_*_SDK` variables: clap-validator 33 passed,
  0 failed, 11 skipped; Steinberg validator 47 passed, 0 failed; `auval`
  succeeds; AAX bundle check 1/1; LV2 4/4 and `lv2_validate` 0 errors; the four
  signed macOS bundles pass `codesign --verify --strict`.
- Linux x86_64 (Omarchy, over SSH): `ravet-webview` Clippy with warnings denied
  and 15 tests pass; native CTest 4/4 with `WEBKIT_DISABLE_DMABUF_RENDERER=1`
  (without it `ravet-web-paint` fails on GBM buffers, identically on unmodified
  `release/0.1.0` in that session).
- Windows x86_64: `cargo xwin clippy -p ravet-webview --all-targets --target
  x86_64-pc-windows-msvc -- -D warnings` passes against WebView2 SDK
  1.0.4314.35. Compile evidence only; no Windows run.

<a id="release-010-instrument-evidence"></a>

## Generic instrument MIDI/timing evidence and C denormal scope — 5 November 2026

Branch `midi/instrument-evidence`, based on `release/0.1.0` at `eb4d2f5`, in
an independent worktree. No push or main/release checkout mutation. Added the
RAVET-owned [Evidence Synth](synth-plugin.md), a generic stereo f32 instrument
with timed 64-sample bipolar one-shots, CC 1 response toggle, short-MIDI/note
echo, 32/0-sample lookahead and conservative 96/64-sample finite tail.
The musical subtractive synth tutorial was read and retained separately.

Generic CLAP now advertises the product's MIDI output port and enables the
existing bounded bridge output. Accepted MIDI prefixes are never replayed;
output rejection leaves successful audio valid. Activation publishes latency
after cached controls are synchronized; active parameter flushes also detect
latency changes. Existing VST3 IMidiMapping hidden CC/aftertouch/pitch-bend
parameters, note output, AU MIDI callbacks, AAX local nodes and LV2 Atom
mappings needed no adapter runtime change. Actual AU/AAX/LV2 product checks
were extended for this instrument, including editor-free validation. The AU
bundler retries only asynchronous component-discovery failures, not validator
failures, and removes its exclusive temporary installation even on failure.
The LV2 fixture now permits a product with no UI declaration.

The native product library exposes `ravet_plugin_with_denormal_mode_v1`:
synchronous callback, preserve/FlushBoth selection, no scope token or raw
register setter, exact prior control/sticky-status restoration, invalid/null
argument refusal and unsupported-environment refusal before callback dispatch.
Nested scope and restoration tests pass. Native table test DSP now observes
FlushBoth on every invocation. Entry-point review confirms CLAP process/active
flush use the wrapped bridge; VST3 process, AU render chunks, AAX callbacks and
LV2 run chunks use the same guarded native table. Host event conversion before
DSP is outside this scope. See [contract](plugin-export.md#denormal-guard) and
[decision 0291](decisions/0291-instrument-midi-and-c-denormal-scope.md).

### Executed local checks (macOS 27.0.1 arm64)

With external SDK paths provided through `RAVET_VST3_SDK`,
`RAVET_AUDIO_UNIT_SDK`, `RAVET_AAX_SDK`, `RAVET_LV2_SDK`:

```sh
python3 scripts/bundle-plugin.py --package ravet-synth-plugin \
  --formats clap,vst3,au,aax,lv2 --validate --clap-validator /path/to/clap-validator
```

All five final debug bundles pass this command. Separately built the VST3 mock
with `bundle-vst3.py --sdk <external SDK> --mock --profile debug`.

| Format | Actual result |
|---|---|
| CLAP | clap-validator 0.4.1: 34 pass, 0 fail, 0 warnings, 10 skips. `process-note-out-of-place-basic` and `process-note-inconsistent` run and pass. Native-CLAP wildcard notes remain skipped because the generic ports advertise MIDI only; other skips include f64, preset discovery and port configurations. |
| VST3 | Steinberg SDK validator: real instrument 47 pass/0 fail. Real-product adapter-class CTest 1/1 checks hidden CC mapping, note IDs/output offsets, exact audio/tail and latency notification. Independent mock CTest 1/1 and validator 47/0 retain CC/pitch/pressure regression coverage. The SDK validator separately loads the final module. |
| AUv2 | Actual MusicDevice component passes three note/CC/exact-audio/finite-tail/MIDI-byte-offset/latency-notification cycles. `auval` 1.10.0 passes `aumu/CJLN/TYDS`. Initial installation discovery failed, then a retry passed; it was not hidden as a validator pass. |
| AAX | Actual unsigned bundle synthetic CTest 1/1 plus final bundle check pass: 0x2 layout, two local MIDI nodes, CC-scaled exact audio, note bytes/offsets and host latency-change notification. Finite audio tail passes; AAX SDK 2.8 has no tail report here. Avid validator and Pro Tools instrument run not performed. |
| LV2 | CTest 4/4: mock contracts/lilv and actual instrument contracts/lilv; instrument checks exact CC audio, silence, Atom MIDI bytes/offsets and 32→0 latency port. LV2 1.18.10/lilv 0.28.0; `lv2_validate`: 0 errors among 84 files/1960 restrictions. No exported tail port. |

Package tests for `ravet-plugin` (CLAP/native features), `ravet-synth-plugin`
and `ravet-delay-plugin` pass. The instrument has six tests: CLAP MIDI/audio
and allocation counts, cached-control activation latency, native MIDI/CC/
latency/tail/reset/allocation checks, C scope nesting/restoration, block-boundary
independence and atomic output-capacity rejection. Delay regression tests pass.
Script tests: 123 pass, including selective AU discovery retries and cleanup.
REAPER Python tests: 17 pass, including exact-sample corruption/late-tail
rejection and disposable-directory setup. The lines saying FAIL in those
Python logs are expected negative-fixture output; both unittest suites exit 0.
Also passed workspace doctests (`cargo test --workspace --doc --locked`),
warning-denied workspace Rustdoc (`RUSTDOCFLAGS="-D warnings" cargo doc
--workspace --no-deps --locked`), targeted warning-denied Clippy for plugin/synth,
Cargo formatting and the documentation registry/link checker. The existing
native-DSP guard/processor integration tests also pass (six tests), including
exact sticky-status restoration and fault/unwind paths. Whole-workspace
non-doc tests/Clippy were not run for this work package.

### Executed Linux host checks

Synced this branch with `rsync` excluding `target` and `.git` into the new
`~/ravet-instrument-20261105` directory on `comfy-omarchy`, leaving the normal
checkout alone. Arch Linux x86_64 kernel 7.2.11; REAPER 7.82/linux-x86_64. Built
CLAP/VST3/LV2 with the unified bundler and `--validate`, using the supplied
external VST3/LV2 SDKs and installed clap-validator 0.4.1. Results: CLAP
34/0/10 skips/0 warnings, VST3 47/0, LV2 CTest 4/4 and Turtle 0 errors. Native
and CLAP Rust package tests also pass there, including the C floating-mode
callback on physical x86_64. A final real-product VST3 adapter-class CTest 1/1
also checks generated event offsets and note IDs directly, independently of
REAPER MIDI-through; the final module validator again passes 47/0.

Ran `instrument.py` separately for CLAP, VST3 and LV2 with
`XDG_RUNTIME_DIR=/run/user/$(id -u) WAYLAND_DISPLAY=wayland-1 DISPLAY=:0` and
`--reaper /usr/bin/reaper`, in fresh work folders. All three final runs PASS:

- `pdc`: reported latency 32, clicks begin exactly at samples 24,000 and 48,000.
- `zero`: latency changes to 0; onsets remain aligned at those exact samples.
- `cc_off`: latency 32, CC disabled; both first-sample amplitudes are 0.25.
- `echo`: first instance latency 0 and audio pins disconnected; second
  instrument receives MIDI and produces the expected 32-sample-compensated
  audio. Independent format fixtures establish generated MIDI bytes/offsets;
  the REAPER render alone does not exclude host MIDI-through.

Every case checks all 192,000 stereo samples, polarity, amplitude and exact
64-sample one-shot extent, then silence. CC-on second amplitude is
`0.25 * 64 / 127`, tolerance 1e-5. All final runs exit normally and their normal
REAPER resource snapshots remain unchanged. No live audio/MIDI device or
callback deadline is qualified.

Early Linux echo attempts exposed fixture errors: two lookaheads caused a
32-sample advance from summed host audio PDC, and a second attempt mixed both
instruments' audio (0.5 amplitude). Fixed the harness by using zero first-instance
latency and disconnecting its audio output pins; retained failed logs and
re-ran all formats successfully. These were fixture corrections, not claimed
adapter defects.

### macOS host attempts and remaining gaps

Used the task's REAPER.app through a private ad hoc signed copy and the new
instrument app identifier. Six CLAP startup attempts and one LV2 attempt timed
out (90 seconds, except one 120-second attempt) **before any host script log**.
A `sample` trace of the isolated process locates it in
`Init_Audio → MessageBox → NSRunAlertPanel`. CUA could not attach a window
(`cgWindowNotFound`), so the exact alert text was not observed. Attempts with
explicit disposable CoreAudio settings, warning masks and a work-directory
current directory did not resolve startup. Only the child test process was
terminated; every normal-resource snapshot stayed unchanged. This supplies no
macOS instrument MIDI/audio/PDC host evidence. VST3 on macOS was skipped as
requested because of the known third-party scan stall; no normal REAPER config,
production project or global audio service was changed. Windows instrument
runs were not attempted.

Local raw evidence is under `target/evidence/instrument/` (bundle checks,
Rust/Python/Rustdoc/Clippy logs, VST3 mock, macOS failed-host logs and startup
sample), `target/reaper-instrument/` and the copied `linux-reaper/` work folders.
Linux originals are in the new checkout's `target/evidence/instrument/` and
`target/reaper-instrument/`. Artifacts are generated/ignored, not published
fixtures. The [support matrix](support-matrix.md#instrument-evidence) keeps
these validator, synthetic-host and actual-DAW observations separate.

The denormal release criterion is checked. The MIDI/timing criterion remains
open: AAX SDK 2.8 has no native tail report and macOS instrument host execution
remains blocked. No blanket platform, editor, device or deadline claim is added.

<a id="release-010-macos-architectures"></a>

## 0.1.0 macOS slices, Rosetta checks and external oversampler baseline — 5 November 2026

Branch `platform/macos-x86-64`, reset from `release/0.1.0` at `eb4d2f5` in its
own worktree. Main/release checkouts were not modified and nothing was pushed.
Environment: Apple M4 Pro, macOS 27.0.1 (26A434), Rust 1.99.1
(`58cf8f9c3`), Apple Clang 21.0.0 (`clang-2100.3.34.2`). Both
`aarch64-apple-darwin` and `x86_64-apple-darwin` Rust targets were already
installed; **no Rust target was installed**. Every x86_64 result below is
**Rosetta 2 on Apple silicon**, not native Intel hardware.

Implementation: unified `--arch arm64|x86_64|universal` defaults to the running
host architecture, builds Rust once per target, combines universal cdylib/AAX
staticlib with `lipo`, and supplies `CMAKE_OSX_ARCHITECTURES` to all C++ adapter,
SDK and helper builds. Explicit architecture outputs and adapter caches are
separate; omitted `--arch` retains the previous native output directory.
Final notices cover the union of selected target dependency graphs. Validators
select each requested slice. LV2's optional pkg-config lilv check is disabled
for universal/non-host builds to avoid its thin arm64 Homebrew dependency;
actual-module contracts still run for each slice. See [decision 0288](decisions/0288-unified-product-bundling-and-foundry.md#macos-architecture-selection--5-november-2026).

SDK environment: `RAVET_VST3_SDK` supplied Steinberg `v3.8.1_build_84`,
`RAVET_AUDIO_UNIT_SDK` supplied `AudioUnitSDK-1.4.0`, `RAVET_AAX_SDK` supplied
SDK 2.8.0 (`AAX_SDK_VERSION 0x0208`), and `RAVET_LV2_SDK` supplied Homebrew
LV2 1.18.10 headers. Downloaded the repository's official checksum-pinned
universal clap-validator 0.4.1 (`152b982`) with
`python3 scripts/fetch-clap-validator.py target/tools/clap-validator`.

Commands from this worktree, with those SDK variables set:

```sh
python3 scripts/bundle-plugin.py --package ravet-delay-plugin \
  --formats clap,vst3,au,aax,lv2 --release --arch universal --validate \
  --clap-validator target/tools/clap-validator/clap-validator
python3 scripts/bundle-plugin.py --package ravet-delay-plugin \
  --formats clap,vst3,au,aax,lv2 --release --arch x86_64 --validate \
  --clap-validator target/tools/clap-validator/clap-validator
python3 scripts/bundle-plugin.py --package ravet-delay-plugin \
  --formats clap --release --arch arm64 --validate \
  --clap-validator target/tools/clap-validator/clap-validator
```

All five universal and all five thin-x86_64 bundles built. `lipo -archs` checked
every shipped Mach-O (nine per architecture selection): universal images have
arm64+x86_64, thin images x86_64 only. External VST3 SDK archives, AU SDK archive
and AAX SDK archive also contain both universal slices. Strict/deep ad hoc
signature verification passed for all 14 final bundles/LV2 images.

| Format | Recorded x86_64 execution, including universal slice |
|---|---|
| CLAP | `arch -x86_64 .../clap-validator validate ...`: 33 passed, 0 failed, 0 warnings, 11 skipped |
| VST3 | SDK validator built with selected CMake architecture(s), run with `arch -x86_64`: 47 passed, 0 failed |
| AUv2 | Exact final component native contracts pass, including retained-view cycles; delay contracts pass. `arch -x86_64 /usr/bin/env ... ravet-auvaltool -v aufx QFKS TYDS`: AU Validation Tool 1.10.0 succeeds |
| AAX | `arch -x86_64 .../ravet_aax_check <final bundle>` passes mono/stereo rendering, eight parameters and saved/corrupt state checks. CTest 1/1; no Avid/PACE validation |
| LV2 | `arch -x86_64 .../ravet_lv2_contracts <final bundle> ...`: 46 checks, 0 failures. CTest 2/2; `lv2_validate`: 0 errors among 84 files, 2,064 restrictions |

Universal CLAP/VST3/AAX/LV2 checks also pass their explicitly selected arm64
slices. AU arm64 exact-bundle contracts pass; system `auval` succeeds but its
registrar may select the already-installed same-identity component. Initial
system-registrar Intel checks could not find the component. Intel validation
now uses a RAVET constructor to register the **exact final component** inside
an ad-hoc-signed build-directory copy of Apple's auvaltool, with original
FourCC/name/version. The system tool and installed component are untouched;
no Apple executable is shipped. The arm64 system validator is arm64e and
cannot load an arm64 constructor, so its earlier temporary-installation path
and documented same-identity caveat remain. Temporary copies were removed.

### REAPER 7.82 / OSX64 under Rosetta

The supplied REAPER.app is universal. The harness's new `--arch x86_64` wraps
its private copy with `arch -x86_64` and selects x86_64 scan keys. A process
sample explicitly reports `Code Type: X86-64 (translated)`. The existing
normal-resource snapshots and foreign-resource abort remain enabled.

Initial CLAP/LV2 runs timed out in `Init_Audio`'s modal without reaching Lua.
Setting **only disposable ini keys** to Mac mini Speakers, 48 kHz, 512 frames
resolved that blocker. Successful VST3 execution additionally used the
per-process `sandbox-exec` wrapper documented in [host validation](host-validation.md#macos-rosetta-2)
to deny the two system VST3 folders; the initial unrestricted scan timed out.
Its fresh private cache names `ravet_delay_plugin.vst3` with the product UID,
not the installed arm64 reference. No installed plug-in was moved or replaced.
AU used the documented process-local registration wrapper pointing to the
new **thin x86_64** component; it needed no global installation. The AU default
FX prefix was corrected to REAPER's `AU: RAVET Delay` after the first AU attempt
logged that candidate but did not match the older prefix.

| Harness run / local log stem | Result |
|---|---|
| `reaper-clap-device` (universal CLAP) | 63 host checks; fixed, recalled and synced renders pass |
| `reaper-lv2-device` (universal LV2) | 63 host checks; all three renders pass |
| `reaper-vst3-private` (universal VST3) | 63 host checks; all three renders pass |
| `reaper-au-local-2` (thin x86_64 AU) | Discovery, parameter edits/readback and recall: 63 checks pass. **Strict renders fail**: fixed/recalled echoes +6,848 frames instead of +4,800 (2,048 late), synced +12,672 instead of +9,600 (3,072 late) |

Every run reports the normal REAPER resource directory unchanged. Renders are
48 kHz stereo impulses, with 100 ms fixed/recalled delay and 200 ms synced delay
at 150 BPM; passing cases include exact amplitudes, dry mix and silence
elsewhere. The harness removes master hardware sends. These runs establish no
live-device, custom-editor or deadline support. AAX is not a REAPER format;
Pro Tools x86_64 was not run. The AU REAPER timing failure remains open.

### External oversampler comparison

A separately authored scratch harness outside RAVET invokes Apple's optimized
`vDSP_biquadm` for the same rounded halfband coefficients, stereo polyphase
branches, branch delays and cascaded 2× stages. It combines pairs of first-order
allpasses into two biquads per branch and processes four branch/channel streams
per library call. The earlier single-channel `vDSP_biquad` baseline was slower;
the recorded verdict uses the faster multichannel variant. There is no imported
implementation code or new RAVET dependency. This qualifies cost relative to
this library-kernel baseline, not all independently designed oversamplers.

| Factor | arm64 RAVET / vDSP ns per stereo frame | Rosetta x86_64 RAVET / vDSP ns per stereo frame |
|---:|---:|---:|
| 2× | 4.658 / 31.301 | 5.241 / 31.886 |
| 4× | 13.481 / 95.794 | 15.183 / 96.304 |
| 8× | 31.137 / 218.174 | 34.704 / 226.529 |

512-frame blocks, f32 planar stereo, identity high-rate processing, persistent
state and prepared buffers; identical timed input copies and no timed heap
operations. Each figure is a median of nine × 4,000 blocks after 1,000 warm-up
blocks. Rust release uses default target features; C++ uses `-O3`, system
Accelerate, and the matching `-arch`; no cross-machine comparison. Continuous
random input over 80 blocks per factor agreed within 9.83e-7 upsampled and
3.69e-7 round-trip. The harness asserts maximum error <1e-5 before timing.

Quality: a 65,536-low-frame impulse run per direction/phase, with a 131,072-point
float64 NumPy 2.4.4 FFT, measured the same filter profile in both implementations.
Up pass/stop bands: 0..0.225 / 0.275..0.5; down: 0..0.22 / 0.28..0.5, in cycles
per high-rate stage sample. RAVET ripple up/down 6.29e-7/8.67e-7 dB and stop
rejection 93.261/85.261 dB on both targets. Accelerate arm64:
1.83e-6/2.54e-7 dB ripple and 93.261/85.262 dB rejection; Rosetta:
1.21e-6/1.18e-6 dB and 93.267/85.261 dB. Impulse tails below 8e-45; these are
finite-grid observations. See the [full method](signal-processing.md#external-oversampler-baseline--5-november-2026)
and [decision 0290](decisions/0290-stereo-block-dsp-kernels.md#external-baseline-evidence--5-november-2026).
Verdict: **competitive at matched quality against the measured external baseline**,
6.7–7.1× cheaper on arm64 and 6.1–6.5× under Rosetta. Native Intel, nonlinear
aliasing and real-time deadlines remain unqualified.

The scratch sources/results are retained outside the repository at
`/Users/davewhiting/.t3/scratch/ravet-macos-010-validation/baseline/` (original
execution directory `/tmp/ravet-macos-010-baseline`). Reproduce with
`cargo build --release --target <apple-triple>`, then
`arch -arm64 target/aarch64-apple-darwin/release/ravet-external-baseline` or
`arch -x86_64 target/x86_64-apple-darwin/release/ravet-external-baseline`.
`--response <directory>` emits each implementation's impulse phases;
`response_quality.py` measures them and `quality.py` computes the ideal rounded
coefficient response on 100,001 points per band. Source SHA-256:

- `baseline.cpp`: `e83fbca30a654639155315c3424d5f4ea35efa2c099fbcd43468ed722d49e514`
- `src/main.rs`: `afc465414ff4479a8646adf6f8005867bfd729d83dbfd1c7ab93492701296199`
- `build.rs`: `39a94526ad9a41bccade08ccc7ab8c4fb83d3f4602ce4d69b20aeedaa320656e`
- `response_quality.py`: `83e589b3bc929df2aee74b78a78a59694fd3742e520f8984f876fbf9fca81f55`

### Checks and remaining scope

- Python script suite: **127 passed**; REAPER harness suite: **15 passed**.
- Stereo block DSP test: **6 passed per target**, arm64 and Rosetta x86_64.
- Workspace Rustdoc: **66 passed**; warnings-denied workspace `cargo doc
  --workspace --no-deps --locked` passed. `cargo fmt --all -- --check` passed.
- `python3 scripts/check-docs.py --write` and `python3 scripts/check-docs.py`
  passed. Final validator/host logs and slice inventory are local under
  `target/evidence/macos-arch/`; release bundles under
  `target/plugins/macos-{universal,x86_64}/release/`. Generated REAPER resource
  payloads and expendable x86_64 intermediates are cleaned after validation;
  logs, final WAVs/projects, validators and bundles are retained. Cleanup
  removed **2.79 GiB**; the retained worktree target directory is about 729 MiB.

Open: native Intel hardware, Rosetta AU strict render timing, x86_64 Pro Tools,
Avid validator, x86_64 lilv/lv2lint, arbitrary product/vendor signing and
live-device/deadline qualification. The release architecture and DSP cost items
are checked only within the explicitly recorded Rosetta/benchmark scope.

<a id="release-010-editors-win-linux"></a>

## Plug-in editors on Windows and Linux, editor asset origin — 5 November 2026

Branch `editors/windows-linux`, based on `release/0.1.0` at `eb4d2f5`
([decision 0293](decisions/0293-cross-platform-plugin-editors-and-asset-origin.md)).

Implemented:

- The native table's `editor_open` accepts `RAVET_PLUGIN_VIEW_WIN32` on Windows
  and `RAVET_PLUGIN_VIEW_X11` on Linux (and still `COCOA` on macOS);
  `editor_idle` pumps GTK on Linux. The CLAP GUI offers `win32`/`x11` and, on
  Linux, registers a 10 ms host timer. The VST3, LV2 and AAX adapters needed no
  change: they already passed these view kinds, so the empty Windows AAX view
  was the table refusing `WIN32`. LV2 product bundles now declare their UI on
  every platform. `RavetPlugin.h` changed in comments only.
- Editor assets: `EditorAsset`/`Product::with_editor_assets` →
  `ravet_webview::WebView::from_html_with_assets` → `ravet_web_create_assets`,
  served from `ravet://editor/` (macOS `WKURLSchemeHandler`),
  `ravet-<module address>://editor/` (WebKitGTK scheme) and
  `https://editor.ravet.invalid/` (WebView2 `WebResourceRequested`). Products
  without assets keep the inline page. The Gain example ships `asset.css`.
- Cargo MSVC builds link `WebView2LoaderStatic.lib` (no loader DLL in bundles);
  `bundle-plugin.py` takes `--webview2-sdk`/`RAVET_WEBVIEW2_SDK`/`[sdks].webview2`
  and copies the SDK's BSD-3-Clause LICENSE and NOTICE into the notices of
  Windows editor bundles (the metadata tool now reports the editor size).
- Backends pin their module once a view exists. Before this, REAPER 7.82 on
  Linux crashed (SIGSEGV in `g_main_context_iteration`, frame in unmapped
  code) about a second after a CLAP Delay with an open editor was removed,
  because it unloaded the library while GLib callbacks remained.
- New checks: `ravet-web-assets` native probe; `ravet_aax_check --editor` on
  Windows (real top-level HWND); `tools/reaper-validation/editor.py` +
  `editor.lua` (disposable REAPER configuration, as `delay.py`) and
  `x11-editor-probe.c` (finds the editor's plug in a named window, prints the
  window chain with map state and `_XEMBED_INFO`, captures only that window,
  injects XTest clicks/drags).

Evidence (5 November 2026):

- **macOS arm64 (macOS 27.0.1):** `bundle-plugin.py --package ravet-delay-plugin
  --formats clap,vst3,au,aax,lv2 --release --validate` passed: clap-validator
  0.3.2 33 passed/0 failed/11 skipped, VST3 validator 47/0, `auval` PASS, LV2
  `CocoaUI` declared and its checks passed; `ravet_aax_check --editor` opened
  both Delay effects' editors in an NSView. `ravet-editor-smoke --generic` on
  the Gain CLAP passed two cycles, now also asserting
  `location.href === 'ravet://editor/index.html'` and the asset stylesheet's
  custom property. `ravet-web-assets` and `ravet-web-lifecycle` passed.
  `cargo test`/`clippy` for `ravet-plugin` (clap, native, editor),
  `ravet-webview`, both examples; plug-in bundle, LV2, Windows-cross, AAX and editor-client script tests (42) passed.
- **Arch Linux x86_64** (kernel 7.2.11, Hyprland/XWayland, WebKitGTK 2.54.1,
  NVIDIA; bundles built with `RAVET_WEB_LINUX_SOFTWARE_RENDERING=1`, as the
  earlier REAPER note requires on this machine): `ravet-web-assets` passed with
  and without software rendering, including the plain X11 parent (plug
  reparented, viewable at 320×180, followed a resize to 400×200); lifecycle and
  host-GTK probes passed with `GDK_BACKEND=x11`; the accelerated paint probe
  fails on this NVIDIA machine as before and passes with software rendering.
  Delay bundles: clap-validator 32/0, VST3 validator 47/0, LV2 contracts 4/4 and
  `lv2_validate` 0 errors with `ui:X11UI`. REAPER 7.82/linux-x86_64,
  `editor.py --probe` for CLAP, VST3 and LV2 (`RESULT PASS`, REAPER exit 0,
  normal REAPER directory unchanged): the window chain was the floating FX
  window → REAPER's plain child window (no `_XEMBED_INFO`) → our GtkPlug
  (`_XEMBED_INFO`, viewable, 560×340); captures show the rendered Delay with
  the host's Feedback edit (47.5 %); an XTest click on Ping-pong logged
  `CHANGE Ping-pong 0 -> 1` and a 40-pixel drag on Mix logged 8–10 steps to
  55 %; close, reopen (state kept) and removal with the editor open passed.
  The Gain CLAP (asset page, resizable) passed the same run; REAPER reopened
  it at 480×321 and the plug followed. clippy for `ravet-plugin`/`ravet-webview`
  with each feature combination and `ravet-plugin` tests passed there.
- **Windows 11 x64 VM** (build 26100, WebView2 Runtime 156.0.4314.58; bundles
  cross-built on the Linux machine with WebView2 SDK 1.0.3650.58, imports
  checked: no `WebView2Loader.dll`): `ravet-web-assets.exe` passed (origin
  `https://editor.ravet.invalid`, 1.5 MiB script, 404, snapshots); REAPER 7.82
  x64 `editor.py --windows-root` runs for CLAP, VST3 and LV2: editor rendered
  (QEMU `screendump`), host Feedback edit shown, close/reopen/removal passed,
  REAPER exited 0; the Gain CLAP asset page rendered and reached `Ready`.
  `ravet_aax_check --editor` opened and rendered both Delay AAX effects'
  editors in a top-level HWND (2 child windows, 5 descendants with WebView2),
  210 timer ticks, closed; `PASS`. Windows clippy (cargo-xwin) clean.

Not established / open:

- No editor→host input on Windows: QEMU monitor clicks landed on the taskbar
  and minimised REAPER, so Windows clicks were dropped from the evidence.
- The Delay's division buttons never responded to XTest clicks in any Linux
  format, while toggles and knob drags did; the protocol accepts the message
  (checked offline). Not investigated.
- The existing `ravet-web-lifecycle` probe fails on Windows with "snapshot not
  delivered"; the unmodified `release/0.1.0` backend fails the same way.
- No Pro Tools for Windows, no LV2 host other than REAPER (jalv and Carla are
  not installed; Ardour is GTK 2-based and refused by design), no keyboard
  input, display scaling above 100 %, Wayland parent or deadline claim.
  WebView2 page content scales with monitor DPI while editor sizes are physical
  pixels.

<a id="release-010-docs-ci"></a>

## 0.1.0 release documentation, checklist audit and hosted CI — 5 November 2026

Branch `release/docs-ci`, based on `release/0.1.0` at `628af5d`. Added the root
[unreleased changelog](../CHANGELOG.md), [draft release notes](releases/0.1.0-notes.md)
and an [evidence-only support matrix](support-matrix.md). The matrix is
hand-maintained because receipts qualify manual observations, missing host
versions and partial checks; feature status alone cannot generate those claims.
It links stable anchors added to the earlier receipts without rewriting their
historical observations. Host runs and validators remain separate. Windows AAX
stays provisional by the recorded owner decision; generic Windows/Linux editors
and Pro Tools keyboard typing remain unsupported.

Audited every open item in release-plan sections 1–4 and 6. Newly checked:
section 2 stable/explicit parameter IDs, automation policies, frozen/direct
bypass, main-thread changes reaching saved state, and state-import/identity
hooks, with source/test/receipt links; section 6 changelog and support matrix.
Every remaining item has a short gap note. The full MIDI criterion stays open
(CLAP note output absent; AAX SDK 2.8 has no native tail property). The denormal
criterion stays open: Rust scope and default export guards exist, but no
separate C scoped-guard entry point is exposed. Assigned standalone, editor/FIR,
SDK-discovery and robustness/notice work was not marked complete. The combined
release-notes/version/tag item stays open. No workspace version or tag changed.
Corrected stale current-guide/catalogue claims of no DAW evidence and the
blanket strict-sample-check claim: Pro Tools has amplitude/normalized-precision
qualifications; Logic bounces contain the metronome.

The new [portable release workflow](../.github/workflows/portable-release.yml)
uses GitHub-hosted `macos-latest`/`windows-latest` on every `main` push, PRs and
manual dispatch. Read-only contents permission, no persisted checkout
credentials, no secrets and no self-hosted runners; maintainer approval for
outside PR workflow runs remains required in repository settings. It builds/
tests the explicit portable default-feature package list, optional data crypto
and the native DSP processor guard, all `ravet-plugin` features and Delay,
bundles release Delay CLAP and validates it. External VST3/AUv2/AAX SDK jobs
and LV2 are outside this workflow. Existing broader runtime jobs are unchanged.

The [download helper](../scripts/fetch-clap-validator.py) pins clap-validator
0.4.1 (`152b982`) universal macOS/Windows ZIP URLs and SHA-256 digests, verifies
before unpacking and copies only the executable. Downloaded both official
release assets; both hashes match GitHub's release API digests. The universal
ZIP's nested tarball is labelled 0.3.2 upstream, but its executable reports
0.4.1. No downloaded source or binary was added to the repository.

Local validation on macOS 27.0.1 (26A434), arm64, rustc 1.99.1 and Python 3.14.7:

- `python3 scripts/check-docs.py --write`, then `python3 scripts/check-docs.py`:
  pass; catalogue regenerated from stable feature records. The checker still
  validates local file links, not support semantics or anchors. Independently
  checked every matrix WORK_STATUS anchor and staged all new release pages
  with `scripts/docs/site.py`; pass.
- `actionlint .github/workflows/portable-release.yml` (1.7.12): pass.
- `python3 -m unittest discover -s scripts/tests -v`: 105 pass, including three
  downloader tests (both archive layouts, corrupt checksum refusal before any
  extraction/write, unsupported platform refusal; synthetic fixtures only).
- `cargo test --locked -p ravet-plugin --all-features -p ravet-delay-plugin`:
  57 unit/integration tests and one doctest pass; two illustrative doc snippets
  remain ignored.
- `cargo test --locked --all-targets` for the workflow's 13 portable/default
  packages: 770 pass. `cargo test --locked -p ravet-data --all-features`: six
  pass. `cargo test --locked -p ravet-native-dsp --all-features`: 65 pass.
- `python3 scripts/bundle-clap.py --plugin delay --profile release`: pass in
  this worktree; the freshly fetched macOS clap-validator 0.4.1 on that bundle
  reports 44 run, 33 passed, 0 failed, 0 warnings, 11 skipped.
- Verified both real pinned archives through the helper's checksum/layout
  reader; executed only the macOS executable (`--version` and validation).
- `git diff --check`: pass.

Not run: either GitHub-hosted job, Windows binaries/native packaging, Linux
commands, any new DAW/editor/device session, or the broader workspace lint/API
build. Rust APIs and example code were unchanged. Local commands are not a
hosted-CI pass: the section 5 every-main-push CI criterion remains unchecked
until execution after merge. No push, tag, outside contribution or modification
of either protected checkout was made.

<a id="release-010-reaper-win-linux"></a>

## Export robustness, dependency and notice review — 5 November 2026

Release-plan review of the plug-in export surface (§1 opt-in dependencies, §6
panic/unsafe review and dependency notices), branch `review/robustness` from
`release/0.1.0` (`628af5d`). Decision
[0287](decisions/0287-export-robustness-and-notices.md) (numbered 0286 on its branch; renumbered at merge because the keyboard-focus record took 0286) records the policy.

Rust sites reviewed (non-test `panic!`/`unwrap()`/`expect()`/`unreachable!`
and `unsafe` blocks, plus panicking indexing on host and audio paths):

| Crate | Panic sites | Unsafe blocks | Fixed | Documented |
|---|---|---|---|---|
| ravet-plugin | 11 | 39 | 10 panic sites; CLAP audio-thread event-buffer growth; `debug_assert!` on the CLAP synchronization path; null/misaligned instance pointers | 1 (`# Panics` on const parameter constructors); 1 missing SAFETY comment; indexing invariants in the table |
| ravet-native | 4 | 65 | 3 panic sites; misaligned handles in `NativeProcessor::control` | 1 (auxiliary scratch); 65 SAFETY comments added |
| ravet-native-dsp (processor, denormal guard) | 1 | 4 | 1 (FP-state mask read on every block) | all 4 blocks already had SAFETY comments |
| ravet-foundation | 8 | 0 (forbidden) | 4 (parameter-state decoding) | 4 (UMP/MIDI packet constructors) |
| ravet-clap | 2 | 0 (forbidden) | 1 (output delivery) | 1 (multi-bus scratch) |
| ravet-signal (delay line, smoothing) | 0 | 0 (forbidden) | — | `DelayLine` index invariant |
| examples/delay-plugin | 10 | 0 (denied) | — | 6 audio-path invariants; 4 in `prepare`/`Default` (control) |

`#![deny(clippy::undocumented_unsafe_blocks)]` now covers library code in
`ravet-plugin`, `ravet-native` and `ravet-native-dsp`.

C++ adapters (VST3, AAX, LV2, AUv2), reviewed in the same change: seven
host-path risks fixed. Exceptions such as `std::bad_alloc` can no longer
escape VST3 factory, component, controller, state, connection, view and timer
callbacks, AAX init, parameter, chunk, display and timer callbacks, the LV2
descriptor entry points, or the AUv2 render. A half-built VST3 factory is no
longer published. AAX `GetChunk` refuses state larger than the last
`GetChunkSize`. Out-of-range LV2 `time:Position` values saturate instead of
overflowing. AUv2 `ScheduleParameter` stages into prepared storage instead of
zeroing 56 KiB of stack per call.

New tests, each failing before its fix:
- `examples/delay-plugin/tests/clap.rs`: a 1,025-event CLAP block is silenced,
  its values are not applied, and no process call allocates. Before the fix it
  allocated once and changed the cache.
- `tools/clap-smoke/src/delay.rs`: the same rejection through the built bundle.
- `crates/ravet-plugin/src/native/tests.rs`: null and misaligned table
  instances are rejected.
- `crates/ravet-native/tests/entry.rs`: misaligned handles. The old code
  crashed with SIGSEGV.
- VST3 `contracts.cpp`: allocation failure under every guarded entry point.
  The old code terminated with an uncaught exception.
- AAX `adapter-tests.cpp`: chunk sizing.
- LV2 `contracts.cpp`: saturating positions, 106 checks. The old code failed
  under UBSan.

Dependencies:
- `ravet-plugin` gains an `editor` feature (umbrella `plugin-editor`).
  `clap` and `native` no longer link `ravet-webview`. The delay and gain
  examples enable it.
- `sha2` and `serde_json` stay required, for the reasons in
  [dependency notices](dependency-notices.md).

Notices:
- The export graph has 20 third-party crates, all permissive, with no copyleft
  licences. The audit for both licensing routes is recorded in
  [dependency notices](dependency-notices.md).
- `scripts/third_party_notices.py` generates `THIRD-PARTY-NOTICES.txt`, RAVET's
  licence and the Rust standard library notice. Every bundle script now ships
  them.

Local macOS arm64 validation (Rust 1.99.1, Python 3.14):
- Format checks:
  - `bundle-clap.py --plugin delay` followed by `ravet-clap-smoke --delay`
    passed. Its Resources/notices folder is present.
  - VST3 delay: Steinberg validator 47 passed, 0 failed.
  - LV2 delay: CTest 4/4; `lv2_validate` reported 0 errors.
  - AUv2 delay `--validate`: product and real-delay contracts passed.
  - AAX delay: `ravet_aax_check` 1/1.
  - Each format's bundle contains the notices.
- Mock adapter suites: VST3 CTest 1/1; AAX CTest 2/2; LV2 2/2 under Release
  and under ASan+UBSan; AU effect, instrument and MIDI mocks all passed.
- Rust and scripts:
  - Clippy with `-D warnings` (default and all features) and tests (674 with
    all features, 0 failed) for the touched packages.
  - `cargo fmt --check`.
  - `python3 -m unittest discover -s scripts/tests`: 105 passed.

Not run:
- Windows and Linux builds of the changed adapters.
- clap-validator (not installed here), Avid's validator, and any DAW.

Open:
- `ravet-webview` (about 75 unsafe sites and its Objective-C++ view, used by
  the macOS editor) and the standalone runner were outside this review.
- The dependency-notice audit covers only the plug-in export graph.


## Unified product bundler and generic Foundry — 5 November 2026

Implemented decision [0288](decisions/0288-unified-product-bundling-and-foundry.md)
on `tools/bundle-foundry`, based on `release/0.1.0` at `628af5d`. The
[bundle command](bundle-plugin.md) compiles the selected product once (cdylib
and staticlib together for AAX), reads its native table, calls importable format
builders, collects selected bundles, signs and optionally validates final copies.
SDK discovery uses explicit flags, environment, nearest project TOML and user
TOML in that order, with no probes into other products. Vendor signing hooks are
argument arrays; credentials/configuration stay with the vendor. The existing
CLAP reference command shares the prebuilt packaging function and its explicit
SDK-based companion commands remain available.

Foundry now creates consumer-owned `Processor` + `PluginExport` projects for
effect and monophonic sine roles, with no copied reference wrappers. Its Build
calls the same command and checks SDKs before dependency resolution. DSP/editor
source remains untouched by regeneration; historical wrapper projects require
explicit migration. AUv3 has no generic adapter and is rejected. Local dependency
paths remain the default; GUI/CLI share one default constant, and the starter has
its own one-line default constant, ready for the release-time tag switch.
Only the SDK discovery/build-integration release item was ticked; no tag or
release-default switch was made.

macOS evidence: arm64, macOS 27.0.1 (26A434), Rust/Cargo 1.99.1, Python 3.14.7,
CMake 4.4.3, Xcode macOS 27.0 SDK. External VST3, AudioUnitSDK, AAX and Homebrew
LV2 SDK paths were supplied through `/tmp/ravet-bundle-sdks.toml`, never source
constants. An existing clap-validator was supplied on PATH. Ran:

```sh
python3 scripts/bundle-plugin.py --package ravet-delay-plugin \
  --formats clap,vst3,au,aax,lv2 --settings /tmp/ravet-bundle-sdks.toml \
  --release --validate
```

All five final bundles are under `target/plugins/release/` with Cargo package
filenames. Final results: clap-validator 0.3.2, 44 tests, **33 passed, 0 failed,
0 warnings, 11 skipped**; Steinberg validator **47 passed, 0 failed**; native AU
metadata/render/editor lifetime contracts (three retained-view cycles), Delay
8 kHz sample-2000 repeat/state/bypass/offset-16 automation contracts and
**auval 1.10.0 succeeded** (`aufx/QFKS/TYDS`); AAX SDK version 0x20801345980,
CTest **1/1** plus final synthetic check **PASS** (mono/stereo, eight parameters,
state rejection/recall); LV2 CTest **4/4** (mock contracts, mock lilv, product smoke,
product lilv), `lv2_validate` **0 errors among 84 files, 2064 restrictions**.
AU validation used the tool's exclusive temporary component destination and
confirmed no `ravet-validation-*` components remained afterwards. macOS output
images/bundles used default ad hoc signing. A real fake vendor Python command
was also exercised by unit tests, including argument boundaries, format/path
environment, exactly-once invocation and failure propagation; no production
signing credentials were configured or tested.

External Foundry scratch workspace `/tmp/ravet-generic-foundry.iWwydq`: generated
`foundry-generic-effect` with all five formats, generated its offline lockfile,
and built all five with the bundle command using `--manifest-path`, shared
`CARGO_TARGET_DIR` and the SDK settings above. Its final VST3 separately passed
**47/0** after the filename fix below. Generated `foundry-generic-instrument`
and built its CLAP through `ravet-foundry build <scratch>/instrument --offline
--bundle`; the reported artifact was in its own `target/plugins/debug/`.
`python3 scripts/check-foundry.py` and `--build` both **PASS**, including disposable
offline, standalone hidden UI/audio and effect/instrument CLAP workflows.

Optional Linux/Windows evidence: synced this source, excluding `.git` and
`target`, into the new `~/ravet-bundle-foundry-1793853311` on `comfy-omarchy`
(Arch Linux x86_64, cargo-xwin 0.23.1, clang 23.1.3, CMake 4.4.4, Wine 11.20).
SDKs were supplied by `RAVET_VST3_SDK`, `RAVET_AAX_SDK`, `RAVET_LV2_SDK`.
Native `--formats clap,vst3,lv2 --release` built all three. Native
`--formats vst3,lv2 --release --validate` passed Steinberg **47/0**, LV2 CTest
**4/4** and `lv2_validate` **0 errors, 84 files, 2056 restrictions**.
`--target windows --formats clap,vst3,aax,lv2 --release` built all four with one
Rust product invocation, including the Wine metadata reader and Turtle generator.
Subsequent Windows VST3 validation under Wine passed **47/0**; AAX/LV2
`--target windows --formats aax,lv2 --release --validate` passed AAX CTest **1/1**
and the final synthetic bundle **PASS**, LV2 CTest **2/2** and Turtle validation
**0 errors, 84 files, 2056 restrictions**. No CLAP validator was available on
that Linux machine for native or Windows CLAP checks. The CLI accepts a
Windows validator through `--clap-validator`; its Wine command path has unit
coverage, but was not exercised with a Windows CLAP validator here.

The optional validation caught final-bundle naming requirements on Linux VST3
and Windows AAX. Builders now emit the final Cargo package name internally as
well as externally, rather than renaming only the enclosing directory; regression
tests cover the names. The reruns above passed. No Windows VM or DAW run was
performed for this change. AAX evidence is a synthetic host, not Avid validation
or Pro Tools; neither these checks nor prior history implies production signing,
editor, platform or real-time deadline support.

Repository verification: Python script suite **115 passed**; Foundry integration
suite **13 passed**; `cargo fmt --all -- --check`,
`cargo clippy -p ravet-foundry --all-targets --locked --jobs 2 -- -D warnings`,
`cargo test --workspace --doc --locked --jobs 2`,
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked --jobs 2`,
and documentation generation/check **passed**. Only needed build packages plus
the mandated workspace Rustdoc checks were built. No push, release tag, main
checkout or protected release checkout write was performed.

## State-format policy and stereo block DSP — 5 November 2026

Branch `policy/state-and-dsp` from `release/0.1.0` (`628af5d`). Machine: Apple
M4 Pro, macOS arm64, rustc 1.99.1; shared with other sessions (load average
6–9 during measurement).

**State and parameter IDs.** Inventoried every saved format and parameter-ID
scheme in the supported surface against the compatibility policy; the table is
in the [0.1.0 upgrade notes](releases/0.1.0-upgrade.md). Only `RVETTREE`
observable-tree records (and so `.ravetpreset` files) needed changes: format 2
adds a product schema version, newer formats and schemas reject with distinct
errors, 0.0.1 format-1 records still load as schema 1, and
`Store::restore_migrating` / `PresetLibrary::load_into_migrating` migrate older
schemas atomically ([decision 0289](decisions/0289-versioned-tree-records.md)).
Foundation gained `parameter_record_version`. `cargo test -p ravet-state -p
ravet-preset -p ravet-foundation` passes, including a hand-built 0.0.1 record.

**Stereo FIR and oversampler.** Added `StereoFir` (f32/f64) and
`StereoOversampler` ([decision 0290](decisions/0290-stereo-block-dsp-kernels.md),
[guide](signal-processing.md#stereo-block-fir-and-oversampling)). Tests compare
the FIR with an independent double-precision convolution (1–1,024 taps, many
block shapes) and the oversampler sample-for-sample with `Oversampler` at every
factor; both process with zero counted allocations. The x86_64-apple-darwin
build passes the same tests under Rosetta 2. Release-mode medians at 512-frame
blocks, ns per stereo frame: f32 FIR 0.91/3.10/12.0/47.4 for 16/64/256/1,024
taps (Apple Accelerate `vDSP_conv`, separate harness: 0.78/2.51/11.4/49.1;
existing `FirConvolver` ×2: 13.5/79.7/389/1,577); oversampler 4.64/13.7/31.2 at
2×/4×/8× (per-sample `Oversampler` ×2: 16.0/38.3/100). One machine only; no
external oversampling baseline and no native x86_64 measurement, so the
release-plan item stays open.

## REAPER delay check on Windows and Linux VST3 — 5 November 2026

Ran the scripted REAPER delay check
([host validation](host-validation.md#windows)) for VST3 on Linux and for
CLAP, VST3 and LV2 on Windows. Branch `evidence/reaper-win-linux` from
`release/0.1.0` (`b0c79ea`). All bundles were built from that source on the
Linux machine: `bundle-vst3.py --plugin delay --profile release` (VST3 SDK
3.8.1; its validator: 47 passed, 0 failed), and `bundle-clap.py`,
`bundle-vst3.py` and `bundle-lv2.py --plugin delay … --target windows` (LV2
1.18.10 headers; cross-built as in decision 0285).

Machines: Arch Linux x86_64 (kernel 7.2.11-arch1-1, Python 3.14.8), REAPER
7.82/linux-x86_64 with JACK; Windows 11 Enterprise Evaluation x64 VM
(NT 10.0.26100.0, Windows PowerShell 5.1.26100.9711, no Python), REAPER
7.82/x64 at `C:\REAPER` (an installed copy: its configuration is
`%APPDATA%\REAPER`; `C:\REAPER` holds no ini), audio system WaveOut.

| OS | Format | Discovered as | Runs | Host checks per run | Renders | Normal config |
| --- | --- | --- | --- | --- | --- | --- |
| Arch Linux x86_64 | VST3 | `VST3: RAVET Delay (RAVET)` | 3 PASS | 63 PASS | 18 PASS, 0 FAIL | unchanged |
| Windows 11 x64 | CLAP | `CLAP: RAVET Delay (RAVET)` | 3 PASS (final settings) | 63 PASS | 18 PASS, 0 FAIL | unchanged |
| Windows 11 x64 | VST3 | `VST3: RAVET Delay (RAVET)` | 3 PASS | 63 PASS | 18 PASS, 0 FAIL | unchanged |
| Windows 11 x64 | LV2 | `LV2: RAVET Delay (RAVET)` | 3 PASS | 63 PASS | 18 PASS, 0 FAIL | unchanged |

In every passing run the rendered samples (48 kHz, 192000 frames per channel,
both channels) were: `fixed` and `recalled` echoes at frames 28800 and 100800
(+4800 frames = 100 ms after the impulses at 24000 and 96000), amplitudes
0.5000000/−0.2500000, dry 0; `synced` (1/8 at 150 BPM, mix 50) echoes at
33600 and 105600 (+9600 frames = 200 ms), amplitudes 0.2500000/−0.1250000,
dry 0.2500000/−0.1250000; maximum residual elsewhere 0. Every edit read back
exactly and both reopened projects recalled every control and the tempo. VST3
on both systems reports normalized 0..1 ranges and was driven through the
formatted readback; CLAP and LV2 reported physical ranges. REAPER quit by
itself with exit status 0: Linux VST3 in 2.0 and 3.0 s (the two runs whose
driver output was kept), Windows in 28.2–35.8 s (about 20 s of it before the
script started). After the changes below, Linux CLAP and LV2
were run once more from this branch: both PASS (63 host checks, renders as
above).

Normal configuration: on Linux the driver's check of `~/.config/REAPER`
passed every run. On Windows, `run-windows.ps1` hashed (SHA-256, size, mtime)
the 19 top-level files in `%APPDATA%\REAPER` and `C:\REAPER` before and after
each run; all 19 Windows runs, including failed and killed ones, left them
unchanged, and the snapshot before the first run equals the one after the last.

Procedure on Windows (the VM has no Python): `delay.py --windows-root
C:\Users\shine\ravet-reaper` prepared each work directory on Linux, `scp`
copied it to the VM, `schtasks /create … /it` + `schtasks /run` started
`run-windows.ps1` in the logged-on user's console session, Linux polled for
`run-windows.done`, deleted the task, copied the directory back and ran
`delay.py --check-only`.

Problems found and fixed in the harness (`tools/reaper-validation`):

- Run from the OpenSSH session (session 0, not interactive), REAPER wrote no
  host log within 120 s and was killed. The scheduled-task route above runs in
  session 1 and works. No Python is needed on Windows: `--windows-root`
  rewrites the ini and script paths for the Windows copy and writes the
  runner; `--check-only` checks a copied-back directory and its guard snapshots.
- REAPER stopped at "You have not yet selected an audio device" (a VM
  `screendump`). Windows configurations now get `[audioconfig] mode=2`
  (WaveOut); `--ini` accepts `[SECTION]KEY=VALUE`. With `mode=5`, REAPER
  reported no audio system and CLAP edits never reached the plug-in (Time
  stayed at its 375 ms default after 10 s), so the run failed.
- With WaveOut, 2 of 6 runs passed. The other 4 stopped mid-script (after a
  render, before or at a project reopen) and were killed at the timeout; for 3
  of them a `screendump` taken 60 s in showed REAPER's "New Version
  Notification" (REAPER 7.84 available) over the main window, and the fourth
  was not captured. `verchk=0` is now written on every platform; the 9 Windows
  runs after it all passed.
- The Windows plug-in path keys are `clap_path_win64`, `lv2path_win64` and
  `vstpath64` (named in `reaper.exe`; the unconfirmed candidates were removed).
- The ReaScript logs `Audio system <name>` (`GetAudioDeviceInfo('MODE')`).

`python3 -m unittest tools/reaper-validation/test_delay.py
tools/reaper-validation/test_checks.py`: 14 tests pass on macOS and Linux (3
new: ini sections and `verchk`, Windows preparation and runner, guard
comparison and `--check-only`).

Not run: the in-place driver on Windows with Python, macOS with this branch's
changes (no REAPER in `/Applications` on the Mac used here; the changes add
the `verchk=0` ini entry and a log line there), VST3 on macOS, AU, and any live playback,
device, deadline, editor or automation check. The Windows evaluation "About
REAPER" window appeared on screen during runs but did not block them.

<a id="release-010-reaper-delay"></a>

## Scripted REAPER delay check — 5 November 2026

Added an unattended REAPER check for the RAVET Delay product
(`tools/reaper-validation/delay.py`, `delay.lua`, `delay_checks.py`; see
[host validation](host-validation.md#scripted-ravet-delay-check-any-format-macoslinuxwindows)).
One command per format builds a disposable resource directory, loads the
bundle by format name, edits and reads back parameters, renders three cases,
checks saved-project recall, quits REAPER and verifies the rendered samples.
Branch `tools/reaper-harness` from `release/0.1.0` (`4c0b2f2`); bundles were
built from that source with `bundle-clap.py --plugin delay --profile release`
and `bundle-lv2.py --plugin delay` (LV2 1.18.10 headers).

Executed, all with REAPER 7.82 and 48 kHz 24-bit PCM master-mix renders:

| OS | Format | Discovered as | Host checks | Renders |
| --- | --- | --- | --- | --- |
| macOS 27.0.1 (26A434) arm64, REAPER 7.82/macOS-arm64 | CLAP | `CLAP: RAVET Delay (RAVET)` | 63 PASS | PASS |
| macOS 27.0.1 arm64 | LV2 | `LV2: RAVET Delay (RAVET)` | 63 PASS | PASS |
| Arch Linux x86_64 (kernel 7.2.11), REAPER 7.82/linux-x86_64 | CLAP | `CLAP: RAVET Delay (RAVET)` | 63 PASS | PASS |
| Arch Linux x86_64 | LV2 | `LV2: RAVET Delay (RAVET)` | 63 PASS | PASS |

In all four runs: every parameter edit read back exactly (time 100 ms,
feedback 0, mix 100/50, ping-pong 0, sync 0/1, division 6 = 1/8); both
reopened projects recalled every control and the 150 BPM tempo; REAPER quit by
itself with exit status 0 (1.5–6.5 s per run); the normal REAPER resource
directory was unchanged during each run. Rendered samples (192000 frames per
channel, both channels):

- `fixed` and `recalled` (sync off, 100 ms, mix 100): echoes at frames 28800
  and 100800, exactly +4800 frames after the impulses at 24000 and 96000,
  amplitudes 0.5000000/−0.2500000; dry 0; maximum residual elsewhere 0.
- `synced` (sync on, 1/8 at 150 BPM, mix 50): echoes at frames 33600 and
  105600, exactly +9600 frames (200 ms), amplitudes 0.2500000/−0.1250000 and
  dry 0.2500000/−0.1250000; maximum residual elsewhere 0.

Problems found while making it unattended, now handled by the driver:

- macOS showed its "REAPER unexpectedly quit while reopening windows" prompt
  (tied to the `com.cockos.reaper` bundle identifier) after a terminated run.
  The driver now runs a private ad hoc signed copy of REAPER.app with bundle
  identifier `org.ravet.reaper-harness`.
- REAPER always appends the system VST3 folders to the VST path; on this Mac
  the startup scan stalled on an unresponsive third-party VST3. The driver sets
  `vst_scan=2` (no startup VST scan; CLAP and LV2 still scanned) unless VST3
  is under test.
- CLAP parameter edits reach the plug-in only at its next process/flush, so
  readback waits in a `defer`-driven coroutine (bounded at 10 s).
- An early attempt passed `-ApplePersistenceIgnoreState YES` before `-cfgfile`;
  REAPER then ignored `-cfgfile` and started with the normal user resource
  directory on this Mac, and a `CLAP_PATH` variable set by that driver let it
  scan the test bundle. Three such launches (about 03:07–03:10) rewrote REAPER's
  own files there (`reaper.ini`, `reaper-reginfo2.ini`, `reaper-midihw.ini`,
  plug-in scan caches; the CLAP cache gained a `RAVET Delay.clap` entry). No
  project was saved or modified, and the instance was killed without quitting.
  The argument and the environment variable were removed (`--env-path` opts
  back in), and the resource-directory guard and changed-file check were added.

Not run: Windows (commands are documented; its plug-in path ini keys are
unconfirmed), VST3 and AU (accepted by format name; their adapters are on other
branches). Live playback, audio devices, deadlines, the editor and automation
are outside this check. `python3 -m unittest tools/reaper-validation/test_delay.py
tools/reaper-validation/test_checks.py`: 11 tests pass.

<a id="release-010-lv2-adapter"></a>

## Generic LV2 adapter over the plug-in table — 5 November 2026

Added the generic LV2 export ([decision 0283](decisions/0283-generic-lv2-adapter.md),
[guide](lv2.md#generic-lv2-export)): one DSP module, one UI module and a Turtle
generator in `crates/ravet-plugin/lv2`, written once against `RavetPlugin.h`,
plus a C++ mock table and contract tests. `scripts/bundle-lv2.py` gains
`--plugin delay|generic|mock` and `--crate`; the 0.0.1 reference profiles are
unchanged. Built against the table from `plugin/native-table` (21088c5).

Validation on macOS 27 arm64, AppleClang, CMake, external LV2 1.18.10 source
headers, Homebrew lilv 0.28.0, sord 0.16.22 and lv2 1.18.10:

- `bundle-lv2.py --plugin mock`, `--plugin generic` and `--plugin delay` each
  exit 0. CTest passes `ravet_lv2_mock_contracts` (100 checks, 0 failures),
  `ravet_lv2_mock_lilv` (12 checks), `ravet_delay_smoke` (46 checks, including
  the real delay editor opened in an `NSView` through the CocoaUI module, idled
  and resized), `ravet_delay_lilv` (8), `ravet_generic_smoke` (40) and
  `ravet_generic_lilv` (8). With `RAVET_LV2_UI=OFF` the delay bundle ships no UI
  and the same suites pass.
- `lv2_validate` (sord_validate against the LV2 1.18.10 specifications) on each
  packaged bundle: 0 errors (mock 2,013, gain 1,974, delay 2,064 restrictions
  checked). `codesign --verify --strict` passes on all nine binaries.
- `lv2ls` lists `urn:org.ravet.mock`, `urn:org.ravet.gain-generic` and
  `urn:org.ravet.delay`; `lv2info` shows the latency port, optional features,
  `state:interface` and the CocoaUI.
- `lv2apply` (no host features, one frame per run) on a 1 s stereo 48 kHz
  sine with peak 16,000: mock `gain -6` peak 8,019; gain `gain 0.5` peak 8,000,
  with `enabled 0` 16,000. Delay on an impulse of 20,000 with `sync 0, time 100,
  mix 100, feedback 0`: echo of 19,999 at frame 4,800 exactly; with `enabled 0`
  no echo.
- `python3 -m unittest` of `scripts/tests/test_lv2_generic.py` (5 tests) and
  `test_lv2_bundle.py` (6 tests) pass.

Not established: any DAW (no LV2 DAW was run), Linux or Windows builds or runs,
the X11 and Win32 UI paths with a real parent (tested with the mock and fake
parents only), physical MIDI, or real-time deadlines. Linux commands are in the
guide.

## Public repository preparation — 21 October 2026

The hosted-history purge had not taken effect after more than a week, so the
0.0.1 source preview is published from a new repository holding only the
cleaned history; see [publication policy](publication.md). Self-hosted CI jobs
no longer run for pull requests: documentation and embedded checks use a
GitHub-hosted Ubuntu runner for them, and the omarchy Linux workspace job runs
only after merge. See [maintenance](maintenance.md#continuous-checks-and-reference-artifacts).

Two small native fixes are included. Optional VST3 interface queries treat
`kResultFalse` with no pointer as an absent interface, as JUCE-built plug-ins
answer that way instead of `kNoInterface`; this is source review against those
wrappers, not new hosted-plug-in evidence. The standalone window's Quit menu
item names the window title instead of a fixed "RAVET Examples" string.

Local validation of this tree on macOS arm64 before the public push: formatting,
all-target/all-feature workspace tests, Clippy with warnings denied, all-feature
doctests and warning-free Rustdoc pass (1,679 Rust tests passed, none failed,
four ignored), as do 68 Python script tests, `actionlint`, the documentation
checker and the publication path checker. This is local evidence only.

## Device nominal-rate admission — 13 October 2026

Completed shared control-thread sample-rate admission for all seven direct CPAL
stream builds in `ravet-device`, including input/duplex, clocked and reclaimable
paths; graph/file/recording/recovery adapters inherit it through their openers.
A successful build with a different or unreadable rate is dropped before return.
Duplex construction rechecks input after output to catch shared-device changes.
Known-rate CPAL rate-update/listener failures return explicit mismatch receipts,
including equal-rate timeouts, with no automatic retry and unchanged reclaimable
retirement ownership. Added `device_nominal_sample_rate` and four provisional
opening-error variants. See [decision 0277](decisions/0277-device-rate-admission.md)
and [audio devices](audio-devices.md).

CPAL 0.18.2 source review found its CoreAudio default configurations read stream
ASBDs, rather than the nominal property. The macOS helper resolves its public UID
and queries the nominal property directly. Other backends expose only current
CPAL default-config proxies; their actual negotiation is not qualified. The
standard popup now prefers a new device's nominal rate, preserves supported
saved preferences, displays reported mismatches and offers an explicit supported
actual-rate selection without starting. The interactive examples provide the
snapshot signal and prepare again on explicit Start.

Validation on sandboxed macOS arm64, Rust 1.99.0, Cargo locked/offline:

- All 128 tests/doctests for `ravet-device`, `ravet-examples` and `ravet-webview`
  pass, including 40 device unit tests and the typed example snapshot test.
  Fakes exercise stream disposal, failed queries, message classification,
  equal-rate timeouts, shared-clock changes and ordinary/paired retirement.
- The actual popup script passes a Node DOM/bridge fake for device switches,
  saved settings, supported/unsupported/equal-rate mismatch selection,
  independent/stale inputs, running controls and explicit Start. All 68 Python
  tests pass, including this test.
- Workspace check, all-target Clippy with warnings denied, formatting,
  documentation registry/checker, all 64 workspace doctests and workspace
  Rustdoc with warnings denied pass.
- The requested full workspace test command stops at two unchanged
  `ravet-media` H.264 `Decode` failures. A complete `--no-fail-fast` run records
  1,287 passes and 14 failures across seven unchanged targets: two video tests,
  eight Metal tests returning `Backend`, one OSC loopback test and three runtime
  TLS tests. The network failures explicitly report `Operation not permitted`;
  video also reports a restricted `sysctlbyname` call. No changes were made to
  these crates or their tests; this environment cannot establish a green full
  workspace gate.
- The hidden native UI probe fails at its pre-existing opt-out page readiness
  assertion before the added rate fixture. Logs show sandbox-denied WebKit paths
  and unavailable XPC services. The new hidden rate fixture is provided but
  native integration qualification remains pending; the DOM fake is not a
  substitute for that qualification.

No real audio devices were opened. Physical/virtual-device rate changes, audible
pitch correctness, later routing changes, other platforms and real-time deadlines
remain unvalidated. Scratch logs and the empty screenshot directory were removed;
normal Cargo build artifacts remain under this worktree's `target/`. No commit
was created; integrator review and commit remain external to this session.

## Hosted documentation publishing prepared — 11 October 2026

Prepared, not deployed: `.github/workflows/hosted-docs.yml` builds the hosted
reference with the website's page shell and tests on documentation-input pushes
to `main`, then publishes only `/ravet/docs/` through a docs-only FTPS account by
uploading to a staging directory and swapping it with the live one. The build
now writes `docs/version.json` with the source commit for live checks. See
[hosted documentation](../scripts/docs/hosted/README.md#automatic-publication).

The complete navigation tree is now one content-hashed `docs/nav-<hash>.json`
rendered by the website script; each page embeds only a static fallback for the
current path. Branch IDs, default and current-path opening, and current-page
highlighting match the previous embedded sidebars for all 2,100 pages; the only
change is that the two overview pages now highlight themselves. Rustdoc now runs twice, the second time one
crate at a time, so cross-crate links no longer depend on parallel build order.

Local evidence on macOS arm64 with Rust 1.99.0:

- The website builder generated 2,100 pages, 2,105 files and about 90 MB from
  this tree. Before the navigation change the build produced 1.8 GB, with a
  median page of 856 KB; the median page is now 30 KB.
- Two consecutive builds were byte-identical.
- The website's `ravet.test.js` passed within a 512 MB V8 heap in 3 s. The
  previous test ran out of memory at 2 GB.
- A dry run against a local directory standing in for the server published,
  swapped, refused unsafe or incomplete snapshots, left the live snapshot
  untouched after an upload failure, rolled back twice, and changed nothing
  outside the release root.
- Headless Chrome confirmed rendering, keyboard expansion, current-page
  highlighting, remembered state and the no-JavaScript fallback.

No FTPS host, GitHub runner, LiteSpeed server or live publication has been
exercised.

## 0.0.1 source preview preparation — 10 October 2026

Aligned all 35 workspace package versions and lock entries, public VST3/AUv2
reference metadata, mobile fixture versions and the Arduino library to 0.0.1.
Foundry native adapters continue substituting each independent consumer's own
version; project starters retain their own initial product version. Added
[release notes](releases/0.0.1.md) and corrected the README dependency summary.
The existing remote feature-ordering commit is preserved in the release history.

Local preparation checks on macOS 27.0.1 arm64 with Rust 1.99.0: documentation
checker, 43 packaging/documentation Python tests, four host-evidence Python tests,
and all 11 native WebKit CTest probes pass. Fresh 0.0.1 locked workspace check,
all-target/all-feature tests (763 passed, four external decoder diagnostics
ignored), all-target/all-feature Clippy with warnings denied, default/all-feature
doctests (18/23 passed) and default/all-feature Rustdoc with warnings denied pass.
Formatting and Git diff checks pass without first-party warnings. Foundry's
external-consumer workflow and effect/instrument metadata propagation at product
version 2.3.4 pass. Rust artifacts use a separate local build directory because
an earlier programme build occupied the shared target directory. Native evidence
is scoped to the synthetic fixtures; no broader platform, DAW/device or deadline
support is inferred.

The publication path checker passes over staged files and reachable local refs.
A targeted credential-pattern scan found no matches in 1,025 reviewed working
files or 1,392 historical blobs. Public image/audio/font fixture provenance and
retained codec/drawing notices were reviewed; all 265 resolved external packages
in all-feature Cargo metadata declare a licence or licence file. These checks are
bounded source/metadata review, not an exhaustive credential or distribution
licence audit. Cargo publishing remains disabled, commercial terms remain a
draft and no source archive or binary release is being uploaded.

GitHub reports the existing remote is private. The hosted old-history limit in
[publication policy](publication.md) remains unresolved: remove hosted copies or
publish only the clean history to a new repository before public visibility.
Verbatim third-party notices retain original whitespace through scoped Git
attributes; normal checks continue for first-party files. Current validation logs
are local `target/release-0.0.1-*.log` receipts; generated build output is excluded
from the commit. The actual ad hoc signed 0.0.1 release CLAP gain bundle passes
the smoke host's 18 processing blocks and six activation cycles. The official
local CLAP validator reports 34 passed, zero failed/warnings and ten skipped
checks (44 total). This verifies the trusted reference bundle, not general DAW
compatibility or production signing/distribution.

## Knob and fader pointer editing defaults (10 October 2026)

All shared knobs and sliders/faders leave paired numeric editing when pointer
adjustment begins, suppress pointer focus outlines and prevent surrounding label
activation from opening a numeric editor on release. Native range dragging stays
browser-owned; pointer release/cancel leaves controls unfocused so host refresh
can resume. Tab/navigation input restores keyboard styling. Direct numeric
selection/entry remains available. See [web controls](web-controls.md) and the
[updated rotary-control decision](decisions/0132-standalone-settings-and-knobs.md).

On local macOS arm64, the complete 11-test native editor suite passes. The expanded
semantic fixture passes 28 programmatic contracts, including paired number focus,
label activation, host refresh, keyboard input and exact attribute retirement.
Real AppKit knob and native-range fader drags also pass in a visible owned WebKit
window, including number entry and computed keyboard outline checks. Evidence is
in `target/web-pointer-tests.log`, `target/web-pointer-native-tests.log` and
`target/web-pointer-controls/Testing/Temporary/LastTest.log`. This is scoped local
UI evidence; other browser/platform and assistive-technology validation remain
independent. Workspace doctests, warnings-denied workspace Rustdoc, formatting
and the documentation checker pass. Cargo evidence is in
`target/web-pointer-doctests.log` and `target/web-pointer-rustdoc.log`; doctest
compilation also reports two existing `jpeg-encode` feature-configuration warnings.

## WebKit context menu defaults (10 October 2026)

Cocoa/WebKit and Linux/WebKitGTK now install document-start context-menu
suppression in all frames through their shared editor creation paths. Browser
menus no longer compete with consumer right-click reset/custom-menu actions;
mouse/pointer events and context-menu propagation remain available. Embedded and
standalone WebKit consumers inherit the default. See the [native editor
guide](native-editor.md) and [decision 0205](decisions/0205-webkit-context-menus.md).

On local macOS arm64, the new hidden synthetic DOM probe passes file/inline
creation, repeat loads, child frames, early cancellation, right-click reset,
custom-handler propagation and unchanged left clicks. All nine existing native
probes also pass. The initial child-frame fixture used a srcdoc navigation
rejected by the existing navigation policy; an explicitly allowed about:blank
frame fixes the fixture and the failed-test rerun passes. Evidence logs are
`target/web-context-menu-tests.log` and `target/web-context-menu-recheck.log`.
Workspace doctests, warnings-denied workspace Rustdoc and the documentation
checker pass; Cargo evidence is in `target/web-context-menu-doctests.log` and
`target/web-context-menu-rustdoc.log`.
Linux runtime, actual OS context-menu presentation and DAW routing validation
remain pending; Windows/WebView2 retains its existing policy.

## Foundry native directory selection (10 October 2026)

Added accessible folder-icon buttons to all seven SDK/framework/project-directory
fields. Initialized native frontends open Cocoa, Windows COM or GTK directory
choosers, seeded at the current value's nearest existing directory. New-project
browsing chooses a parent and appends the package name, retaining exclusive
creation. Cancel leaves the field unchanged; accepted choices edit the form
without saving. Bounded whitelisted requests and sequenced snapshots preserve
subsequent typed edits and permit reselecting the same folder. The macOS bridge
queues chooser work outside WebKit callback borrows to permit modal-loop events.

On local macOS arm64, 11 Foundry tests and warnings-denied all-target Clippy pass.
The hidden native UI fixture clicks the SDK icon, opens/cancels a real Cocoa
chooser, checks unchanged input, publishes an accepted canonical directory through
the native snapshot bridge, then creates/regenerates an instrument while
preserving consumer source. Browser checks verify all seven accessible icons and
selection handling. Complete workspace doctests and warnings-denied Rustdoc
builds pass, as does the documentation checker. Linux aarch64 Rust cross-check
passes; Windows/Linux native picker runtime validation remains pending. See [Foundry](foundry.md) and
[decision 0194](decisions/0194-foundry-project-management.md). Evidence logs are
`target/foundry-directory-tests.log`, `target/foundry-directory-clippy.log`,
`target/foundry-directory-ui.log`, `target/foundry-directory-linux.log`,
`target/foundry-directory-doctests.log` and `target/foundry-directory-docs.log`.

## RAVET Foundry project manager (10 October 2026)

Implemented the shared Rust engine/CLI and RAVET controls/theme GUI, including
an accent-coloured SVG header and desktop sidebar that stays in place while forms
scroll. Foundry creates external independent workspaces, saves/reopens versioned
TOML, rejects changed managed metadata before regeneration, preserves consumer
DSP/editor/native source and Cargo.lock, and runs explicit background builds with
bounded logs. GUI and CLI expose offline, application and effect/instrument
plug-in starters. Effect exports are CLAP/VST3/AUv2/AUv3/AAX/LV2; instruments expose
CLAP/VST3/AUv2/LV2. SDKs stay external. Native identifiers derive from saved product
identity; export selections and product metadata can change without recopying
source. Platform-build instructions cover macOS Rust, Windows x64 MSVC/WebView2
and Linux GTK3/WebKitGTK4.1/X11 launchers. See [Foundry](foundry.md) and
[decision 0194](decisions/0194-foundry-project-management.md).

Local macOS arm64 evidence: 10 project/controller tests, warnings-denied Foundry
Clippy, debug build, complete workspace doctests and warnings-denied workspace
Rustdoc pass using isolated target directories. The external-consumer workflow
creates/reopens/regenerates/builds all four starters, runs the offline gain fixture,
the standalone four-mode hidden UI/audio/settings fixture and packages both CLAP
roles. The native Foundry hidden WebKit fixture verifies logo/controls, instrument
creation, regeneration and source preservation. A copied standalone test knob was
attached to its owner document to match the new connected-control contract.

Generated native effect LV2/VST3/AUv2/AUv3/AAX and instrument LV2/VST3/AUv2 debug
builds pass with local external SDKs. The official VST3 validator passes 47/47
for each role; actual generated AU components pass the existing gain/performance
local audio/state/lifecycle probes. The generated AAX bundle loads and exposes
its ACF functions and SDK version query. These are local development checks,
not DAW compatibility, AUv3 registration or production AAX/PACE evidence.
Linux aarch64 Rust engine/CLI cross-check passes; Windows/Linux GUI and generated
native plug-in runtime validation remain pending. No MPE/component/module/platform
selector, instrument AAX/AUv3 adapter, universal build, global installation or
production release is claimed. Regeneration is atomic per file rather than as a
group; cancellation owns the direct build child, not arbitrary descendants.

Validation logs: `target/foundry-tests.log`, `target/foundry-clippy.log`,
`target/foundry-consumers.log`, `target/foundry-ui.log`,
`target/foundry-workspace-doctests.log`, `target/foundry-workspace-docs.log`,
`target/foundry-linux.log`, `target/foundry-<effect|instrument>-<format>.log`,
`target/foundry-<effect|instrument>-vst3-validator.log`,
`target/foundry-<effect|instrument>-au-contracts.log` and
`target/foundry-effect-aax-check.log`. The complete reproducible SDK-enabled external workflow passes, including
version regeneration, native packages, signature/metadata checks and both
47-test VST3 validator runs (`target/foundry-native-workflow.log`). It accepts
optional SDK paths, AUv3 packaging and an official VST3 validator.

## Explicit sample-wise waveshapers

Added `ravet-signal::Waveshaper<F>` with inline caller-owned f32/f64 functions,
once-only sample calls, in-place processing and atomically checked separate
buffers. Built-in f64 constructors supply hard clip, tanh and cubic soft clip;
cubic endpoints are +/-2/3 with unity small-signal slope. Drive, gain, smoothing,
DC removal and oversampling remain explicit. The wrapper has no sample history,
latency/tail or reset requirement; custom captured state/resources belong to
callers. See [decision 0149](decisions/0149-sample-wise-waveshapers.md).

On macOS arm64, all 98 signal tests/doctests pass, including five new waveshaper groups for
curve landmarks, symmetry/monotonicity/extreme values, native precision/captured
controls, ordered calls, sample/block/partition identity, atomic length rejection,
explicit f32 oversampling composition and 10000 workloads without counted heap
actions. Signal all-target Clippy with warnings denied, workspace doctests and
workspace Rustdoc with warnings denied pass on the current checkout using a
separate Cargo target directory. The complete 574-test workspace suite also passes
in an isolated snapshot that excludes the two concurrently introduced embedded
workspace members and uses an offline-generated lockfile. The shared workspace
was incomplete at validation startup; its shared build target later reported
missing exports despite their presence in source, while the separate target
passed. Waveshaper source formatting and whitespace checks pass. Workspace
formatting reports unrelated in-progress embedded/foundation edits.

Documentation generation/checking passes in the isolated snapshot. On the shared
checkout, `check-docs.py --write` and `check-docs.py` report missing feature source
coverage for the concurrent embedded packages. The catalogue is refreshed from
the current registry/manifests with the checker's existing renderer; full package
coverage validation remains pending those registrations. This is portable offline
evidence without new host/device, platform support, alias-free or callback-deadline
claims.

## Implemented

| Work | Status | Paths and evidence |
|---|---|---|
| W01 — baseline | Integrated | README, decision record, this status, local Git |
| W02 — portable contract | Integrated, provisional | `crates/ravet-foundation`; configuration, borrowed buffers, events, transport, lifecycle; contract fixtures including a no-input test instrument |
| W03 — gain and smoothing | Integrated for M1 | `crates/ravet-signal` linear smoother; reference gain processor in `examples/gain`; exact ramps, retarget/reset, sample-rate conversion, event timing, stereo storage and partition tests |
| W04 — offline and heap instrumentation | Integrated for M1 and extended to CLAP | Offline fixtures, bridge and host ABI heap tests, concurrent worker isolation |
| W05 — parameters, state and meters | Portable APIs integrated; CLAP prototype integrated | Validated schemas/migration, version-two gain/bypass recall with legacy loading, sample-timed bypass/meters, committed-edit gestures and callback heap tests; decisions 0010/0012 |
| W07 — embedded editor | macOS prototype integrated | `crates/ravet-webview`, gain CLAP GUI/inline HTML, `tools/editor-smoke`; Cargo-built WebKit binding, visible gain/bypass/meters, live drag gestures and native lifecycle evidence; decisions 0013/0015 |
| W06 — CLAP feasibility | Prototype integrated | `crates/ravet-clap`, `examples/gain-clap`, `tools/clap-smoke`, `scripts/bundle-clap.py`; built macOS bundle, named development host, official validator |
| Documentation foundation | Integrated locally; CI configured | `docs/README.md`, guides, `docs/features.toml`, generated catalogue, `scripts/check-docs.py`, contributor/agent instructions, PR template, and documentation workflow |
| Small feature examples | Portable DSP integrated; live UI prototype | `examples/tutorials`; eight-voice shared-envelope subtractive synth, live gain/MPE, MIDI 2.0 inspector, knobs/piano keyboard and default standalone I/O popup; explicit offline fixtures retained; [guide](examples.md) |

The completed packages retain `publish = false` and inherit `GPL-3.0-only`.
The portable crates and `ravet-clap` retain `unsafe_code = forbid`. The CLAP
export and smoke host use `deny`, with local allowances for the entry macro,
trusted dynamic loading, and raw ABI test fixtures. Clack dependencies are
confined to the adapter/export/host graph; portable runtime dependencies remain
local workspace crates.

## Validation

Executed on macOS 27.0.1 arm64 with rustc/Cargo 1.99.0:

- `cargo fmt --all -- --check`
- `cargo check --workspace --locked`
- `cargo test --workspace --locked`: 49 behavior/instrumentation tests and one documentation example.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passes in
  the isolated eight-package state snapshot. The shared checkout currently
  fails on `clippy::derivable_impls` in the concurrently developed
  `examples/shine/src/lib.rs`; those files were left intact.
- `cargo doc --workspace --no-deps --locked` with rustdoc warnings denied.
- `cargo run -p ravet-gain --locked`: stereo fixture matches known output.
- `cargo tree -p ravet --edges normal`: portable dependencies remain local.
- `python3 scripts/bundle-clap.py`: optimized, ad hoc signed macOS CLAP bundle.
- `cargo run -p ravet-clap-smoke --locked -- "target/clap/release/RAVET Gain.clap"`:
  18 checked blocks across mono/stereo and six activation cycles on a separate
  audio thread; parameter text, inactive/active flush, automation, reset, and
  session save/restore while active.
- CLAP validator 0.4.1: 34 passes, 10 skips, zero failures. Final JSON evidence
  is in `target/clap/release/validation.json`. All five state checks pass.
  Skips include preset discovery, note ports, f64, and optional port activation/configuration.

The heap workload uses 48 kHz stereo, block sizes 0/1/3/64, two gain events per
block, default 5 ms smoothing, and periodic reset. Each in-place/separate mode
runs 10,000 callbacks and checks zero allocation count and zero net allocation
count/bytes (detecting frees of previously allocated storage). Fault/inactive
paths are also measured. CLAP tests additionally measure 10,000 callbacks
through the host ABI (including active flush, reset, and malformed events), and
10,000 in-place bridge callbacks, with no allocation or deallocation. Separate
fixtures verify legal host aliasing, zero frames, output tails, event overflow
before host event access, truncated payloads, invalid layouts, and f64 rejection.
State tests also measure 1,000 restore-consuming callbacks through process,
active flush, and reset with zero allocation/deallocation. A deterministic host
fixture restores midway through a callback and verifies that the new state wins
and is applied on the next block. Portable state fixtures cover version-one
compatibility, malformed records, bounded reads, short I/O, and stream failures.
Deliberate allocate/free probes verify the counter;
a separate worker allocates concurrently without affecting callback counts.

Development-only dependency: `allocation-counter` pinned to 0.8.1, declared
MIT/Apache-2.0 in its distributed manifest, with no further dependencies.
Its [documentation](https://docs.rs/allocation-counter/0.8.1/allocation_counter/)
and downloaded allocator source were inspected for thread-local counting and
net deallocation semantics. It replaces the global allocator in the test
binary that imports it. Native malloc and host/FFI allocations are outside this
counter's evidence. The owner has selected GPLv3 plus commercial licensing; see
[LICENSING.md](../LICENSING.md) for the GPL route and pending commercial terms.

## Concurrent workspace changes

A separate change began adding `examples/shine` during state validation. Its
initial missing build target temporarily prevented workspace Cargo commands, so
an isolated eight-package snapshot first passed tests, Clippy, and Rustdoc.
After the target became available, shared-checkout workspace tests passed too:
49 behavior tests and one Rustdoc example, including four concurrently added
delay/oversampling tests. These checks do not establish Shine product behavior;
its implementation and feature evidence remain owned by that separate change.

The proprietary consumer has now moved to a separate private sibling project,
with its own workspace and product validation. It is no longer a RAVET workspace
member. Reusable delay, feedback diffusion, and fixed 2x conversion remain in
`ravet-signal`; their generic tests are public framework evidence. Product
source, assets, fixtures, wrappers, and licence tests stay outside this checkout.
The original private commit still contains an early scaffold; the publication
path checker is expected to fail until history cleanup before public release.

## Documentation foundation validation

Executed locally on 8 October 2026 with Python 3.14.7 and Rust/Cargo 1.99.0:

- `python3 scripts/check-docs.py`: feature metadata, source/test references,
  coverage of all eight workspace packages, generated output, and local links pass.
- `python3 -m unittest discover -s scripts/tests -v`: 12 checker regression
  tests pass, including stale output, deleted evidence, missing package coverage,
  invalid registry records, and Markdown link handling.
- `cargo test --workspace --doc --locked`: the executable gain API example passes.
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked -p ravet`:
  all eight package references build.
- Documentation workflow YAML parses with both expected jobs. GitHub execution
  and artifact upload remain pending; there is no remote configured.

## Historical remaining work — initial M1 snapshot

M1 is implemented for one f32 main bus and input events. The test instrument
only exercises the contract; it is not a synth product. There is no output
event writer, auxiliary bus or device adapter yet. Reusable parameter attachments, portable
parameter schemas, complete state/migration and gain bypass/metering are now
implemented. CLAP exposes gain/bypass recall and a committed-edit gesture/meter
bridge. The macOS CLAP prototype now embeds gain/bypass controls and input/output
meters through an explicit Rust/WebKit binding. A general Rust widget API remains pending; reusable metadata/control attachments
are implemented separately. Product-specific
gain CLAP session state is a complete gain/bypass prototype that reads legacy
scalar state; the portable parameter schema and explicit migration layer are
implemented separately. The CLAP prototype exposes ports and automation, with its
original adapter limits recorded in decision 0002 and new state behavior in
the [state guide](session-state.md).

Callback duration distributions/deadline misses have not been measured. REAPER
7.82 on macOS arm64 has now exercised the gain CLAP paths recorded below. Other
DAWs, devices and production compatibility remain unverified. Native Linux
evidence and Windows compile evidence retain the limits recorded separately.
Validator and native smoke fixtures remain regression gates.

Next bounded work packages:

1. W07 follow-up: broader widget APIs and editor platforms;
   retain native lifecycle checks and record actual host-loop requirements.
2. W06 follow-up: broaden the single-REAPER validation to additional host versions
   and platforms; retain the validator/ABI and synthetic render checks.
3. W05/W07 follow-up: further editor backends and attachment integrations.
   Portable schemas, complete CLAP gain/bypass recall, committed-edit host gestures,
   coalesced meter reads and macOS gain GUI/native binding are implemented.
4. W08: separate standalone device adapter; use the same gain processor.

Keep format/window backend choices provisional until these experiments provide
host evidence. The documentation workflow is configured; add runtime CI/platform
matrices when supported targets are exercised.


## Licensing decision — 8 October 2026

The owner selected GPLv3 plus separate commercial licensing. Root LICENSE
contains GPLv3 and all current workspace packages inherit GPL-3.0-only.
The owner also accepted launch pricing: Starter free up to $50K company revenue,
Indie $199/developer/year above $50K through $500K, and Pro $699/developer/year
above $500K with no upper limit. Prices are USD excluding applicable taxes.
Company revenue is assessed over the previous twelve months at purchase or
renewal. Paid licences include twelve months of updates and permanent rights to
build and distribute with versions received. All tiers include all supported
features, unlimited products, and no royalties; CI/build machines carry no fees.
Launch billing is annual, with paid licensing following a free preview when
production plug-ins, editors, and documentation are reliable. Community support
covers all tiers; paid customers receive a direct reporting channel, with
engineering support and contractual response times priced separately.
The final commercial agreement, detailed revenue/seat definitions, Starter
retained rights, support operation, and contributor terms remain open.
This decision does not change the implementation status above or publish any
package. See [decision 0006](decisions/0006-distribution-policy.md).


## Commercial agreement draft — 8 October 2026

Prepared draft 0.1 at the owner's request; it is now kept privately.
It includes the accepted prices and permanent version rights, an Order Form,
and proposed details for seats, Starter registration, group revenue,
third-party scope, support, verification, liability, and breach termination.
The draft separates accepted business policy from proposed legal defaults.
It awaits owner and legal review and is not an executed commercial grant.
No runtime code, Cargo licence metadata, feature status, or publication
setting changed. Documentation links and catalogue freshness were checked with
`python3 scripts/check-docs.py`; this is documentation validation, not legal
validation.

## Reusable private-consumer components — 8 October 2026

Added prepared double-precision delay/feedback diffusion and fixed 2x half-band
conversion to `ravet-signal`; added explicit control-thread XML/hash/legacy integer
verification in `ravet-data`; added a bounded coherent word mailbox to foundation.
A separately built Cocoa/WebKit editor backend now has a native main-loop probe.
These are framework components. Proprietary DSP, assets, identities, SDK wrappers,
state/licence fixtures, and comparison tools remain in a separate private consumer.

Executed locally on macOS arm64 with Rust/Cargo 1.99.0 and Xcode 27.0:

- Workspace tests, two Rustdoc examples, and API documentation with warnings denied
  pass after adding data services and the mailbox.
- Generic data tests cover bounded/malformed XML, Unicode/entity roundtrip,
  SHA-256, and signature byte ordering. Mailbox tests cover a newer restore winning
  over stale audio reflection and 20,000 successful concurrent publications
  without mixed word snapshots.
- The native WebKit probe passes three create/message/snapshot/navigation/
  detach/reattach/destroy cycles and verifies callbacks stop after destruction.
  It does not measure native heap behavior or callback deadlines.
- A follow-up native probe passes with opaque layer backing, configurable
  background colour, and suppression of identical snapshots. Delivery now allows
  one JavaScript evaluation in flight. Host repaint stability remains subject to
  consumer testing; these checks do not establish DAW support.
- The publication checker now examines commit path changes so a rename of an
  identical blob cannot hide a private historical path. Four disposable Git
  fixture tests pass; all 16 Python checker tests pass together.
- The public publication checker deliberately fails on the earlier private
  scaffold in reachable history. The GitHub remote was verified private; no
  public visibility change or history rewrite was performed.

The editor's Rust binding, gain integration, broader platform/deployment support,
DAW embedding, and deadline measurements remain pending. Private consumer host
validation is recorded outside this repository and does not establish public
framework format support.

## Delay processing optimization — 8 October 2026

Removed floating remainder from bounded fractional-delay cursor wrapping and
integer division from cursor advancement. Added `read_integer` for whole-sample
distances, and used it inside prepared feedback diffusion. Generic tests compare
both read paths across repeated wraps of lengths 3, 5, and 17, finite clamping,
nonfinite fractional distances, and oversized integer distances. These changes
retain the prepared storage and allocation-free lifecycle. Product CPU
benchmarks and sound comparisons remain private consumer evidence; this record
does not establish a callback deadline or a framework-wide performance ratio.

Added a prepared `PhaseRotation` arithmetic kernel for caller-owned quadrature
banks. Local tests exercise 16 phases over 8,192 steps with positive, negative,
zero, and near-Nyquist increments and reanchoring every 128 steps. Signal
unit tests include dense fractional-delay interpolation across arbitrary-length
wraps and integer/fractional read equivalence. Consumers must establish
their own numerical error budget and reanchor cadence. Oversampling entry points
now expose inline bodies to consumer compilers; their stream semantics remain
unchanged.

Added `AudioBuffer::map_stereo_frames` to select storage once and map safe active
prefixes with zero-filled missing inputs. Three tests cover in-place read-before-
write swaps, separate instrument/mono/stereo inputs, unequal capacities, zero
frames, and atomic rejection of unsupported shapes. See
[decision 0011](decisions/0011-stereo-frame-mapping.md). This helper retains the
processor's existing preparation and event-validation obligations.

Final local framework verification passes workspace tests (including Rustdoc),
Clippy with warnings denied, warning-free API documentation, formatting, and
documentation consistency. Quadrature rotation and diffusion use explicit FMA
on AArch64/FMA targets and separate operations elsewhere; guides describe the
target-dependent double rounding. Consumer-specific sound and CPU acceptance
results remain recorded privately rather than implying a framework-wide
performance or host support guarantee.


## Portable parameters, state, bypass and meters — 8 October 2026

Completed the first incremental W05 package: foundation now provides borrowed
validated parameter metadata, linear/logarithmic and discrete value mappings,
fixed-size complete records and explicit stable-ID migration. Gain adds a complete
version-two gain/bypass record that migrates legacy version one with bypass off.
Restore validates both values before mutation and snaps smoothing. Portable
bypass switches at sample offsets while smoothing continues. Signal provides
sample peak/RMS windows; gain records input/output per successful nonempty block
and offers a separate one-attempt coherent 20-word telemetry mailbox.
See [the guide](parameters.md) and [decision 0010](decisions/0010-parameter-state-and-meters.md).

Executed locally on macOS arm64 with Rust/Cargo 1.99.0:

- Workspace tests pass, including existing CLAP ABI/state/allocator fixtures.
  Thirteen new parameter, migration, bypass and meter tests cover malformed
  records, extreme mappings, same-frame event order, block partitioning,
  stereo/tails, empty/fault behavior, nonfinite audio and legacy recall.
- 1,000 complete encode/decode/restore, bypass, meter publish/read and reset
  cycles report zero Rust allocation/deallocation; existing 10,000-callback
  gain and host workloads also pass with automatic block metering enabled.
- Workspace check, formatting and Clippy with warnings denied pass. Both
  existing Rustdoc examples pass; workspace API docs build with warnings denied.
- Documentation generation/checks and all 16 Python checker tests pass.
- Rebuilt ad hoc signed optimized CLAP bundle passes the smoke host's 18 blocks
  and six activation cycles. CLAP validator reports 34 passes, 10 skips,
  zero failures and zero warnings.

The CLAP export still advertises gain alone and uses its original scalar state
protocol. New portable bypass metadata, complete restore, host gestures and meter
UI integration remain follow-up work. Bypass has no crossfade; meters coalesce
blocks without retaining intermediate peaks. No device, DAW, native allocation
or real-time deadline evidence is claimed. Existing unrelated workspace work
was preserved; no dependencies, commits or publication settings were added.


## CLAP complete state, bypass and editor control — 8 October 2026

Completed the next W05 increment. CLAP now advertises gain and stepped bypass
(including the standard bypass flag), parameter text conversion and sample-timed
bypass automation. Save emits complete version-two state; load accepts both
versions and migrates legacy records with bypass off. A single atomic word holds
both values and a 31-bit restore generation. Restore still wins over stale
callback reflection. The product main-thread control API queues 32 committed
edits and emits complete host gestures with bounded backpressure handling.
Successful callbacks publish coalesced input/output meters for Rust editor code.

Executed locally on macOS arm64 with Rust/Cargo 1.99.0:

- Workspace tests pass. New tests cover complete bypass recall and malformed
  complete records, legacy bypass-off migration, bounded partial stream I/O,
  edit validation/scheduling, queue fullness/wrapping, partial output resumption,
  stale gesture closure and synchronization with newer restores.
- 10,000 concurrently submitted/consumed edits preserve value order. This
  stress fixture may yield/retry on the test threads; production callback
  operations remain bounded without retry loops.
- 1,000 active committed-edit flush/reset cycles and 1,000 bypass processing/
  meter publication cycles show zero Rust heap allocation/deallocation. Existing
  host ABI probes also pass. A concurrent ABI restore now verifies gain and
  bypass both win over stale callback automation and reach the next block.
- Workspace check, formatting and Clippy with warnings denied pass. Both Rustdoc
  examples pass and all workspace API documentation builds with warnings denied.
- All 16 Python checker tests and regenerated documentation checks pass.
- The rebuilt optimized ad hoc signed macOS CLAP bundle passes 18 smoke blocks
  and six activation cycles, now checking two-parameter metadata and text.
  Validator reports 34 passes, 10 skips, zero failures and zero warnings.
  Text evidence is in `target/clap/release/validation-complete-state.log`.

[Decision 0012](decisions/0012-clap-complete-state-and-edits.md) records state and
edit ordering. [The control guide](editor-control.md) describes overflow and
host output backpressure. This is a Rust product control bridge, not an embedded
GUI or general attachment system. Native Rust/WebKit binding, visible controls,
continuous drag gestures and DAW embedding remain the next work. No dependency,
commit or publication changes were made, and unrelated workspace work remains.


## Cargo native gain editor — 8 October 2026

Completed the first W07 integration: the default macOS gain CLAP build now
provides a fixed 480 × 390 embedded Cocoa editor with gain and bypass controls,
input/output stereo peak/RMS readings, and visible edit errors/host-wait status.
Slider changes commit on release through the existing bounded host gesture queue.
Host automation and state recall use coherent parameter snapshots; audio meters
remain coalesced. The HTML is compiled into the bundle with no external assets.

Added the explicit `ravet-webview` Cargo package for the existing Cocoa/WebKit
backend, main-thread ownership and teardown, bounded snapshot copying, panic
containment and reentrancy guards. Native attachment is an isolated unsafe
boundary. The portable umbrella dependency graph remains local and excludes
WebKit/JSON. The optional default `native-editor` feature can be disabled.
Native compilation requires Xcode tools; macOS 12 is a build deployment target,
not a verified minimum supported OS. Runtime JSON dependencies and their licences
are recorded in [the native editor guide](native-editor.md); ownership and scope
are recorded in [decision 0013](decisions/0013-cargo-native-gain-editor.md).

Executed locally on macOS 27.0.1 arm64, Rust/Cargo 1.99.0 and Xcode 27.0:

- Workspace tests pass, including existing audio/host Rust heap probes and three
  binding tests for wrong-thread creation, bounded/reentrant snapshot handling,
  and copying before callback context destruction. Editor protocol tests reject
  malformed, unknown-shape and nonfinite messages.
- Formatting, workspace Clippy with warnings denied, both Rustdoc examples, and
  workspace API documentation with warnings denied pass. The gain CLAP export
  also checks with `--no-default-features`.
- Regenerated catalogue, documentation checks and all 16 Python checker tests
  pass; the registry covers all eleven workspace packages.
- The rebuilt optimized ad hoc signed CLAP bundle passes 18 audio smoke blocks
  across six activation cycles. Validator reports 34 passes, 10 skips, zero
  failures and zero warnings; evidence is in
  `target/clap/release/validation-native-editor.log`.
- The hidden Cocoa editor host passes three create/attach/show/hide/destroy
  cycles, verifies DOM gain/bypass edits and six emitted gesture events, visible
  invalid-parameter errors, host automation, live audio meter delivery and audio
  processing after GUI destruction. Expected show-before-parent failures are
  checked. The captured `target/editor/gain-editor.png` was visually inspected
  without clipping.
- The older file-backed native lifecycle fixture timed out once during concurrent
  WebKit startup. Two sequential reruns passed; after the final native ownership
  change, rebuilding and running CTest sequentially passed again (3.47 seconds).
  That fixture still has its original four-second startup window.

This is a fixed embedded macOS prototype. Floating/resizable windows, continuous
drag gestures, general parameter attachments, other native platforms and DAW
embedding remain pending. Native heap behavior and real-time deadlines have not
been measured. Meter displays have no peak retention or decay, and bypass has no
crossfade. No commits or publication changes were made; unrelated shared-checkout
work was preserved.


## Windows and Linux native C++ editor prototypes

Added WebView2/HWND and GTK3 X11/WebKitGTK 4.1 backends behind the existing
native C++ API, with finite backing colours, local/inline pages, bounded messages,
coalesced snapshots, explicit teardown and Linux UI-loop pumping. The portable
Rust dependency graph and macOS-only Cargo webview API are unchanged. Consumer
products and format wrappers remain private siblings. See
[decision 0014](decisions/0014-cross-platform-native-web.md).

Validation on 8 October 2026 used local Ubuntu 24.04 containers in Colima on the
macOS arm64 development machine: native Linux arm64 and QEMU Linux x64, GCC 13,
CMake 3.28, GTK3 and WebKitGTK 4.1. Both lifecycle probes passed three cycles,
malformed snapshot rejection, unchanged suppression, changed updates, navigation
restriction, reattachment, callback-triggered destruction and stopped callbacks.
The x64 probe took about 18 seconds under emulation; it is not a performance
measurement. Root-container probes alone disabled the WebKit sandbox for their
isolated process. Production backends retain its default.

The Windows x64 backend and lifecycle executable cross-compiled using MinGW
GCC 13 and Microsoft WebView2 SDK 1.0.3650.58. Windows graphical execution is
pending; no Windows editor, DAW, deadline or native heap support is
claimed. The Windows runtime must be installed separately. Linux consumers need
X11/XWayland and an explicit host UI pump; native Wayland parenting is excluded
from this prototype. The Rust wrapper/gain GUI still expose macOS only.

The same generic x64 lifecycle executable also passed on a native Omarchy
machine under Hyprland/XWayland, with WebKitGTK 4.1 2.52.6 and JSON-GLib 1.10.8.
It ran as the desktop user with the default WebKit sandbox, covering all three
create/message/snapshot/coalescing/navigation/reattach/destroy cycles. WebKit
emitted GBM buffer warnings during this probe; visible rendering and embedding
in a DAW remain separate pending checks. This is native backend evidence, not
an Omarchy host-support or CPU claim.


## Continuous editor gestures — 8 October 2026

Extended W05/W07 with `GainMain` begin/update/end methods and live macOS gain
slider input. Pointer movement now publishes accepted values before release
within one host gesture. Numeric/bypass edits remain complete transactions.
Only one continuous gesture is open at a time; conflicting begins/complete edits
reject. Commands use the existing fixed 32-slot queue and 96-iteration callback
budget. Begin/value commands reserve a slot for end; rejected values are visible
and never overwrite an older queued update. Restore closes an idle old gesture
on the next scheduled callback and discards remaining values from that drag.
Editor destruction queues end and requests host scheduling. See
[decision 0015](decisions/0015-continuous-editor-gestures.md) and the updated
[control guide](editor-control.md).

Executed locally on macOS 27.0.1 arm64, Rust/Cargo 1.99.0 and Xcode 27.0:

- Workspace tests pass, including new cross-callback live gesture, one-event
  backpressure, reserved end under full updates, and idle-restore cancellation
  fixtures. Public main-thread methods check validation, conflicts, canonical
  clamping, scheduling and values applied before end.
- 1,000 live queue gestures and 1,000 active-flush gesture cycles report zero
  Rust heap allocation/deallocation. Existing ABI/process/meter probes pass.
- Formatting, workspace Clippy with warnings denied, both Rustdoc examples,
  warning-free workspace API documentation and a no-default-features export check
  pass. Regenerated catalogue/docs and all 16 Python checker tests pass.
- The rebuilt optimized ad hoc signed bundle passes all three Cocoa editor
  cycles with a value before release, a second value/end in a later flush, and
  exactly one begin/end. Destroying an editor during a new open drag queues end;
  the subsequent active audio callback receives begin/end and processes correctly.
  The fixture's expected show-before-parent error messages remain intentional.
- The built bundle passes 18 audio smoke blocks and six activation cycles. CLAP
  validator reports 34 passes, 10 skips, zero failures and zero warnings; text
  evidence is in `target/clap/release/validation-continuous-gestures.log`.

Delivery depends on host scheduling/output acceptance. Overflow can reject the
final slider value; end remains enqueueable but no bounded delivery time is
guaranteed. DAW embedding, native allocation and callback deadlines remain
unverified. Resizing and reusable framework attachments remain follow-up work.
No dependencies, commits or publication settings changed. Unrelated platform
backend work in the shared checkout was preserved.


## Parameter attachments and editor resizing — 8 October 2026

Implemented reusable `ravet-surface` attachments with borrowed schema metadata,
stable IDs, physical/normalized linear/logarithmic/discrete edits, explicit
commit/begin/update/end transport and host refresh without feedback. Failed
transport operations preserve ownership; submitted values remain separate from
displayed host snapshots. `GainMain` implements the transport trait and the
macOS page callbacks now own gain/bypass attachments. Native teardown explicitly
ends an active attachment. No native/host/JSON dependency entered the portable
surface crate; its only runtime dependency is local foundation metadata.

The gain editor now accepts both-axis resizing from 480×390 through 1200×900
logical Cocoa pixels with no fixed aspect ratio. The view and WebKit child resize
together and the page fills its viewport. Invalid dimensions reject before
mutation; native owned-view bounds are finite positive dimensions through 16384.
This new native resize entry point is currently macOS-only. See
[attachments](parameter-attachments.md) and
[decision 0016](decisions/0016-attachments-and-resizing.md).

Executed locally on macOS 27.0.1 arm64, Rust/Cargo 1.99.0 and Xcode 27.0:

- Workspace tests and Clippy with warnings denied pass. Three Rustdoc examples
  pass, including the new attachment refresh example. Workspace API documentation
  builds with warnings denied; the editor-free gain export checks too.
- New attachment tests cover mapping, strict nonfinite rejection, refresh without
  feedback, accepted submit versus displayed state, conflicts and rejected
  begin/update/end ownership. Existing audio/host heap fixtures pass after GUI
  attachment integration.
- The rebuilt signed bundle passes three native editor cycles at minimum,
  intermediate (640×520) and maximum sizes. Host extension sizes and JavaScript
  viewport dimensions agree with no scroll overflow. Invalid/oversized requests
  preserve the prior size; oversized adjustment caps to the maximum.
- The C++ lifecycle fixture additionally verifies nonfinite/zero/oversized resize
  rejection and matching container/child dimensions after valid resizing. CTest
  passes after these checks (3.15 seconds).
- Native screenshot `target/editor/gain-resizable.png` was inspected without
  clipping. Actual DAW visual behavior requires separate evidence.
- The built bundle passes 18 audio smoke blocks and six activations. Validator
  reports 34 passes, 10 skips, zero failures and zero warnings; evidence is in
  `target/clap/release/validation-attachments-resize.log`.
- All 16 Python documentation/publication checker tests pass, as do regenerated
  catalogue/reference checks. The portable umbrella tree remains local crates.

Prepared a separate official REAPER 7.82 evaluation copy and isolated resource
directory under `target/reaper-validation` at the owner's request. Its CLAP scan
cache discovers `org.ravet.gain`. Native UI automation failed with a closed pipe
and a subsequent timeout; startup dialog handling currently requires the owner.
The public ReaScript session/automation harness and synthetic render checker are
ready in `tools/reaper-validation`; two checker fixtures pass, including rejection
of incorrect audio. These fixtures do not constitute an actual DAW render or
session pass. Actual REAPER execution/results remain pending and are tracked
separately from the completed implementation. See [host validation](host-validation.md).


## REAPER host validation — 8 October 2026

Completed the remaining single-host validation package using official REAPER
7.82/macOS-arm64 on macOS 27.0.1, the optimized ad hoc signed gain CLAP bundle
and the disposable configuration under `target/reaper-validation`. Native UI
control recovered after the owner cleared startup dialogs. The normal REAPER
resource directory and production sessions were not used.

- The ReaScript harness passes CLAP discovery/instantiation, gain and bypass
  metadata/host edits, editor creation/reopening, complete state recall from two
  saved projects and gain automation playback at 0.5 and 2.0. Live track peaks
  match 0.125/0.25 and 0.5/1.0 respectively (about 1.2e-7 maximum peak error).
  Evidence: `target/reaper-validation/config/ravet-validation/host-validation.log`.
- REAPER rendered four-second 48 kHz stereo 24-bit PCM track stems. The render
  checker passes wet gain 1.5, dry bypass and both stepped automation regions.
  Every checked sample matches the expected amplitudes exactly; source fades and
  smoother settling regions are excluded. Evidence: generated `wet.wav`,
  `dry.wav`, `automation.wav` and `render-check.log` in that fixture directory.
- Native UI inspection showed input peak/RMS −12.0/−6.0 dB and wet output
  −8.5/−2.5 dB at gain 1.5. Numeric gain 2.5 and an actual slider drag to about
  3.308 reached REAPER and changed output meters. Plug-in bypass restored dry
  peaks/meters. Write-mode recording produced automation points containing the
  changed GUI gain, verified by the observer and saved in `ui-recording.rpp`.
  Evidence: `ui-validation.log`; this checks recorded values, not every possible
  gesture mode or envelope interpolation.
- REAPER's FX-chain window zoom expanded the embedded page to its capped maximum
  with all controls/meters visible; restoring its window returned the smaller
  layout. Audio/meters and controls continued during resizing. The floating
  wrapper's corner drag did not change its size in this experiment, so that
  interaction is not claimed as validated. Exact minimum/intermediate/maximum
  dimensions remain covered by the independent CLAP native resize fixture.

Harness corrections made during actual execution: match stable CLAP identifiers
`0:1`/`1:2` instead of selecting REAPER's additional bypass wrapper, and seek
playback with `SetEditCurPos(...,true)` rather than moving only the edit cursor.
An initial live-peak attempt failed; a fresh project rerun passed with numerical
peak logging, so this does not establish uninterrupted device behavior. Initial
automatic renders selected the muted Master mix and were silent. Explicitly
applying Selected tracks (stems) in the render dialog produced the passing audio;
the guide records this requirement and the exploratory automatic renderer was
removed from the public harness. The verified wet render was retained from the
first successful stem render after a later exploratory run replaced the working
copy with silence. No plug-in processing change was needed for these harness
issues. Startup scan and evaluation handling remain host-owned.

This completes reusable attachments, bounded macOS resizing and the requested
REAPER validation milestone. It does not establish other DAWs/versions, release
signing, native heap behavior, callback deadlines or general production support.
Broader host/device/platform work remains listed separately above. REAPER's main
test window is left open with playback stopped.

Final checks pass: regenerated catalogue/local references, all 16 documentation
and publication checker tests, both render-checker fixtures, formatting, three
workspace Rustdoc examples and workspace API documentation with warnings denied.


## Cooperative Linux host GTK initialization — 8 October 2026

The native Linux web backend now asks a loaded SWELL provider to initialize
GTK through its LOAD_GTK extension before creating widgets. Both globally
exported host APIs and locally loaded libSwell.so providers are discovered;
no new host library is loaded. An unavailable capability returns a null editor
rather than replacing the host's process-wide GDK event dispatcher.

Native Omarchy/XWayland checks establish the integration boundary: a GDK host
compatibility shim continues receiving events before/after view creation and
destruction, while the unsupported-loader case declines creation and preserves
events. Replacing the cooperative call with the previous direct gtk_init_check
reproduces lost host events in the same test. The existing standalone native
create/message/snapshot/coalescing/navigation/reattach/destroy probe also passes.
The real SWELL libraries from REAPER 7.79 and 7.82 respectively decline and
accept editor creation in a separate native probe. These are toolkit-library
checks, not a successful REAPER session/editor/audio audition. The actual DAW
retry is pending. See decision 0014 and the native editor guide.


## Linux editor viewport and painted-pixel checks — 8 October 2026

The GTK box now expands the web child to fill the requested editor instead of
assigning its one-pixel minimum height. The portable lifecycle fixture now
checks page viewport dimensions as well as message/state behavior. Its new
assertion fails with the previous layout and passes after explicit expansion.

Added RAVET_WEB_LINUX_SOFTWARE_RENDERING (framework CMake default OFF), selecting
WebKit's non-accelerated policy for these views before loading documents. It
does not modify other host views or sandbox settings. On native Omarchy with
NVIDIA RTX 2060 / WebKitGTK 2.52.6 / XWayland, the normal accelerated paint path
produces no expected pixels and emits GBM allocation errors. The software mode
produces the exact two-color 320x180 native GTK paint image without those errors.
The software native lifecycle/viewport probe passes too. Actual DAW painting
and UI CPU cost still need independent evidence. The new painted-pixel test
writes its checked PNG to the native test build directory.

CMake/CTest integration passes all four native web tests in the Ubuntu x64
container with software rendering selected; its root-only sandbox override was
confined to that test process. Native Omarchy probes use the normal sandbox.
Workspace Rustdoc tests, warnings-denied documentation build and documentation
catalogue/link checks pass.

## macOS activating mouse press — 8 October 2026

The Cocoa editor web view now accepts the activating mouse press. A native
two-window AppKit probe sends a drag to an inactive editor while another owned
window is key, checking page begin/change/end messages. The prior backend drops
all three phases; the corrected backend delivers all three. Both this probe
and the existing native lifecycle probe pass on macOS 27.0.1 arm64. This
establishes the owned Cocoa-view behavior, not every DAW's window routing.
The policy is recorded in decision 0014 and the native editor guide.


## Extended processing and output devices (8 October 2026)

The owner expanded the implementation scope beyond earlier gain milestones.
The feature catalogue records scoped implementations and remaining integrations;
earlier gain milestones do not establish complete framework coverage.

W08 extends portable processing to f32/f64, up to 64 channels per bus direction,
eight ordered auxiliary buses, bounded emitted events and latency/tail metadata.
Gain/CLAP explicitly retain their narrower mono/stereo f32 subset. CLAP forwards
validated block-start transport and tracks seeks across accepted callbacks.
Foundation/gain tests and 11 bridge tests pass, including f64 precision, auxiliary
fault silencing, output-event prefix preservation and invalid transport rejection.
These transport changes have not yet been exercised in REAPER.

W09 adds explicit `ravet-device`/`ravet-device-probe` CPAL output-only prototypes.
Four adapter tests pass, including 1,000 eight-channel variable callbacks with
zero counted Rust allocation/deallocation, correct interleaving and sample
position, malformed frame rejection and fault silencing. On macOS 27.0.1 arm64,
the default UMC1820 output opened at 20 channels/96 kHz for a one-second silent
probe: 189 callbacks, 96,768 frames, zero processor faults/backend errors/panics.
This establishes scoped stream/callback execution, not audible output, latency,
real-time deadlines, duplex/MIDI support or any other platform. Cargo builds
are capped at two jobs. Input/duplex, MIDI, recording and device recovery remain
pending.

W10 adds portable validated MIDI 1.0 short messages, running-status decoding and
timed input/output event representation. Four MIDI tests pass for all channel
status families, pitch-bend resolution, zero-velocity note-off, interleaved
real-time messages, common-message cancellation and malformed/SysEx recovery.
SysEx payloads, MIDI devices, MPE and instrument exports remain pending.


W11 (9 October) adds six stable f64 biquad designs, prepared complex radix-two
FFT, direct f64 FIR convolution and selectable 1x/2x/4x/8x f32 oversampling.
Signal's 25 unit tests plus a prepared-kernel heap fixture pass. New tests compare
independent DFT/convolution/impulse recurrences, response landmarks, partitions,
reset and validation bounds. The shared 1,000-workload fixture reports zero
counted allocation/deallocation. Direct FIR has O(taps) cost; there is no long-IR
performance or deadline claim. Wider DSP work remains planned.

Local SDK inspection found Apple headers, VST3/AudioUnit/AAX resources outside
RAVET and installed PACE wraptool 5.10.4 at the owner's Eden location. These are
build prerequisites only, not completed wrappers, signing or host evidence.
No SDKs, signing credentials or proprietary consumer code entered this checkout.

W11 also adds validated sample-counted linear ADSR with current-level retrigger/
release, exact targets and zero-duration transitions. Its heap path is included
in the prepared-kernel fixture; synthesis voice routing remains pending.


Final extended-foundation/device/MIDI/DSP validation on 9 October: workspace
checks pass with 124 tests including three Rustdoc examples, Clippy all targets
with warnings denied, warning-denied API HTML, explicit workspace doctests,
formatting and documentation checks. The umbrella normal dependency tree remains
local portable crates; CPAL is confined to the explicit device graph. Rebuilt
macOS CLAP bundle passes 18 checked audio blocks/six activation cycles in the
smoke host; official validator reports 34 success, 10 skips, no failures. This
includes validator transport fuzz workloads but does not add REAPER transport
or broader DAW evidence. Logs are retained under the ignored target directory.

The final silent output probe was repeated on 9 October after callback containment
changes: UMC1820 20-channel/96 kHz, 189 callbacks/96,768 frames, no reported
processor faults/backend errors/panics. All 16 documentation/publication checker
regression tests also pass. The broader framework backlog remains incomplete.


Concurrent runtime work began after that successful 124-test snapshot, adding
`ravet-runtime` and `ravet-runtime-ffi`. A later shared-checkout documentation/
formatting check was temporarily blocked first by the absent FFI manifest, then
by its missing feature reference and a missing runtime HTTP test module. Those
in-progress files were preserved. Documentation for the completed device/MIDI/
DSP package snapshot passes in an isolated target directory; the latest
runtime additions are outside the 124-test evidence and need their own checks.

## Public documentation scope — 9 October 2026

External framework comparison notes and the comparison plan were preserved in
private sibling notes outside the checkout. Public guides, decision records,
the historical handoff and feature descriptions now state RAVET's capabilities
and limits directly. Original historical wording is preserved in those private
notes. Stable feature IDs, implementation statuses and recorded validation
outcomes are retained. Contributor guidance and filename ignore rules keep
future comparison plans outside the repository.

The regenerated catalogue, evidence/package coverage and local links pass the
documentation checker. All 16 documentation/publication checker regression
tests pass; the content and filename audit finds no removed framework references
among 186 existing tracked or non-ignored files. Git whitespace checks pass.

Existing Git history and local checkpoint refs still contain earlier references;
this documentation cleanup does not rewrite them. Publication still requires
the content and history review described in [publication](publication.md).

## Native reference adapters — 9 October 2026

W12 adds the reusable gain reference's Rust static ABI and external-SDK VST3
processor/controller. Two ABI tests include 10,000 prepared process/reset/restore
iterations with no Rust heap operations. Native contracts cover mono/stereo,
in-place/separate buffers, sample bypass, complete state/controller recall,
zero-frame flush, whole-stream rejection and tails. SDK 3.8.1 validator loads the
actual macOS arm64 bundle and reports 47 passes, zero failures. This is a gain
reference prototype; generic mappings, custom editor and actual DAW evidence
remain pending. No SDK or private consumer implementation was copied.

W13 adds an external-SDK AUv2 component embedding that VST3 artifact. Actual
AudioComponent in-process tests exercise 48 kHz stereo f32, normalized gain,
bypass, gain preset recall, preserved host bypass and backing tails. The SDK's
host bypass is intentionally separate from preset recall. The external Apple
library builds with Xcode 27 at deployment target 12.0; that minimum is not a
runtime compatibility claim. Local bundles are ad hoc signed and kept in target;
no global host installation, production signing, auval or DAW evidence is added.

W14 builds a native mono/stereo AAX reference with the external VST3/Avid SDKs.
The macOS arm64 artifact loads and exposes all eight inspected AAX/VST3 factory
entry points; its SDK query succeeds with word 0x20801345980. Underlying native
DSP/controller contracts pass. This adds no Pro Tools render/state/acceptance
evidence. Authorized PACE signing or a developer host is an external prerequisite;
no credentials or commercial signing actions were used.

W15 adds mono/stereo LV2 descriptors, block-level gain/bypass ports and Turtle
metadata. Actual packaged binary contracts pass for gain smoothing, bypass,
control recall, malformed controls, zero-frame runs, lifecycle, retained tails
and in-place aliases. External official LV2 1.18.10 headers are used only from
target; licence notices accompany artifacts. macOS arm64 only, no host metadata
validation, DAW session or Linux runtime claim.


## Reusable Rust runtime services — 9 October 2026

The owner clarified that replacing consumer-local JUCE code with reusable RAVET
APIs is the objective, with Rust used wherever possible and product authorization
kept private. Added safe `ravet-runtime` files/HTTPS/owned-workers/timers/user-path
APIs and separate `ravet-runtime-ffi` native ownership and narrow OS queries.
C++ facades contain no networking, storage or scheduling implementation. Private
consumer code now uses these APIs; endpoints, payloads, trial/licence rules,
machine hashing and product paths remain outside this repository.

Rust unit/integration/ABI tests pass on macOS 27.0.1 arm64 and Omarchy x64 with
Rust 1.98.1. HTTP tests use synthetic localhost TLS, including untrusted-certificate
rejection, status/body handling, redirect refusal, chunked bounds and deadline.
The standalone native facade links the Rust static library and passes in
C++17/C++23 on macOS and C++17 on Omarchy, exercising UTF-8/binary files, captured
input lifetime, rejected work, self-teardown and timer shutdown. The initial
native probe incorrectly captured a local future by reference beyond its scope;
correcting the fixture to own it resolves that test-only failure.

Added a generic Cocoa/Windows/GTK native file chooser, with no product import
policy. The Cocoa owned-window cancellation probe passes, including the caller's
extension filter. GTK source compiles with warnings denied on Omarchy; actual
GTK chooser and Windows compile/runtime evidence remain pending. An existing
first-mouse probe run failed to acquire its required test-window focus; an
isolated rerun passes. No focus-policy implementation was changed by this package.

Workspace tests, explicit doctests, warnings-denied Rustdoc and Clippy checks
pass. Catalogue/package/link checks and formatting pass. Generic widgets and
reusable format adapters are still separate work; this package is not complete
JUCE parity, a production DAW support claim or a live activation test. Contracts
and limits are in [runtime services](runtime-services.md) and decision 0020.

W16 adds explicit control-thread hierarchical state, atomic transactions, grouped
undo/redo, bounded polling subscriptions/resync and typed complete preset records.
Five integration tests plus the Rustdoc example pass, as does warning-denied
package Clippy. Tests include every truncated record prefix, independent invalid
records, transaction rollback, branching and journal/history limits. No GUI/host
or audio-thread state delivery claim follows from these control-thread tests.

The owner confirms an authorized PACE signing configuration exists, but keeps
its contents private because NDA disclosure scope is uncertain. Production
signing remains the product release workflow; framework implementation continues
independently. No protected signing configuration was requested for upload or
read, and the AAX artifact retains its development-only signature.

W17 adds f64 phase-continuous sine/polyBLEP saw/square/naive triangle, peak
attack/release detection, a soft-knee explicit-sidechain compressor and prepared
arbitrary-ratio sinc conversion. Seven integration fixtures pass for phase,
independent exponential/curve landmarks, arbitrary-ratio partitions/counts,
backpressure, DC/passband/stopband, reset and heap-free paths. Fixed numerical
profiles have documented quality/latency limits; no callback deadline or general
spectral support claim is added.

W18 adds explicit bounded PCM16/24/32/Float32 RIFF streaming, frame seeking,
record finalization and an atomic interleaved SPSC audio ring. Five fixtures pass
for independent chunk data, truncated/malformed inputs, quantization, tails,
odd padding, fault-injected partial I/O, backpressure and 100000 ordered stereo
frames across two threads. Prepared ring exchanges count zero Rust heap actions.
Memory-stream and synthetic handoff evidence does not establish device recording
or a complete playback transport. Other codecs and scheduling remain pending.

W19 adds fixed-capacity portable MIDI voice allocation and statically configured
lower/upper member expression. Four integration fixtures plus an age-rollover
unit test pass for duplicate-note ownership, deterministic steal priority,
sustain, pressure/bend/timbre, master/member expression and channel reuse. Heap
fixtures include 10000 workloads and rollover. This is documented receiver policy,
not dynamic MPE negotiation, hardware validation or a synth/format export.
W18 also gains explicit usize counter-wrap and Float32 nonfinite-bit fixtures.

W20 adds f32 input enumeration/selection/capture and matching-rate FIFO duplex
effects, using the shared prepared audio ring and output panic containment.
Four input/duplex fixtures plus existing output/containment fixtures pass;
1000 capture/render workloads count no Rust heap actions. Package/probe Clippy
passes with warnings denied. Clock drift compensation, sample-exact alignment,
integrated recording, MIDI and hotplug recovery remain separate work.

Physical W20 probe on 9 October, macOS 27 arm64: UMC1820 f32 18-channel input /
20-channel silent output at 96 kHz. One-second capture reports 190 callbacks,
97280 accepted frames, no dropped/invalid frames or backend errors. Output
reports 188 callbacks, 96256 frames, zero DSP/backend/panic errors and zero input
underflow frames. Only counters were retained; no sample recording was made.
This verifies this interface's callback delivery, not audible processing,
round-trip latency, drift over long sessions or hotplug recovery.

W21 adds prepared acyclic f32 routing with stable node IDs, compiled topology,
serial/parallel channel mixing, latency compensation and conservative tails.
Six fixtures pass for gain/events, independent delay impulses/partitions, faults,
invalid topology, aliases and failed reprepare; 1000 workloads count no Rust heap
operations. GraphStatus deliberately exposes later-node partial progress and
requires reset after faults. This is not third-party discovery, GUI hosting or
a Processor contract with arbitrary-node rollback.

W22 adds bounded OSC codecs and explicit worker/control UDP endpoints. Four
fixtures pass for independent network bytes, truncated/malformed packets,
optional scalar/bundle roundtrips, structural limits and localhost oversize/
timeout/recovery. Warning-denied package Clippy passes. Scheduling, pattern
matching, nested timetag semantics and audio event delivery remain separate.

New VST3 REAPER script is prepared for normalized control/audio/state/automation
checks in a fresh target resource directory. Its first startup scanned unrelated
system VST3 paths and stalled in vendor activation/scanning dialogs; no new DAW
validation pass is claimed from that launch. Only isolated test processes were
stopped before retrying with prior test scanner metadata; no user licence data
or normal application settings were copied.


## W23 — uniform partitioned convolution — 9 October 2026

Added a prepared single-channel f64 FFT overlap-add engine with responses up
to 262,144 taps, power-of-two partitions 16..4,096 and exact one-partition
latency. Caller block sizes do not affect transform boundaries. Three targeted
fixtures pass: delayed independent direct sums over multiple response/block
sizes, maximum response endpoints and invalid preparation, and 1,000 prepared
processing/reset workloads with zero counted Rust allocation/deallocation.
No live response swap, multichannel backend, SIMD or real-time deadline is
claimed. Updated the device crate introduction to cover implemented input/FIFO
duplex behavior. See decision 0031 and the signal-processing guide.


## W24 — explicit MIDI connections — 9 October 2026

Added optional `ravet-midi-device` with pinned MIT midir 0.11.0, exact backend-ID
selection, explicit input/output ownership and prepared complete-packet handoff.
Short MIDI 1.0 and seven-bit complete SysEx are bounded to 1,024 bytes; full rings
drop newest input, malformed packets reject, timestamps remain connection-local
microseconds. Four fixtures pass including 10,000 threaded maximum-payload
transfers, usize counter rollover and 10,000 heap-instrumented queue cycles.
The explicit native virtual-endpoint example on macOS CoreMIDI transferred six
note/controller/bend/clock/SysEx packets in order and closed both endpoints. It
sent only to its temporary virtual destination; no physical MIDI device was used.
No hotplug/reconnect, fragmented SysEx, UMP, audio scheduling or other runtime
backend/deadline is claimed.

## VST3 suspended-edit recall and REAPER evidence — 9 October 2026

REAPER 7.82 on macOS arm64 discovered and instantiated the actual public VST3
bundle. Initial startup-driven playback ran before audio dialogs cleared and
returned zero peaks; after normal startup, live wet/bypass output worked but
recalling edits saved while host bypass suspended processing failed. Fixed the
actual gap with complete controller getState/setState and validated SDK
connection-point restore synchronization, including restore-before-connect.
Native regression contracts and the 47-test Steinberg validator pass. REAPER
then passed wet/bypassed synthetic stereo peaks, complete wet/dry session recall,
normalized automation before/after seeking and generic parameter-window creation.
No custom VST3 editor, physical sound output, deadline or other DAW is claimed.
Only disposable test resources were changed; master hardware volume stayed zero.


## W25 — buffered file/device workers — 9 October 2026

Added explicit WAV read-ahead and recording pumps over prepared rings. Read-ahead
retains unqueued decoded suffixes, reports EOF separately from buffered audio
and supports stopped seeks that validate matching endpoints before discarding
an old generation. Recording drains bounded scratch, poisons on dequeued errors
and finalizes after stopped-producer drain. Four worker fixtures pass for tiny
ring backpressure/order, stopped/foreign seeks, bounded final drain and injected
partial-write/nonfinite PCM failure. A fifth prepared-device integration records
and decodes 201 exact synthetic stereo frames with zero capture drops. These
fixtures use memory streams and synthetic input; no physical recording, implicit
worker/transport, file destination or capture permission is selected.

The controller recall change initially regressed AU preset rendering because the
SDK wrapper's controller can lag AU parameter calls until UI notifications run.
The SDK's `IVst3ToAUWrapper` marker now preserves its original component-based
state behavior. Actual AU bundle render/gain-recall/host-bypass tests pass again,
and the rebuilt AAX bundle passes its export/version probe. AAX host/signing
validation remains separate.


The final VST3 REAPER regression also unbypasses a restored dry session without
a new gain edit and verifies resumed half-gain DSP. A zero master fader caused
intermittent decaying meter observations; the harness now keeps master processing
at unity and explicitly zeros every hardware-send gain. The complete harness
passes again with physical destinations silent. Automation is observed inside
its held regions, with bounded repositioning when deferred UI timing crosses
a loop boundary. This is processing/state evidence, not deadline evidence.


## W26 — reusable local web controls — 9 October 2026

Moved gain slider/number and bypass widget behavior into `ravet-surface` local
DOM components, adding reusable numeric choices and action buttons. Explicit
consumer transport owns physical edits/gestures; one slider owns a document
gesture, held navigation keys group updates, focused/active refresh sends no
feedback, and rejected ends retain ownership for retryable disposal. The Rust
inline assembly helper validates one marker and a 1 MiB assembled document cap.
The actual rebuilt gain CLAP/WebKit probe passed three complete create/attach/
resize/edit/gesture/automation/meter/hide/destroy cycles. Additional in-page
checks cover all four components, pointer cancellation, held-key grouping,
rejected begin/end, disposal and noncanonical numeric option spelling. The
resulting native screenshot was inspected. Broader layout/drawing/fonts/assets/
themes, assistive-tech audit and other native runtime backends remain pending.


## W27 — explicit preset-file libraries — 9 October 2026

Added optional `ravet-preset` outside portable state/umbrella dependencies. An
explicit existing directory/product identity and decoding limits supply sorted
metadata discovery, validated atomic replacement through existing runtime file
mechanics and undoable complete selected-file recall. Three filesystem fixtures
pass for sorting/replacement, missing/malformed/foreign/destination-limit failures
that preserve state/history, rejected pre-write replacement, Unix symlink skipping,
portable ASCII stem validation and the 4,096 total-directory-entry limit. No
default path, watcher, categories/Unicode titles, host program/discovery extension
or automatic DSP publication is claimed.


## W28 — runtime contract workflow configuration — 9 October 2026

Configured GitHub workspace formatting/tests/lints/Rustdoc for macOS, Linux and
Windows, with explicit Linux ALSA build dependencies, two Cargo jobs, independent
matrix failures and timeouts. A separate macOS job builds/development-signs the
public CLAP bundle and exercises its ABI smoke host. Workflow YAML, matrix,
permissions and worker-cap settings parsed/checked locally. The rebuilt CLAP ABI
host passes 18 blocks and six lifecycle cycles. Remote matrix execution, other
platform runtime and licensed-SDK/DAW/device/GUI/deadline jobs remain pending.
No changes were pushed and no artifacts were published.

## Current outstanding scope — 9 October 2026

The current catalogue is authoritative for implemented/prototype limits; the
initial M1 remaining-work section above is historical. Further work includes:

- Generic format exports: instruments, f64/aux/event/output/transport mappings,
  custom editors, AUv3 and host preset/program registration. The gain references
  cover narrower, individually validated contracts.
- Device recovery/hotplug, adaptive duplex clocks, full standalone transport/UI,
  physical recording and additional MIDI device/platform validation; UMP and
  full expression conformance/instrument export; single-zone negotiation added in W30.
- Native drawing/layout/fonts/assets/themes/window APIs and accessibility audits
  beyond reusable local DOM controls and existing webview prototypes.
- Additional audio codecs, live transport/seek/loop scheduling and further DSP
  quality/performance profiles beyond prepared current kernels.
- Third-party plug-in discovery/loading/editor ownership and dynamic/f64/event
  graphs beyond the prepared static portable routing contract.
- Executed remote/platform/DAW matrices, application templates/installers, release
  signing/notarization and production deadline/reliability evidence. AAX release
  signing remains a private consumer workflow; Pro Tools validation is pending.
- Graphics/GPU, animation/video, and explicit consumer opt-in analytics/unlocking
  facilities beyond current runtime networking and file/worker services.

These remaining scopes prevent a claim of overall framework feature completeness.


## W29 — independent offline starter — 9 October 2026

Added an explicit project generator with a public offline gain template. New
consumers stay outside RAVET in independent, publish-disabled workspaces; existing
destinations reject without overwriting, and name/licence inputs are explicit.
Two generator fixtures pass for manifest/scope boundaries and existing-file
preservation. A generated synthetic project in a disposable directory outside
RAVET was locked and built offline with two Cargo jobs, then processed/checked
four stereo frames successfully. No persistent proprietary consumer was added,
workspace membership changed, device opened or release artifact published.
Further format/device/UI and installer/release starters remain pending.


## Final local regression evidence for W21–W29 — 9 October 2026

On macOS 27.0.1 arm64 / Rust-Cargo 1.99.0, two-job workspace all-target tests
pass 190 unit/integration fixtures; five Rustdoc examples pass separately.
Warning-denied workspace Clippy and Rustdoc, formatting, generated catalogue/
link checks and diff whitespace checks pass. All 18 Python checker/generator
fixtures pass. Native WebKit shared-control/reference lifecycle checks, CoreMIDI
virtual loopback, the generated external offline starter and rebuilt CLAP ABI
smoke checks pass. The VST3 SDK validator passes 47 tests; actual AU bundle
render/recall and AAX export/version probes pass with the controller fix.
REAPER VST3 wet/dry recall, resumed bypassed edits, automation and generic-window
checks pass. A final host API check confirms stopped transport and zero hardware
send gains; the disposable test main window remains open. No Rust compiler/build
processes remain running. Overall outstanding scope is recorded above.


## W30 — RPN negotiation and memory transport — 9 October 2026

Implemented per-channel fixed-storage RPN decoding and opt-in single-zone voice
negotiation in `ravet-instrument`. Lower/upper configuration, default reset,
shared member/independent master bend ranges and active snapshot refresh are
covered. Invalid/null/NRPN/reset selectors preserve the intended receiver state;
released tails freeze. Three new fixtures include 10,000 prepared negotiations
without Rust heap operations. This does not establish complete MPE conformance,
controller hardware support or instrument-format export.

Added `ravet-audio-file::MemoryPlayback`: bounded copied interleaved clips with
play/pause/stop/seek, half-open loops, EOF silence and rejection without transport
mutation. Three fixtures cover exact frame boundaries, command faults and 10,000
prepared operations without heap activity. The synthetic WAV transport example
connects worker-side decoding to looping and pause. Streaming seek/clock mapping,
resampling, command queues, smoothing and physical playback remain separate work.


The same package also closes the master-expression snapshot gap: pressure and
CC74 from the master are retained separately from member values, refresh active
members, and freeze on release. A further fixture verifies both zone sides,
sustain, controller reset and polyphonic zero master values. The synth consumer
owns combination policy; overlapping zones and instrument exports remain pending.


## W31 — integrated portable instrument — 9 October 2026

`SineInstrument<N>` consumes voice ownership and linear envelopes in a portable
no-input mono/stereo f32 processor. Sample-timed notes, phase-continuous member/
master bend, selected-side RPN negotiation, sustain, deterministic stealing and
release completion are integrated. Complete block validation preserves state on
fault; zero-frame events and inactive/failed preparation behavior are covered.
Three new fixtures compare analytic sine/attack samples, split-block equality,
stereo initialization/tails, release and 10,000 synthesis workloads without Rust
heap activity. The offline executable renders only to memory. Native instrument
exports, complete state/editor support and physical controller/device deadlines
remain unvalidated and unfinished.


## Local regression evidence for W30–W31 — 9 October 2026

On the existing macOS arm64/Rust 1.99.0 environment, with two Cargo workers:
workspace all-target tests pass (200 unit/integration tests), workspace doctests
pass (5), warning-denied Clippy and Rustdoc pass. Formatting, documentation
freshness/link checks and diff whitespace checks pass. The Python script suite
passes 18 tests. Both synthetic transport and synthesis executable examples
pass without opening devices or writing audio files. New API evidence remains
offline; unchanged native format and REAPER evidence is recorded above rather
than expanded into a new instrument or platform claim.


## W32 — native CLAP main bus and MIDI instrument — 9 October 2026

Added `MainBusBridge<P,S>` with prepared native f32/f64 storage, no-input or
0..64-input/1..64-output main buses, selected-port timed short MIDI, global
parameters and block-start transport. Three bridge fixtures cover native f64
precision across 64 channels, MIDI length/port/timestamp validation, instrument
fault preservation and 10,000 callbacks without Rust heap activity. Auxiliary
buses/output events and CLAP note-ID/modulation mappings remain pending.

Built RAVET Sine CLAP (`org.ravet.sine`) with stereo f32 output, MIDI-only note
input, a stable level and atomic 16-byte level state. Linked and actual-binary
fixtures pass 3,009 blocks across three activation cycles, including timed notes,
stereo release silence, active/inactive flush, invalid-load/block preservation,
active state recall and 3,000 callbacks with no heap operations. Stale audio
reflection cannot overwrite a newer restore. The local CLAP validator reports
34 passed/10 skipped/0 failed. Its 768 kHz test exposed an unnecessary DSP rate
cap; finite positive rates now validate actual envelope count capacity and an
explicit overflow fixture rejects before processing. Other DAW/platform and
production-signing evidence remains separate.


## W33 — explicit opt-in analytics — 9 October 2026

Added a default-disabled bounded numeric event queue in the optional runtime
crate. Explicit consent enable/revoke, complete JSON byte/event limits, newest
overflow rejection and whole-batch delivery retry are implemented. No endpoint,
identifier, background worker, persistence or transmission is selected. Three
fixtures verify default-off/empty/revoked no-send behavior, valid ordered JSON,
failed retry preservation and invalid/oversized inputs with a synthetic sender.
Consumer policy and production delivery remain private and unvalidated.

W32 workspace regression checks pass 207 unit/integration tests plus five
doctests, warning-denied Clippy/Rustdoc and rebuilt gain ABI/validator checks
(34 passed/10 skipped/0 failed). REAPER scanned the new sine descriptor, but the
isolated host's startup/UI did not respond sufficiently to execute its new script;
DAW instrument audio/state/automation evidence is still pending. The script uses
synthetic MIDI and silences hardware sends before playback. No new host support
claim is made from descriptor scanning alone.


## W34 — local signed entitlements — 9 October 2026

Added canonical bounded claims and explicit consumer-selected public-key/product
verification in the optional data crate. The domain-separated envelope uses
pinned ed25519-dalek 2.2.0 strict verification; weak/invalid/duplicate trust keys
reject. Successful immutable grants verify half-open time validity and cached
feature checks require current time again. No private-key API, endpoint, stored
grant, hardware/clock query or product gating policy is selected.

Three synthetic fixture groups pass: canonical ordering and exact validity edges,
stale-cache denial, every changed token byte/truncated prefix, trailing/oversized
records, wrong product/key, weak trust and malformed claims. Real consumer keys,
licence/session fixtures and activation UI remain outside this repository.

## W35 — bounded AIFF PCM streams — 9 October 2026

Added seekable big-endian AIFF PCM16/24/32 readers/writers with bounded scratch,
exact integer extended80 rates, validated FORM/chunk/frame lengths, SSND offsets,
explicit seeking/finalization, preserved read retry and poisoned partial writes.
Three fixture groups pass all widths and rate edges, independent reversed chunks,
every truncated prefix, malformed metadata/duplicates, odd padding and injected I/O.
Local macOS Core Audio afinfo recognizes the generated stereo PCM24 48 kHz file;
afconvert decodes all 1024 frames exactly to expected PCM16 samples. No physical
recording, compressed codec or WAV-worker integration is claimed.

W33/W34 full regression validation passes 213 unit/integration tests, five doctests,
warning-denied Clippy/Rustdoc and documentation/Python checks.

## W36 — portable visual mechanisms — 9 October 2026

Added finite geometry, bounded allocation-free row/column layouts, explicit-clock
scalar tweens and bounded escaped local SVG drawing with semantic themes and
explicit font styling. Four fixture groups pass geometry/capped redistribution,
shrink/resize/failure preservation, 10000 heap-free layout/tween calls, animation
retarget/edges/time overflow and hostile text/command/document bounds. Local
collaborative browser inspection renders 640-wide dark/time500 and 360-wide
light/time1000 examples without SVG parser errors; measured text fits the viewBox.
Native toolkit/accessibility, font assets, GPU/video and scheduling remain separate.

## W37 — optional compressed clip decoding — 9 October 2026

Added off-by-default worker FLAC/MP3/Ogg-Vorbis decoding with pinned Symphonia
0.5.5 (separate MPL-2.0 dependency), bounded accepted input/output/packet capacity,
stable signal specification, finite samples, whole-result errors and enabled
gapless trim. Two fixture groups pass 4096-frame stereo rates/counts, exact FLAC
PCM, lossy tone error/channel RMS, capacity and malformed/truncated declared files.
Only generated public tones are embedded; no product/session/licence assets.

Independent FFmpeg Float32 agreement RMS is 0 for FLAC and approximately 9.47e-8
for MP3. Vorbis overlap agrees at approximately 3.36e-8, with an observed duration
difference: FFmpeg 3968 frames versus declared/Symphonia 4096. Broader conformance,
compressed writing/stream seeking and dependency-internal total memory/deadlines
remain unclaimed. Runtime matrix now configures all-feature checks; remote runs
remain pending. W35/W36 full default-feature regression checks pass 220 tests plus
five doctests and warning-denied Clippy/Rustdoc.

## W32 follow-up — REAPER instrument validation — 9 October 2026

The isolated REAPER 7.82/macOS 27.0.1 arm64 test now passes RAVET Sine discovery,
timed synthetic MIDI stereo output, generic parameter-window presence, saved
level recall and two automation values. Measured stereo peaks are approximately
0.04 for level0.8, 0.02 for level0.4 and 0.04 after the second automation point.
Playback finishes stopped and every hardware destination remains muted. The test
retains the already-running 96 kHz device rate rather than overriding hardware.

Earlier pending/failed observations remain historical: first playback stayed at
position zero; a preferences reopen later blocked in Core Audio on the host main
thread. Restarting only the disposable copy, completing its evaluation notice and
retaining device rate resolved this run. No production project, global audio service, private consumer or production-signing
configuration was modified.

## W38 — explicit CLAP hosting boundary — 9 October 2026

Added optional bounded non-loading candidate discovery, byte-preserving native
catalogues and retained Clack entry access. Fixtures cover sorted unique explicit
paths, symlink/nested exclusion, non-loading invalid native bytes, directory/entry/
candidate limits and three live activation cycles after catalogue destruction.
Descriptor copies reject empty/oversized fields and preserve non-UTF8 bytes.

Built the unmodified upstream Clack gain example from official v0.2 commit
27ca28391b57a61bad4bb5099288a7bf9247350d under ignored target storage. Local macOS
arm64 explicit discovery/load/catalogue, four-byte state roundtrip and three
stereo f32 blocks pass with peak0.25 for input0.25. The same inspect command passes
RAVET Gain/Sine 32-/16-byte states and stereo effect/no-input instrument lifecycles.
No vendor source/binary is added to tracked repository files. The example supplies
only empty callbacks and a narrow main-bus host; GUI/richer callbacks, other formats,
crash isolation and broad third-party compatibility remain separate.

## W39 — bounded CLAP generated events — 9 October 2026

Added opt-in prepared generated output storage and explicit process/flush host
queues to the native main-bus bridge. Global parameters and selected-port MIDI/
normalized notes preserve event timing/order with explicit 7-bit conversion.
Representability preflight rejects whole unsupported streams; host refusal reports
accepted prefix/discarded suffix while preserving already-successful audio/DSP.
Old zero-output methods reject nonzero capacity before processing. No retry/replay.

Four fixture groups pass exact output fields/ports/padding/quantization, zero-time
flush, unrepresentable stream atomicity, host prefix refusal, input/DSP discard and
10000 prepared callbacks without heap operations. Exported MIDI-output-port and
other-format host evidence remains pending.

## W40 — guarded native plugin editors — 9 October 2026

Added a negotiated main-thread GUI guard borrowing the live plugin instance,
with explicit unsafe parent/transient lifetime, bounded embedded size/resize,
parent-before-show, idempotent accepted show/hide and paired hide/destroy on Drop.
Native callbacks/state can use short-lived handles while audio ownership remains
independent. Missing extension/unsupported API tests reject before native creation.

Local macOS Cocoa fixtures pass three child-view create/attach/show/resize-or-reject/
hide/drop cycles for RAVET Gain and unmodified upstream Clack EGUI gain at v0.2
commit27ca28391b57a61bad4bb5099288a7bf9247350d, including early scope exit and no
remaining parent children. The upstream GUI also passes catalogue/four-byte state/
three-block stereo audio inspection. Its vendor/GPU dependencies stay under ignored
target storage. No floating/other-platform, visual hide/focus/accessibility or
all-plugin compatibility claim follows. Runtime CI configures the own-bundle guard
fixture only; remote execution remains pending.

W37–W39 all-feature workspace regression checks pass 229 tests, five doctests and
warning-denied Clippy/Rustdoc. Rebuilt sine actual ABI/validator still passes 3009
blocks and 34 passed/10 skipped/0 failed. No exported MIDI-output port host evidence
is inferred from that existing zero-output instrument.

## W41: Portable UMP framing

Added fixed-storage exact packet framing for every message-type length, retaining
reserved payloads, with incremental truncation/reset and explicit big-endian
serialization. Message semantics and native UMP delivery remain separate work.
The W40 full regression exposed a parallel preset-test temporary-path collision;
a process-local sequence now prevents same-clock-tick names from colliding.
W40 doctests, Clippy and warning-denied Rustdoc passed; its failed test run is not
reported as a successful full regression.

## W42: MIDI 2.0 channel voice codec

Added every defined MT=4 channel voice status with validated sender widths and
full-resolution data. Independent word fixtures and exhaustive group/channel
round trips pass, including relative signed extrema, zero-velocity Note On,
unknown attributes, ignored reserved fields and bank validity. Native UMP delivery
and instrument expression policy remain unimplemented. W41 framing tests and
all preset tests pass after the temporary-path race correction.

## W43: UMP short messages and SysEx

Added MIDI 1.0 MT=1/2 semantic representation, validated SysEx7/SysEx8 chunk
codecs and single-address caller-buffer assembly with explicit SysEx8 abort.
Three framing/short-message and two SysEx test groups pass along with the
MIDI 2 codec tests; foundation Clippy passes. The W41/W42 full all-feature
workspace run passes after the preset-test path fix, with doctests, Clippy and
warning-denied Rustdoc also passing. Whole-stream routing/negotiation and native
UMP devices remain work.

## W44: Sample-timed UMP transport

Added framed UMP portable events and CLAP main-bus input/output routing. The
old MIDI 1-only translator remains available. Processor semantic policy and
reference MIDI 1 dialect declarations stay explicit. W43 full all-feature
workspace checks pass 237 tests and five doctests, with clean Clippy/Rustdoc.
The all-message-type prepared echo fixture is the W44 acceptance check; native
MIDI 2 DAW/device performance remains unvalidated.

W44 acceptance passes: all sixteen UMP message types retain ordered packet words
and selected output port, unused native words zero, with zero framework heap
operations across 10,000 prepared callbacks. The complete CLAP output test target
passes five groups.

## W45: Core CLAP host request handoff

Added coalesced atomic lifecycle requests and one-callback-per-tick main-thread
service. Actual linked synthetic CLAP initialization and reentrant callback tests
pass alongside concurrent publication; restart/process transitions remain
consumer-owned. The offline inspection example services explicit pre/post-render
callbacks and reports terminal pending requests. W44 rebuilt sine ABI passes
3,009 blocks/three activations; full all-feature workspace tests, doctests,
Clippy and warning-denied Rustdoc pass.

W45 full all-feature workspace checks pass 240 tests and five doctests, clean
Clippy/warning-denied Rustdoc, 18 Python checks, formatting and documentation.
The upstream Clack Gain actual bundle also passes updated host catalogue/state/
three-block rendering with request handlers. Rebuilt sine validator passes
34 successes/10 skips/no failures after UMP transport.

## W46: Bounded hosted parameter access

Added owned parameter metadata and explicit refresh/plain reads/inactive edits
under an exclusive live-instance guard. Active sample-timed automation remains
the audio consumer's responsibility. Linked Gain and synthetic malformed metadata
fixtures provide contract acceptance evidence; updated trusted offline inspection
prints hosted values.

W46 linked parameter acceptance passes two test groups. The upstream Clack Gain
actual bundle exposes one parameter (ID 1, plain value 1) through the new guard
and still passes four-byte state and three-block stereo rendering. Host discovery
test directories now also include a process-local sequence, applying the observed
same-clock-tick fixture fix consistently to parallel host tests.

W46 full all-feature workspace checks pass 242 tests and five doctests with
clean Clippy and warning-denied Rustdoc.

## W47: Native f64 graphs

Graph, node processors, route gains and delay/scratch storage now specialize
in one native f32/f64 precision, default f32. Graph-owned prepared storage remains
64 MiB with byte-counted capacity. Six existing f32 groups and two new f64 groups
pass, including sub-f32 precision, compensated delays, partition/reset, channel
aliases, capacity/nonfinite gain rejection and 10,000 heap-free callback workloads.
Graph Clippy passes. Native f64 host/device/deadline evidence remains pending.

W47 full all-feature workspace checks pass 244 tests and five doctests with
clean Clippy and warning-denied Rustdoc.

## W48: Prepared dynamic graph handoff

Added bounded prepared whole-graph handoff, one FIFO replacement per valid block
and control-owned retirement. Full retirement leaves current rendering active;
invalid outer blocks preserve proposals. Three initial groups pass FIFO/overflow/
config/control-loss/destructor-thread checks, 10,000 graph reuse cycles and 100
separate-thread transfers without callback heap operations. Native f64 handoff
is also exercised. rtrb 0.4.0 is pinned (MIT OR Apache-2.0), outside the umbrella.
State transfer/crossfade/tail drain and native deadline evidence remain separate.

W48 full all-feature workspace checks pass 248 tests and five doctests with
clean Clippy and warning-denied Rustdoc.

## W49: Explicit generated graph events

Added filtered host/node event edges and combined audio/event topology. Default
broadcast remains unchanged. Explicit mode prepares bounded input/output/host
writers within the 64 MiB graph audio/event budget. Merge overflow clears host
outputs and reports route-aware EventFault after possible earlier node progress.
Four initial acceptance groups pass filters/ordering/zero frames, overflow/reset,
failed edits/combined cycles and 10,000 callback heap checks; native f64 opaque
UMP routing is also exercised. Native host/device deadlines remain pending.

W49 acceptance passes all five event-routing groups, including native f64 UMP
transport. Full all-feature workspace checks pass 253 tests and five doctests
with clean Clippy, warning-denied Rustdoc, formatting and documentation checks.

## W50: Native CLAP auxiliary bus mapping

Added ordered native f32/f64 main/auxiliary mapping with independently compact
input/output port indexes. Complete host input is copied/preflighted before DSP;
all native output ports silence on rejection/fault. Combined sample/event scratch
is capped at 64 MiB. Three new fixture groups pass instrument-sidechain/output
auxiliary routing, exact f64 precision, malformed timing/precision/layout/fault
rejection, output discard and 10,000 allocation-free callback/flush/reset cycles.
Full all-feature workspace checks pass 256 tests and five doctests with clean
Clippy and warning-denied Rustdoc. Native exporter/DAW evidence is separate.

## W51: Exported fixed-threshold sidechain reference

Added a public stateless stereo main/mono detector CLAP reference with native
f32/f64 selection and fixed 0.1 threshold. Linked ABI acceptance passes 6006
blocks across three 44.1/48/96 kHz activation cycles, precision/port metadata,
nonfinite detector closure, missing-sidechain silence and prepared heap checks.
Actual bundle/validator/DAW evidence is recorded as executed below.

W51 built macOS arm64 sidechain ABI passes 6006 blocks/three cycles with no
audio-thread diagnostic logging. External CLAP validator 0.4.1 reports 21 passed,
23 skipped, zero failed. REAPER 7.82 discovers three input/two output pins and
maps detector only from channel three. Actual apply-FX four-channel 96 kHz PCM24
render passes 115200 first-two-channel frames across four off/on regions, max
error 1.1920929e-7. Precision chosen by REAPER was not measured. Native UI was
unavailable (cgWindowNotFound); the documented Lua CLI runner executes normally.

The first live-meter test observed peak decay after detector closure, so actual
PCM files provide acceptance. Initial apply-FX used REAPER's default Documents
media location; the generated file was moved into the isolated directory and
the script now sets RECORD_PATH and current project filename explicitly before
rendering. REAPER retains four source channels and a default one-second tail;
only declared first-two-channel interior regions are checked. Earlier accessor
checks were replaced by independent file-sample validation. No production
project/device settings or signing configuration were modified.

W51 full all-feature workspace checks pass 257 tests and five doctests, with
clean Clippy, warning-denied Rustdoc, 20 Python checks, formatting, whitespace
and documentation checks.

## W52: Two-zone MPE voice receiver

Added a separate shared 1..128-slot receiver with lower/upper RPN negotiation,
latest-overlap precedence, shared-channel/master compatibility notes, independent
zone ranges and member expression frozen after note-off while master expression
continues on zone tails. Existing single-zone/sine behavior remains unchanged.
Four new groups pass all 256 size pairs in both orders, channel reuse, polyphony,
master notes/ranges, isolation/sustain/tail rules, fallback/reset and 10,000 heap
workloads. The public MPE specification informed these rules; native conformance,
MIDI-CI/program/mode/extra-pedal and new synthesis integration remain separate.

W52 full all-feature workspace checks pass 261 tests and five doctests, clean
Clippy and warning-denied Rustdoc. Legacy instrument/voice acceptance still passes.

## W53: Prepared UMP device handoff

Added unique bounded SPSC UMP endpoints with exact numeric words, explicit
protocol labels and opaque timestamps. Whole-batch framing preflight precedes
publication; full/abandoned queues retain an accepted prefix and discard suffix.
Three groups pass all framed types, invalid whole batches, overflow/abandonment
and 10,000 prepared/cross-thread heap workloads. rtrb 0.4.0 (MIT OR Apache-2.0)
was already pinned and now also serves the device crate outside the umbrella.
Native UMP connection and clock/negotiation evidence remain separate.

## W54: Native CoreMIDI UMP endpoints

Added a separate macOS backend crate with narrow guarded native CoreMIDI block/
port ownership and Rust exact-ID selection/framing/queues. Native input callback
entry is nonblocking/serialized; close gates and waits outside callbacks before
freeing Rust state on the owner. Protocol/host-clock tick units are explicit.
cc 1.6.0 (MIT OR Apache-2.0) was already pinned and builds the macOS shim.

A target-only SDK feasibility probe preserved exact MIDI 2 group/channel/key/
16-bit velocity. The reusable backend's actual virtual probe passes 96 MIDI 1/2
packets across all 16 groups and six owned endpoint cycles, including exact-ID
enumeration, diagnostics, producer teardown and native endpoint removal. No
hardware messages were sent. Rust callback entry tests pass 10,000 handoffs and
30,000 invalid metadata entries with zero heap operations. Physical devices,
protocol negotiation, hotplug, other OS and callback deadlines remain separate.

W54 native shutdown stress also passes ten owned destinations closed during
worker submissions, followed by producer teardown, bounded retained packet
checks and exact-ID endpoint removal. Native handles remain on their respective
owner threads; only IDs and separate consumers cross threads.

W53/W54 full all-feature workspace checks pass 265 tests and five doctests,
clean Clippy and warning-denied Rustdoc. These are local macOS checks; native
CoreMIDI evidence above remains separately scoped.

## W55: Deferred CLAP host parameter callbacks

Added opt-in host extension handlers with prepared owner-thread exact-ID clear/
rescan handoff and atomic shared flush/core requests. Clear overflow retains the
64-ID prefix and faults rather than clearing unrelated references. Four groups
pass actual native initialization/main-thread extension calls, exact flags/order,
overflow, concurrent flush/core preservation and 10,000 zero-heap workloads.
The offline inspection example now advertises these callbacks, rejects overflow
and reports bounded snapshots; its process calls serve initialization flush.
Automatic reference storage, refresh/recovery and active scheduling remain work.

W55 full all-feature workspace checks pass 269 tests and five doctests, clean
Clippy/Rustdoc, Python checks, formatting, whitespace and documentation checks.
The actual built gain bundle also passes offline inspection through ParameterHost.

## W56: Hosted audio-port and timing metadata

Added owned inactive directional snapshots bounded to nine ports/64 channels,
with IDs/flags/type bytes and validated equal-channel opposite in-place pairs.
Failed inspection/refresh preserves the old snapshot; active refresh rejects.
Two groups pass linked sidechain/instrument layouts, nine malformed/capacity
modes, directional ID reuse, byte preservation, active rejection, exact 65537-
sample latency, finite/infinite tail and 10,000 zero-heap tail queries.

The actual built sidechain bundle's metadata command reports two inputs (stereo
main/mono detector), one stereo output, common native precision/in-place main
IDs and absent latency declaration. No layout negotiation/hosted graph or
native change-notification support follows from these metadata checks.

W56 full all-feature workspace checks pass 271 tests and five doctests, clean
Clippy and warning-denied Rustdoc. Native metadata evidence remains separately
scoped to the explicitly loaded public reference bundle.

## W57: Deferred hosted audio configuration notifications

Added combined parameter/port/latency/tail-aware host handlers and one prepared
atomic audio-change mailbox. Main/audio callbacks only publish; consumers own
deactivation/timing queries, prepared rebuilding and retirement. Two new groups
pass actual linked main-thread rescan, activation latency and audio-thread tail
calls, coalescing/deferral, unknown-bit masking and 10,000 zero-heap workloads.
The offline inspection example integrates these handlers and reports timing/
changes, without automatic native graph/device scheduling.

W57 full all-feature workspace checks pass 273 tests and five doctests, clean
Clippy/Rustdoc, 20 Python checks, formatting, whitespace and documentation.
Actual built sine inspection passes combined handlers, zero-input stereo ports,
state roundtrip and three blocks, reporting absent latency/infinite tail on the
correct owners. No automatic native restart/graph scheduling is claimed.

## W58: Portable native f64 gain and meters

Added separate f64 gain audio with existing shared f32 gain/bypass control/state/
smoothing policy. Native scaled-squares meters retain finite extrema/subnormal
levels and explicit nonfinite diagnostics; optional f32 telemetry saturates.
Three new groups pass exact sub-f32 audio/bypass/events, extreme RMS, subnormal/
nonfinite readings, state/reset/smoothing/zero-frame and rejected-state retention,
plus 10,000 zero-heap prepared f64 workloads. Native format precision mappings
and UI meter transport remain separately scoped.

W58 full all-feature workspace checks pass 276 tests and five doctests with clean
Clippy and warning-denied Rustdoc. Portable native f64 audio does not imply host
sample-size support.

## W59: Separate native f64 gain ABI

Added RavetGain64 native lifecycle/process symbols over the f64 reference, with
f32 controls/shared state codec. Both ABI precisions expand one bounds/alias/fault
implementation; old f32 fixtures still pass. Two additional groups pass exact
f64 samples, cross-channel aliasing, backing tails, bypass/state recall, invalid
preparation and 10,000 zero-heap native workloads. Actual linked public-header
C++ acceptance passes 6006 blocks/six mono/stereo 44.1/48/96 kHz cycles, without
SDKs or hardware. Native format sample-size mappings require separate evidence.

## W60: VST3 native f64 gain mapping

Added separate setup-selected f32/f64 native handles with shared complete state,
exact native sample pointers, precision mismatch rejection and actual per-channel
output silence flags. C++ contracts pass 6006 f64 normal/zero-frame blocks across
six mono/stereo 44.1/48/96 kHz cycles, sub-f32 values/aliases, sample bypass,
malformed preservation, backing tails, precision switching, wrong-size rejection
and both-precision silence flags. The actual dynamic SDK 3.8.1 validator passes
47 tests/zero failures including its now-exercised 64-bit suite. Prior informative
wrong-silent-flag messages disappear after actual-output flag reporting.

W59/W60 full all-feature workspace checks pass 278 tests and five doctests, clean
Clippy/Rustdoc, 20 Python checks, formatting, whitespace and documentation.
REAPER native precision for this change remains unobserved. Embedded AU/AAX
wrappers need separately rebuilt regression evidence, and AAX signing/Pro Tools
acceptance remains the private consumer workflow.

W60 embedded-wrapper regressions also pass: rebuilt AUv2 actual-bundle normalized
parameters, stereo audio, bypass/state/host-bypass and tails; rebuilt AAX bundle
required exports and SDK version probe. These preserve existing tested paths,
not AU/AAX native f64 or Pro Tools acceptance. AAX remains ad hoc signed only.

## W61: Lazy compressed worker reader and exact seeks

Added optional lazy FLAC/MP3/Vorbis reads with retained packet suffixes, exact
atomic fresh-decoder replay seeks and explicit deferred-fault poison/recovery.
Three groups compare all actual synthetic encoded formats with whole decode for
partitioned reads/tails and forward/backward/packet/end positioning, invalid
outputs, failed-seek preservation and deferred malformed-stream recovery.
Encoded input remains bounded memory; replay is O(target) outside callbacks.
Indexed/file-backed seeking and automatic playback/ring scheduling remain work.

## W62: Compressed prepared-ring read-ahead

Added explicit bounded compressed worker buffering into the existing prepared
SPSC audio ring, with retained suffixes, verified-EOF reporting, decode poison and
atomic stopped seek/generation clearing. Three groups pass all actual compressed
fixtures for backpressure/generation/end delivery, worker-to-consumer exact order
and measured heap-free handoffs, plus deferred corruption and stopped recovery.
Physical playback, rate/clock scheduling and automatic recovery remain separate.
The first fixture incorrectly used a non-power-of-two ring capacity; corrected
to the documented capacity before rerunning acceptance.

W61/W62 full all-feature workspace checks pass 284 tests and five doctests, clean
Clippy/Rustdoc, 20 Python checks, formatting, whitespace and documentation.
The first W61 Clippy pass requested the standard is_multiple_of form; corrected
before this combined successful check. Worker/consumer fixtures remain offline
synthetic evidence, not physical device/clock or deadline validation.

## W63: Optional integer PCM FLAC writing

Added single-worker complete signed PCM16/24 encoding through pinned flacenc
0.5.1 (Apache-2.0), default/parallel features disabled. Input channels/rates/
precision/sample count and returned byte limits are explicit. Three groups pass
exact roundtrips for all eight channel counts/two precisions, 1/17/4097/4113-frame
boundaries/extrema, rate edges, invalid inputs and source preservation.

Initial one-frame output revealed backend STREAMINFO min/max below 16; normalized
to the planned fixed 4096 size per RFC 9639, excluding the final block from the
minimum without padding audio. Independent FFmpeg CRC-checked decoding then
matches all six PCM16/24 1/4097/4113 stereo files byte for byte. Optional encoding
compiles without the separate decode feature. Native recording and other encoder
formats remain work; backend internals have no total-memory/deadline guarantee.

W63 full all-feature workspace checks pass 287 tests and five doctests, clean
Clippy/Rustdoc, 20 Python checks, formatting, whitespace and documentation.
The encoder remains opt-in and complete-clip; physical recording, native clocks
and non-FLAC compressed writers remain independent work.

## W64: Held keyboard gestures and reusable meter text

Sliders now retain gesture ownership across multiple held navigation keys and
unrelated modifier releases. Added a native-meter component with full-level
peak/RMS dB accessible/visible text, overload and invalid-audio indications,
validation before mutation, bounded custom text and explicit disposal. The gain
page consumes it. Actual rebuilt macOS WebKit editor passes three complete
lifecycle cycles with all five component fixtures and host control/meter checks.
Native screenshot inspection shows readable unclipped text. Browser tab order
and ArrowRight submission produce begin/value/end exactly once. T3 snapshot
calls failed twice; DOM and keyboard tools worked, but the document lost focus
after automation, leaving visible-focus and assistive-tech audits unvalidated.

W64 full all-feature workspace checks pass 287 tests and five doctests, clean
Clippy/Rustdoc, 20 Python checks, formatting, whitespace and documentation.

## W65: Owned CoreMIDI virtual sources

Added explicit protocol-selected virtual sources with lifetime unique IDs and
exact-word distribution to subscribers. Native macOS 27.0.1 arm64 source probe
passes 198 deliveries across six MIDI 1/2 lifecycles and all 16 groups, two
subscribers, independent input close, source removal, stale-ID/direction mismatch
and invalid-name rejection. Source close preserves subscriber producer ownership;
input close ends it. No hardware connections, negotiation or persistent-ID policy.

The W65 destination regression also passes 96 exact packets/six lifecycles and
ten concurrent owner-close/worker-send cycles. All-feature workspace checks pass
287 tests and five doctests before the subsequent registration changes.

## W66: Borrowed CLAP parameter metadata registration

Added preparation-time portable-schema/ordered-options validation and complete
count/info metadata. Physical ranges remain exact; incompatible strings, IDs,
steps and bypass reject before exposure. Gain consumes the registration and
marks active output-affecting changes as requiring process. Two adapter groups
cover boundaries/metadata and 10,000 heap-free reads; a linked host-extension
check verifies actual gain metadata. Product state/formatting/flush/scheduling
remain consumer policy, and other-format registration remains separate work.

W66 rebuilt gain smoke/validator passes (34 success, 10 skips, no failures).
All-feature workspace passes 290 tests and five doctests with clean Clippy.

## W67: Incremental lossless FLAC output

Added explicit worker-owned empty seekable sinks, retained 4096-frame blocks,
input/output budgets, error poison and consumed finalization with MD5/count/frame
metadata. Three groups pass all eight channels/both precisions/partitions/tails,
input preservation and partial write/seek/flush failures. Initial minimum frame
size zero incorrectly survived finalization; first-frame initialization fixes it.
A five-channel PCM24 fixture exposed a ~192 MiB prediction size; preflight
rejected before serialization. A constant/verbatim fallback now handles oversized
prediction choices, shared by complete-clip encoding, with exact samples.
Backend internal allocations/deadlines remain outside the wrapper guarantee.

W67 independent FFmpeg CRC-checked decoding agrees byte for byte with all seven
actual incremental records, including five-channel prediction fallback. Full
all-feature workspace passes 293 tests/five doctests, clean Clippy/Rustdoc,
20 Python checks, formatting, whitespace and documentation.

## W68: Prepared-ring FLAC capture workers

Added bounded worker float/PCM scratch, explicit reject/clip overload policy,
nearest/ties-away quantization with signed endpoint mapping, post-dequeue fault
poison and stopped tail drain/finalization. Four groups pass boundaries/overload/
nonfinite/capacity/partial-output errors and exact 8193-frame threaded PCM24 order
with heap-free producer handoff. Completion gates producer teardown before file
finalization. The public synthetic example builds with encoder alone and writes
actual PCM16/24 stereo capture files; physical recording/clock recovery remains
separate. No automatic capture device, thread, path or permission policy.

W68 independent FFmpeg CRC-checked decoding matches both actual 8193-frame
PCM16/24 stereo capture files exactly. All-feature workspace passes 297 tests
and five doctests, clean Clippy/Rustdoc, 20 Python checks, formatting, whitespace
and documentation.

## W69: Rust-owned native editor C ABI

Added a macOS C ABI over retained Rust WebView ownership and counted callbacks,
explicit main-thread status checks and optional exactly-once foreign context
release. Gain-native offers default-disabled linkage. Actual compiled public-
header C++ client passes three native lifecycle cycles, wrong-thread rejection/
retained-destroy retry and message/snapshot self-destruction with context release
after callback return. The first fixture queried before DOM readiness; bounded
readiness polling fixes it. Two Rust groups cover snapshot validation/failure
ownership. Native gain64 regression contracts also pass. Format editor adapters
and other-platform bindings remain independent work.

W69 full all-feature workspace passes 299 tests/five doctests, clean Clippy/
Rustdoc, 20 Python checks, formatting, whitespace and documentation. Actual
native public-header C++ editor checks and gain64 contracts pass separately.

## W70: Optional actual-bundle VST3 gain editor

Added explicit macOS native-editor bundling, Rust bounded gain UI codecs and an
SDK IPlugView with normalized host gestures/controller refresh, rejected-value
rollback and retained/retried end ownership. Actual exported bundle passes three
Cocoa lifecycle cycles, same-view reattachment, gain/bypass/held keys/automation
refresh, rejected values/end removal retention and resizing. Native screenshot
inspection confirms readable controls/status. SDK f32/f64/state contracts and all
47 validator tests pass. Two Rust codec groups verify schema/JSON/output bounds.
The synthetic probe needed the complete IPlugView header; JavaScript fixture
locals are scoped to avoid redeclaration across repeated edits. REAPER/focus/
assistive-tech, meter delivery and other-format editor mapping remain separate.

W70 full all-feature workspace passes 301 tests/five doctests, clean Clippy/
Rustdoc, 20 Python checks, formatting, whitespace and documentation.

## W71: Optional actual-component AUv2 Cocoa editor

Added explicit editor-enabled nested VST3 bundling and actual component Cocoa UI
protocol validation. Three cycles pass gain/bypass edits verified in f32 rendered
audio, external host refresh, preset recall/refresh and subsequent edits, resize
and view-before-unit teardown. VST3 validator/editor and existing AU audio/state/
bypass/tail contracts also pass. Initial raw setter did not notify UI; AUParameterSet
fixes that host path. Bulk preset recall needs all-parameter listener notification
to refresh caches for later same-value changes. setFrameSize did not forward;
setFrame exercises the SDK mapping. Hidden snapshots were flat; showing the
owned disposable host window produces readable controls/status, then closes it.
No external SDK source was copied or changed. Meter delivery, actual DAW/auval/
focus/assistive-tech/sandbox/other architecture remain separately unvalidated.

W71 shared native CLAP editor regression passes three complete lifecycle cycles.
All-feature workspace passes 301 tests/five doctests, clean Clippy/Rustdoc,
20 Python checks, formatting, whitespace and documentation.

## W72: Captured sostenuto voice ownership

Added per-voice held-edge capture, independent sustain/sostenuto release and
changed-latch reporting. Four groups pass threshold/repeat/duplicate/later-note
behavior, both release orders, channel/zone/master isolation and slot lifecycle,
with 128-slot heap-free cycles. Sample-timed sine integration retains captured
notes then releases with identical partitioned output. Native export regression
and full workspace checks follow; controller/DAW conformance stays separate.

W72 rebuilt sine actual-bundle passes 3018 blocks/three activations, including
nine heap-instrumented pedal/audio blocks. Validator: 34 pass/10 skip/0 fail.
All-feature workspace passes 306 tests/five doctests, clean Clippy/Rustdoc,
20 Python checks, formatting, whitespace and documentation.

## W73: Complete parameter program banks and staged MIDI requests

Added borrowed complete catalogues with stable IDs/address uniqueness, canonical
state/capacity validation and copied selection proposals. Separate fixed MIDI
selectors stage either bank half and emit repeated/unavailable Program Change
requests, preserving consumer state until explicitly accepted. Five groups cover
metadata/states/capacities/channel isolation/reset and heap-free selection. The
UTF8-size fixture initially moved its still-borrowed string; keeping it alive
fixes the test. Reference/export/host selection integration remains separate.

W73 all-feature workspace passes 311 tests/five doctests, clean Clippy/Rustdoc,
formatting, whitespace and documentation. No dependency/host mapping change.

## W74: Sample-timed factory program recall in sine/CLAP

Added complete Default/Quiet/Full level programs and chosen-master MIDI bank/
Program Change routing with exact event timing, voice/phase preservation and
unknown-address preservation. Two portable groups pass timing/partition/order/
master/reset/fault behavior. CLAP changed active publications coalesce deferred
main-thread optional value rescans; stale reflection preserves newer restore.
A linked parameter-aware host passes three cycles, deferred/coalesced refresh,
complete state and heap-free callbacks. Actual-bundle/validator checks follow;
host program menus and DAW recall remain independent work.

W74 rebuilt sine actual-bundle passes 3024 blocks/three activations, including
analytic program levels and main-thread saved program state. Validator: 34 pass/
10 skip/0 fail. All-feature workspace passes 314 tests/five doctests, clean
Clippy/Rustdoc, 20 Python checks, formatting, whitespace and documentation.

## W75: Master-routed program requests in MPE receivers

Added independent master bank/program staging to both negotiated voice receivers,
with explicit requests separate from voice/configuration changes, ordinary
per-channel fallback and ignored member/unassigned bank messages. Zone/reset
clears all selectors; range updates/CC121 preserve. Sine now consumes negotiated
requests, removing duplicate decoder ownership. Two groups pass both sides/
zones/reset/preservation and 10000 heap-free cycles per side; native export
regression/full checks follow. Sound commit and native conformance remain separate.

W75 rebuilt sine actual-bundle passes 3024 blocks/three activations; validator
34 pass/10 skip/0 fail. All-feature workspace passes 316 tests/five doctests,
clean Clippy/Rustdoc, formatting, whitespace and documentation.

## W76: Complete gain programmes and native catalogue ABI

Added Unity/Silence/Bypass complete schema states and immutable native copied
metadata with stable IDs/addresses/zero-padded names. Portable/native Rust groups
pass both precision restoration/state/render, atomic rejection and heap-free
metadata. The codec fixture initially passed scalars; using its complete state
fixes it. Actual C++ public-header client passes metadata/rejection and codec/
DSP renders for both precisions; gain64 regression also passes. Native format
menu/selection mapping remains the next integration work.

W76 all-feature workspace passes 318 tests/five doctests, clean Clippy/Rustdoc,
formatting, whitespace and documentation. Clippy requested Option propagation;
Rustdoc interpreted bare array values as links. Using `?` and inline-code values
fixes both checks. Native metadata/audio client and gain64 regression pass.

## W77: Optional actual-bundle VST3 factory programs

Added explicit default-disabled unit/list/nonautomatable selector metadata over
Rust programmes, complete controller host-value refresh and timed processor
expansion with whole-block expanded-capacity rejection. Actual bundle passes
36 blocks/six f32/f64 activations, names/flags, notification rollback/reentrant
write rejection, host value delivery, complete state/audio, zero-frame and 512/
513-point limits. SDK validator passes all 47 tests; Cocoa editor three-cycle
regression passes. Initial compile lacked UString's header; then validator found
empty names from a copying helper. Borrowed output UString fixes that mapping.
Default-build regression/checks follow; DAW/other-format mapping remains work.

W77 default flag-off and programme-enabled bundles both pass SDK 47/47 and
three-cycle Cocoa editor probes; final programme-enabled bundle also passes
36-block/six-activation programme contracts. Twenty Python checks, whitespace
and documentation pass. No Rust API changed beyond W76's verified catalogue.

## W78: Optional actual-component AUv2 factory programs

Added a local AU subclass over the optional nested VST3 catalogue, correcting
endpoint normalization and queuing complete controls with coherent host bypass.
Block-start ID3 selector supports nearest/ties-up; nonzero offsets reject.
Three actual component lifecycles pass names, repeated/reversed factory selection,
custom metadata preservation, parameter endpoints/ties, invalid indices and
ClassInfoFromDocument recall across 420 render blocks. Initial fixture wrongly
rejected negative custom metadata accepted by AUBase; corrected preservation
assertions pass. External SDK sources remain untouched. Default-disabled and
final enabled builds pass AU audio/state/tail and three-cycle editor probes;
embedded VST3 SDK validator passes 47/47 and enabled program contracts pass.
Twenty Python checks, documentation and whitespace pass. No auval/DAW menu claim.

## W79: Prepared downward-expansion noise gate

Added finite-floor explicit-sidechain f64 gating over the rectified detector,
with prepared ratio/threshold/quadratic knee and reset-preserved configuration.
Three groups pass independent dB/knee/endpoint, detector exponential timing,
invalid settings and 10000 heap-free sample/reset iterations. Initial formatting
caught fractional literals without Rust's leading zero; corrected syntax passes.
All-feature workspace passes 321 tests/five doctests, clean Clippy/Rustdoc,
formatting, whitespace and documentation. No native/deadline gate claim.

## W80: Prepared low/high shelving filters

Added separate shelf-kind/slope design semantics over stable normalized biquads.
Three groups pass endpoint/half-dB midpoint, reciprocal response and monotonic
512-bin grids over both sides/three frequencies/three slopes/five gains, plus
boost/cut impulse cancellation, partition identity, reset and heap-free loops.
All-feature workspace passes 324 tests/five doctests, clean Clippy/Rustdoc,
formatting, documentation and whitespace. Higher-order composition remains work;
no numerical-all-corners, headroom or native deadline claim.

## W81: Fixed cascades and even-order Butterworth designs

Added fixed 1..16 section coefficient/state arrays, explicit arbitrary sections
and low/high pass orders 2..32. Three groups pass all-order independent warped
response/cutoff/endpoints, rendered cutoff RMS, partition identity, reset and
changed-coefficient history against independent processors, invalid bounds and
heap-free loops. Signal tests, workspace Clippy, five doctests, warning-denied
Rustdoc, formatting, documentation and whitespace pass. W80's workspace remains
the preceding full-workspace evidence; W81 does not claim a new full run.
Odd-order/band/family/order-selection and native deadlines remain separate work.

## W82: Prepared sample-peak limiter

Added one-channel instantaneous peak attack/exponential release limiting with
prepared ceiling and final symmetric rounding clamp. Three groups pass extreme
finite amplitudes/both polarities, unity/memoryless reference, independent release
recurrence, retrigger, partition/reset, invalid/nonfinite recovery and 100000
heap-free samples. Signal tests, workspace Clippy, five doctests, warning-denied
Rustdoc, formatting, documentation and whitespace pass. No true/intersample-peak,
linked-channel, distortion or native deadline guarantee.

## W83: Caller-scheduled exact-device output recovery

Added explicit output recovery/fresh-processor ownership without fallback,
settings changes or implicit polling. Controlled transitions cover missing,
unhealthy, failed-open retry, healthy preservation, disable/reopen/drop and
constructor validation without factory/native work. UMC1820/macOS arm64 20-output
96 kHz three generations each pass 49 callbacks/25088 frames with zero faults/
backend errors/panics and exact control-thread prepare/deactivate counts.
Explicit reopen/disable/reenable exercised; physical removal and real backend
faults remain unvalidated. Device tests, workspace Clippy, five doctests,
warning-denied Rustdoc, formatting/documentation/whitespace pass. Panic-only
fixtures initially needed error inference; then Clippy requested removing Copy
config clones. Those corrections pass. Duplex recovery remains separate work.

## W84: Caller-scheduled exact-pair duplex recovery

Shared stop-before-open sessions now own fixed matching-rate input/output pairs
with fresh FIFO/DSP. Constructor bounds validate without factory/native work.
Actual UMC1820/macOS arm64 18-input/20-output 96 kHz missing exact input/output
IDs wait without opening. Three manual restart generations pass output 49
callbacks/25088 frames each; input 50/50/49 callbacks and 25600/25600/25088 frames.
Dropped/invalid/backend/DSP/panic/underflow counters all zero in this fixture;
exact control-thread prepare/deactivate counts pass. Device tests, workspace
Clippy, five doctests, warning-denied Rustdoc, formatting, docs and whitespace
pass. Physical disconnect/real faults/drift/other-platform/deadline unvalidated.

## W85: Optional local-font horizontal run shaping

Added owned local font/face and explicit directed single-run shaping through
optional pinned HarfRust 0.14.0, with byte clusters/positions/advances/missing
coverage and bounded input/output profiles. Public unmodified Noto Arabic fixture
retains exact upstream/hash/OFL; no proprietary asset is included. Three portable
groups and eight independent installed HarfBuzz 14.4.0 runs pass, including Arabic
joining/marks/joiners, Latin and explicit kerning disable. All-feature workspace
passes 336 tests/five doctests; clean Clippy/default+all-feature Rustdoc, 20 Python
checks, formatting/docs/whitespace pass. Clippy requested collapsing language
validation; corrected check passes. Default umbrella tree omits text dependencies.
Backend transient memory/time, paragraphs/fallback, outlines/raster/native/GPU/
accessibility and broad font/script/platform support remain separate work.

## W86: Typed SVG paths and local TrueType glyph outlines

Added typed finite path topology/curves and optional default glyf extraction;
shaped SVG text renders local glyph paths with baseline/Y conversion and whole-
run bytes/count rollback. Surface default/all-feature tests, workspace Clippy,
five doctests, warning-denied default/all-feature Rustdoc, formatting/docs/
whitespace pass. Initial OutlinePen import needed its public outline submodule.
The XML fixture initially expected 24 paths; 26 glyphs less one empty space
correctly yields 25. Actual 640x240 SVG is 18529 bytes/no text/font URLs; browser
getBBox checks all ink within viewport. Preview screenshot/text snapshots fail
on its client; independent librsvg PNG inspection shows readable Latin, joined
Arabic/marks. This is vector/fixture evidence, no browser screenshot/native GPU/
raster API/accessibility claim. CFF/color/bitmap/variable-instance/paragraph and
other renderer/profile work remain independent unfinished work.

## W87: Optional single-line Unicode bidirectional ordering

Added pinned Unicode 16.0 ordering, byte levels and logical ranges in visual
order, plus shared single-line separator validation for shaping. Three groups
pass 32 unchanged official reference vectors, isolates/marks/UTF8 coverage,
auto/forced base, empty lines and exact capacity. Surface default/all-feature
tests, workspace Clippy, five doctests, warning-denied default/all-feature
Rustdoc, formatting/docs/whitespace pass. Controlled Arabic/Latin/numbers SVG
has 26 paths/23289 bytes and no text/font URLs; browser geometry lies within
640x180 and independent librsvg PNG inspection passes. No full Unicode suite,
script/fallback/paragraph/caret/accessibility or native renderer claim.

## W88: Prepared simultaneous-output linear SVF

Added bounded one-channel f64 trapezoidal low/band/high output processing and
history-preserving prepared coefficient replacement. Three groups pass static
biquad impulse comparisons/weighted reconstruction, changing coefficients versus
independent nodal solves, exact bounds/nonfinite reset and heap-free loops.
Signal tests, workspace Clippy, five doctests, warning-denied default/all-feature
Rustdoc, formatting/docs/whitespace pass. No nonlinear/self-oscillating profile,
arbitrary modulation stability or native deadline guarantee.

## W89: Prepared damped synthetic mono reverb

Added four parallel lowpass-feedback combs/two canonical allpasses with bounded
prepared storage and live wet mixing. Three groups pass independent time-indexed
impulse/noise references, first arrival/finite fixture decay, partition/reset,
dry-retained histories, heap-free loops, maximum lengths and nonfinite recovery.
Signal tests, workspace Clippy, five doctests, warning-denied default/all-feature
Rustdoc, formatting/docs/whitespace pass. Clippy requested compound assignment
in the independent reference; corrected pass. No perceptual quality/physical-room/
exact overall T60/stereo/other-platform/native deadline claim.

## W90: Real half-spectrum FFT and prepared cosine windows

Added N/2+1-bin real transforms backed by existing full complex FFT scratch,
plus five explicit symmetric/periodic window families and sampled gain/energy/
noise bandwidth. Three groups pass direct DFT/roundtrip/Parseval/endpoints,
window landmarks/analytic gains, coherent-bin correction, atomic rejections and
heap-free application. Signal tests, workspace Clippy, five doctests, warning-
denied default/all-feature Rustdoc, formatting/docs/whitespace pass. Initial
mirror indexing needed a local index to avoid overlapping Rust borrows. No
real-only optimization/SIMD/overlap-add/native deadline claim.

## W91: Serialized native gain meter records

Added complete fixed C-layout f32/f64 block meter reads under serialized DSP
ownership. Actual static-library C++ validates layouts and 38 blocks across
mono/stereo/rates, f64 precision/large RMS, counts/diagnostics, empty/fault
retention/reset and null rejection. All three native audio/program/meter targets
pass. Native Rust tests, default/all-feature workspace Clippy, five doctests,
warning-denied default/all-feature Rustdoc, formatting/docs/whitespace pass.
Initial generic reading closure needed direct type-inferred array maps. A default
build exposed an unused text-only glyph helper; cfg-gated it and default checks
pass. Editor delivery remains separate work; no concurrent DSP UI read/deadline.

## W92: Native VST3 host-mediated meter delivery

Native editor builds add twelve hidden/read-only/nonautomatable meter parameters,
processing-owned snapshots and Rust UI JSON/four local meter controls. Actual
exported f32 processor-to-host-to-controller-to-WKWebView cycles pass rendered
RMS/diagnostic/accessible text; SDK-side f32/f64 metadata/queues/no-queue hosts
pass. Default DSP-only and final editor/program builds each pass SDK 47 tests;
program36-block/six-activation and editor lifecycle/edit contracts pass. Native
image inspection shows readable four-meter UI. Rust/default+all-feature Clippy,
five doctests, warning-denied default+all-feature Rustdoc, 20 Python checks,
formatting/docs/whitespace pass. Initial C++ fixture newline was corrected;
Clippy's constant-chunk suggestion was resolved with a fixed four-value array.
No REAPER/assistive-tech/complete-block UI atomicity/deadline claim. Hosts must
forward output parameters; absent/rejected queues drop telemetry without DSP fault.

## W93: Actual AUv2 native meter transfer

Shared editor telemetry traverses the unmodified SDK output transfer/main-thread
timer. Actual loaded component passes twelve readable/not-writable metadata and
normalized values, four displayed RMS/gain changes, injected input/output
nonfinite diagnostics/recovery and three editor cycles. Shown native PNG
inspection passes. Component audio/state/tail, factory420 blocks, nested VST3
SDK47/program36 blocks/six activations and editor regressions pass. Metadata
CF names are released. Documentation/whitespace checks pass. Host bypass and
stopped/empty/fault renders retain prior meters; no complete-block UI atomicity,
DAW/auval/assistive-tech/timer deadline or other platform/architecture claim.

## W94: Native REAPER meter harness, host run pending

Added public-marker/empty-project guards before logging/host mutation, silent
hardware routing and twelve meter comparisons. Three evidence-check groups pass,
including installed-Lua missing/incorrect marker/nonempty project protection.
The guard test caught a missing marker path separator; corrected before host execution.
Restarted only the named isolated REAPER copy to load the new binary. CUA still
returns cgWindowNotFound; a thread sample confirms startup modal state before
script logging. Owner readiness is pending while independent work continues.
No actual REAPER meter-forwarding, visual or assistive-tech claim. The prior
session/automation/render evidence remains historical and distinct.

## W95: Bounded live sinc rate adjustment

Added one-percent-maximum prepared variable-rate range/conservative cutoff,
history/pending-position-preserving setters and reset retaining adjustment.
Three groups pass piecewise partition/DC/continuity, independent100000-input
counts at drift/extreme settings, reset/fresh history, bounds/atomic rejection,
pending position and heap-free changes. Fixed resampler regressions, signal tests,
workspace Clippy, five doctests, warning-denied default/all-feature Rustdoc,
formatting/docs/whitespace pass. No automatic FIFO controller, physical clock
compensation, broad spectral or native deadline claim; integration continues.

## W96: Bounded FIFO-depth clock controller

Added fixed-state smoothed proportional/clamped/slew-limited ppm feedback with
explicit occupancy/time/configuration bounds. Three groups pass independent
30000-callback positive/negative drift and jitter models, expected equilibrium,
smoothing/slew/saturation/reset, variable-resampler coupling, atomic rejection
and heap-free loops. Final clamp protects correction bounds against rounding;
fractional/extreme bounds pass. Signal tests/workspace Clippy, five doctests,
warning-denied default/all-feature Rustdoc, formatting/docs/whitespace pass.
Physical clocks, arbitrary configuration stability and native duplex integration
remain separate; integration is continuing.

## W97: Prepared and native clocked duplex conversion

Added opt-in raw FIFO/controller/per-channel sinc conversion into the existing
output-clock duplex processor, preserving matching-rate behavior. Signed +/-500
ppm models with 6000 variable callbacks each preserve stereo/DC and bounded
occupancy without underflow; priming/starvation/reprime, malformed-frame rejection,
callback counts, deactivation, pre-prepare rejection and 1000 heap-free workloads
pass. All ravet-device tests, default/all-feature workspace Clippy, five doctests,
default/all-feature warning-denied Rustdoc, formatting/docs/whitespace pass.

On macOS 27.0.1 arm64, actual explicit UMC1820 18-input 96 kHz to Mac mini Speakers
2-output 48 kHz f32 streams pass three fresh silent pairs and pause/resume with
exact control-thread preparation/deactivation. Three additional ten-second pairs
each render 938 callbacks/480256 frames before resume, with zero dropped capture,
underflow, invalid-buffer/backend/DSP/panic counts. Final queue depths are
2292/2051/2051 and corrections 1408.5447/16.2635/16.3185 ppm; initial priming is
0/0/512 output frames. Earlier short pairs reached the 2000 ppm configured limit.
Only counters persist, with no input recordings or audible output. These bounded
runs validate this conversion/lifecycle path, not physical phase alignment,
long-session clock estimation/stability, arbitrary settings, hotplug/recovery,
other devices/platforms or deadlines. W94 REAPER remains pending its startup modal.

## W98: Caller-scheduled clocked duplex recovery

Added exact-ID clocked recovery sharing existing session release/retry/generation
ownership. Invalid configuration preflight now precedes direct capture opening;
sinc rate/tap/ppm preflight shares preparation bounds without table allocation.
Constructor/disabled-service no-factory checks, shared session ordering tests,
complete device/signal tests, default/all-feature workspace Clippy, five doctests,
default/all-feature warning-denied Rustdoc, formatting/docs/whitespace pass.

Actual UMC1820 18-input 96 kHz to Mac mini Speakers 2-output 48 kHz silent pairs
on macOS 27.0.1 arm64 pass absent exact input/output IDs without factory work,
manual reopen and disable/reenable across three generations. Output callbacks/
frames are 51/26112, 48/24576, 51/26112; capture drops, conversion underflow and
invalid-buffer/backend/DSP/panic counters are zero. Exact three control-thread
prepares/deactivations pass. No input recordings persist. This validates the
manual recovery paths; physical unplug, real backend faults, long-session drift,
other devices/platforms and deadlines remain unvalidated. W94 remains pending.

## W99: Bounded hosted CLAP audio-owner lifecycle handoff

Added main-activated/stopped token proposals, private non-Send/non-Sync running
ownership, audio-owner start/process/reset/stop and bounded stopped retirement.
Initial/candidate start failures preserve ownership/current processing; full
retirement prevents candidate consumption; abandoned control preserves current
and pending ownership. Explicit finish/drain returns all tokens for matching
main-thread deactivation. Graph/device routing remains separate.

Linked actual synthetic callbacks pass thread-ID checks across six instances,
initial/candidate failure, backpressure, full/gone offers, reset, 6000 tagged output
blocks and exact main deactivation/destruction counts. Linked gain additionally
retains audio/pending ownership after control abandonment. Successful swaps and
rendering pass heap instrumentation; compile-fail Rustdoc enforces non-Send running
ownership. Actual RAVET Gain and prepared upstream Clack Gain bundles each pass
2000 stereo f32 blocks/32000 frames across two audio-owner starts/stops and a
bounded swap with zero measured allocations/main deactivation on macOS arm64.

All host tests, default/all-feature workspace Clippy, six doctests, default/all-
feature warning-denied Rustdoc, formatting/docs/whitespace pass. The only lock
change adds already-existing rtrb 0.4.0 (MIT OR Apache-2.0) to ravet-host's direct
dependencies. No general plugin compatibility, port/config/event routing, native
device scheduling, crash isolation, physical deadlines or other-platform claim.
W94 REAPER remains pending its startup modal; independent graph integration continues.

## W100: Explicit graph audio-owner lifecycle sessions

Added optional no-op processor start/stop hooks and non-Send/non-Sync borrowed
static/dynamic graph sessions. Nodes start topologically and stop in reverse;
failed/error/unwind starts roll earlier nodes back. Dynamic failed candidates
retire while the old graph renders; successful old nodes stop before control
retirement. Default portable rendering stays unchanged. Active preparation
rejects before changes; guards must end before control transfer/deactivation.

Three session groups pass owner-thread/reverse-order/error/panic rollback,
invalid-input/backpressure/retained rendering, control-only deactivation/drop,
heap-free swaps/rendering and native f64 default-hook precision. Foundation/graph
regressions and full all-feature workspace acceptance pass (375 tests including
doctests); default/all-feature workspace Clippy, eight doctests, default/all-
feature warning-denied Rustdoc, formatting/docs/whitespace pass. These are lifecycle
mechanics and offline evidence; hosted CLAP node port/native buffer mapping and
device scheduling remain dependent implementation work, without platform/deadline
claims. W94 REAPER remains pending its startup modal.

## W101: Prepared hosted CLAP f32 main audio graph node

Added main-owned instance/activation and a Send prepared native graph node for
explicit owner sessions, matching zero/one main port per direction with bounded
f32 scratch and stopped-token return. Fixed-config/shape rejection retains main
instance ownership before activation; main audio start rejects before native
callbacks. Declared graph timing is explicit. Node-local clocks, deferred stopped
reset, success-status telemetry, backend nonrollback/silencing and one-shot
retirement/inactive instance recovery are documented. Core callback service is
explicit/main-owned, without implicit restart/polling.

Four linked native fixture groups pass actual main/audio thread IDs, cold/warm
prepare, shape/phase/active rejection, malformed/empty blocks, partial native
runtime fault silence, retained status/sleep output initialization, steady time/
reset, callback service, dynamic wet/dry swap, stop before control graph/instance
retirement, exact deactivation/destruction and prepared heap checks. Actual RAVET
Gain and prepared upstream Clack Gain bundles each pass 2000 stereo f32 graph
blocks/32000 frames and main stopped retirement with zero measured allocations
on macOS arm64, under explicit known zero-latency/no-tail gain probe profiles.

Host/graph/foundation regressions, default/all-feature workspace Clippy, nine
doctests, default/all-feature warning-denied Rustdoc, formatting/docs/whitespace
pass. Only existing internal foundation and development graph packages were added
to host dependencies. Native events/transport/f64/auxiliary/editor/active parameter
mapping, other format/device scheduling, broad plugin/platform/deadline evidence
remain dependent work. W94 REAPER remains pending its startup modal.

## W102: Native f64 hosted main audio graph nodes

Added a separate f64 factory/node with typed double scratch/native ABI buffers and
advertised-port checks before activation. Shared private lifecycle retains f32
behavior and one-shot main/control retirement. f64 scratch is bounded to 64 MiB
outside the graph cap. Public memoryless stereo x2 cdylib fixture supports both
formats without parameters/state/UI/events/assets.

Five linked node groups pass; f64 retains sub-f32 increments, 1e200/tiny values
and exact routed outputs through 1000 heap-free owner blocks. Loaded public
fixture passes 2000 blocks/32000 frames per precision, native wet/dry dynamic
swap/stop/main retirement and zero measured allocations on macOS arm64. Existing
own/upstream gain bundles repeat f32 acceptance; actual Gain f64 request rejects
with Precision because its CLAP ports remain f32 only.

Host/graph/foundation regressions, default/all-feature workspace Clippy, nine
doctests, default/all-feature warning-denied Rustdoc, formatting/docs/whitespace
pass. No new external dependency. Native multiport/events/transport/editor/active
parameter/device mapping and broader format/platform/production work continue.
W94 remains pending its REAPER startup modal.

## W103: Explicit packed native CLAP audio ports in graphs

Added separate f32/f64 packed-port factories while preserving original main-only
validation. Native input/output port boundaries remain intact in prepared buffers;
graph routes address directional native enumeration/channel order. Summed layout
checks precede activation; nine ports and 64 total channels per direction bound
storage. Foundation auxiliary buses stay empty; pairing/negotiation is explicit.

Eight linked node groups pass, including separate stereo auxiliary input/output
mapping, distinct per-port processing, wide f64 values, summed-layout/auxiliary
precision rejection before activation, owner retirement and heap instrumentation.
Actual exported Sidechain CLAP passes 2000 blocks/32000 frames per precision,
alternating detector open/closed samples, native wet/dry routing, dynamic swap/
stop/main retirement with zero measured allocations on macOS arm64. Loaded dual-
format fixture and own/upstream f32 gain regressions also pass.

Host/graph/foundation regressions, default/all-feature workspace Clippy, nine
doctests, default/all-feature warning-denied Rustdoc, formatting/docs/whitespace
pass. No dependency changes. Native events/transport/editor/active parameter/
device scheduling and broader format/platform/production work continue. W94
REAPER remains pending its startup modal.

## W104: Opt-in hosted CLAP block-start transport

Added checked allocation-free portable-to-native transport and stopped main-thread
node opt-in for both precisions. Disabled behavior stays unchanged. Any known
context requires known playing; tempo/beat/seconds flags advertise only known
fields. Native fixed-point bounds/quantization and f64 sample conversion are
explicit. Invalid context clears output before native calls/history/steady-time
changes. Discontinuity has no native flag and does not reset DSP.

Nine linked node groups and three transport groups pass known/absent context,
negative positions, positive tempo, invalid/missing/range rejection, native clock/
call preservation, owner checks and heap instrumentation. Loaded public fixture
validates native context and alternates x2/silent output from playing/stopped flags
across 2000 graph blocks/32000 frames in each precision, wet/dry dynamic swaps,
main retirement and zero measured allocations on macOS arm64. Main-only fixture,
own/upstream f32 gain and packed f32/f64 sidechain regressions repeat acceptance.

Host/graph/foundation regressions, default/all-feature workspace Clippy, nine
doctests, default/all-feature warning-denied Rustdoc, formatting/docs/whitespace
pass. Full all-feature workspace acceptance also passes (388 tests including
doctests). No dependency changes. Native input/output events/editor/active parameters/
device scheduling, richer transport and broader format/platform/production work
continue. W94 REAPER remains pending its startup modal.

## W94 follow-up: Actual REAPER native meter forwarding

After the owner reported ready, CUA successfully inspected the isolated empty
REAPER main window, loaded the guarded public native-meters.lua through the
Actions list and ran it. REAPER 7.82/macOS arm64 discovered the editor-enabled
RAVET Gain VST3, muted the test hardware destination, loaded stereo synthetic
input and returned all twelve meter IDs. Observed normalized peaks/RMS were
0.0625/0.125 input and 0.125/0.25 output, invalid counters zero, within 0.0002.
Native editor floating-window handle passed; playback stopped and the editor
closed. Latest main window inspection retains only the disposable unsaved project.

Evidence: target/reaper-native/config/ravet-validation/native-meter-host.log.
Earlier modal/cgWindowNotFound observations remain historical. This closes the
actual DAW parameter-forwarding harness run; no visual/assistive-tech/other-host/
platform/deadline claim. Implementation programme continues independently.

## W105: Prepared hosted graph global parameter input

Added explicit f32/f64 parameter factories selecting main-only/packed native audio,
1..4096 bounded input slots and unique product-to-native identity mappings.
Current writable metadata/ranges/steps inspect on main before activation. Strict
whole-block unknown/other-kind/range/step rejection precedes native calls. Ordered
physical f32 values widen to native f64 with global targets/empty cookies. Audio
zero-frame flush dispatch retains process/steady counters; fixed snapshots require
fresh preparation after metadata changes. Native output remains void here.

Four linked parameter groups pass pre-activation metadata/capacity/readonly/
ambiguous-ID rejection, actual aliases/targets/cookies, same-frame last-event
ordering, wide f64 buffers and 4000 malformed blocks per precision preserving
native gain across subsequent rendering. Each precision renders 6000 native
blocks/48000 frames plus 1000 active zero-frame flushes with zero measured heap
actions. Linked real Gain independently passes 240-sample smoothing, sample-timed
bypass and empty-flush target/state checks. Loaded public stepped gain fixture
passes 2000 graph blocks/32000 frames and two empty flushes per precision, dynamic
wet/dry swaps/owner stop/main retirement with zero measured allocations on macOS
arm64. Existing loaded gain/sidechain/transport regressions retain acceptance.

Host/graph/foundation tests, default/all-feature workspace Clippy, nine doctests,
default/all-feature warning-denied Rustdoc, formatting/docs/whitespace pass. No
new dependency. Native output/MIDI/per-note/gesture/editor/device integration and
broader format/UI/platform/production work continue. W94 host forwarding is now
completed by the follow-up above; prior startup-modal observations are historical.

## W106: Native global parameter output and preparation builder

Added a builder selecting independent fixed port/transport/input/output profiles,
preserving original factories. Main validates output identities/ranges and permits
read-only native parameters. Typed global output narrows to finite/range/step-valid
portable f32 with bounded slots, offset/order checks and no cookie/pointer retention.
Unmapped/unsupported/per-note output filters and counts. Mapped runtime rejection
clears whole audio/event blocks with nonrollback BackendFailure; successful output
gates success counters/status. Output-only zero-frame profiles dispatch audio flush.

Ten linked graph-node and eight parameter groups pass, including before-activation
output capacity/identity checks, exact outer capacity, output-only zero flush,
readonly aliases and builder packed/f64 transport. Twelve native output modes per
precision each run 1000 heap-instrumented blocks: normal, range/NaN/offset/order/
capacity faults, unmapped/MIDI/per-note filters, f64 rounding, f32 nonrepresentable
and stepped rejection. Rejected output still leaves native gain advanced, verified
by main parameter read after orderly retirement. Native callbacks measure no heap
actions. Existing parameter input/real Gain regressions retain acceptance.

Loaded public fixture passes 2000 graph blocks/32000 frames and two empty flushes
per precision, readonly events at exact final-frame/zero offsets through host/node
graph edges, dynamic wet/dry swaps/owner stop/main retirement, with zero measured
allocations on macOS arm64. Previous input/transport/sidechain/own/upstream gain
loaded regressions pass. Host/graph/foundation tests, default/all-feature workspace
Clippy, nine doctests, default/all-feature warning-denied Rustdoc, formatting/docs/
whitespace pass. No dependency change. Native note/MIDI/per-note/modulation/gesture/
editor/device integration and broader formats/UI/platform/production remain work.

## W107: Selected native note/MIDI graph input

Added explicit inactive native note-input index/dialect selection to the f32/f64
preparation builder. Shared bounded input storage preserves mixed global
parameters and native CLAP notes, MIDI/MPE or framed UMP. Whole-block validation
rejects dialect/kind/value and zero-frame performance before any native mutation.
CLAP notes use wildcard note IDs, MIDI velocity quantizes, and UMP words/groups
remain unchanged without inferred translation. Note-only profiles need no params
extension; original factories retain their strict contracts.

Codec tests cover targets, short-message padding, zero velocity quantization and
one/two/four-word UMP preservation. Four linked note groups cover index-versus-ID,
missing extension, unsupported dialect/capacity, all four dialects per precision,
same-frame parameter/note order and whole-prefix rejection including empty
blocks. Each mixed profile runs 2000 successful blocks plus 2000 rejected calls,
without measured heap actions and with native gain unchanged after rejection.
The linked real sine note-only node checks timed sound and release silence.

Loaded public fixture passes 2000 graph blocks/32000 frames and two parameter
flushes per dialect/precision, through event edges, wet/dry routes, dynamic swaps
and main retirement. Loaded RAVET Sine passes 1000 no-input stereo blocks/4096000
frames, delayed onset, release silence and zero measured allocations. Evidence
is macOS arm64 synthetic offline processing; no device playback or broader host,
platform or deadline claim. An initial probe filtered notes at its parameter-only
edge; selecting the explicit all-event edge corrected the probe and all eight
loaded profiles pass.

Full all-feature workspace tests pass 403 tests including doctests. Default and
all-feature workspace Clippy, nine default doctests, both warning-denied Rustdoc
builds, formatting, documentation and whitespace checks pass. No new dependency;
enabled the pinned Clack note-ports feature. Native performance output, per-note/
modulation/gestures/editor/device and broader format/UI/platform/production work
continue; this is not overall programme completion.

## W108: Selected native performance graph output

Added independent inactive native note-output index/dialect selection, sharing
bounded output slots with optional global parameters. Concrete CLAP notes with
wildcard IDs narrow normalized velocity to portable f32; short MIDI/MPE and framed
UMP retain payload/group with unused native padding ignored. Unsupported ports,
dialects and unrepresentable note targets/identities filter explicitly. Selected
invalid values/statuses, offsets/order/capacity and zero-frame performance output
poison the block with nonrollback BackendFailure and clear audio/event prefixes.
Performance-only profiles need no params mapping/extension. Metadata stays fixed.

Two codec groups cover concrete/wildcard/specific targets, velocity narrowing and
invalid ranges, short MIDI padding/status/data, preserved one/two/three/four-word
UMP and dialect filtering. Seven linked note groups pass, including output-only
metadata/index-versus-ID, malformed MIDI/velocity, overflow, filtered ports/note
IDs, mixed parameter output, zero-frame flush and advanced native state after
output rejection. Both precision fault groups run seven profiles of 1000 blocks
plus flush; output-only runs 1000 blocks. No measured callback heap actions.

Loaded public echo fixture passes all four dialects in f32/f64, 2000 graph blocks/
32000 frames and two parameter flushes per profile, exact performance offsets then
read-only gain through all-event graph edges, wet/dry live swaps and main
retirement. macOS arm64 synthetic processing only, without device playback or
broader host/platform/deadline claims. Existing parameter/input/sine regressions
retain acceptance. Host/graph/foundation tests, default/all-feature workspace
Clippy, nine default doctests, both warning-denied Rustdoc builds, formatting/docs/
whitespace pass. No new dependency. Per-note identities/expressions/modulation/
gestures/editor/device and broader formats/UI/platform/production remain work.

## W109: Main-owned editors for hosted graph nodes

Retained native graph control now creates the existing exclusive-borrowed editor
while the separate audio node/graph runs on its owner. Borrowing prevents main
reclaim/drop until GUI destruction, with a compile-fail ownership example.
The editor guard services at most one coalesced core callback per consumer tick,
preserving restart/process flags and deferred re-requests. Native parent, OS main
thread/event loop and further GUI/parameter/restart scheduling stay explicit.

Linked absent-extension rejection preserves usable control/audio ownership. Actual
RAVET Gain and upstream Clack Gain EGUI bundles each pass three embedded editor
create/parent/show/resize/hide/drop cycles while processing 3000 graph blocks/
48000 frames, exact synthetic stereo unity output and zero measured audio heap
actions. Deliberately coalesced callback service preserves restart/process flags;
main child cleanup, owner stop and inactive instance return pass. macOS arm64
only, no device/DAW/focus/accessibility/platform/deadline claim. The initial probe
prepared its node twice; allowing graph preparation to own that transition fixed
the probe. Corrected warning-free native runs pass for both bundles.

Host/graph tests, default/all-feature workspace Clippy, ten default doctests,
both warning-denied Rustdoc builds, formatting/docs/whitespace pass. Tool package
now uses existing graph/foundation/allocation-counter dependencies; lock metadata
updated, with no new external package. Device callback owner integration and
per-note/expression/modulation/gestures, broader formats/UI/platform/production
remain independent work; overall programme is still active.

## W110: Buffered f32 graph-worker device output

Added control-prepared no-input graph output pumping through an existing SPSC
queue. Its borrowed non-Send worker session starts/stops graph/native nodes on
the worker, retains exact whole-frame backpressure suffixes without rerendering,
and returns stopped storage for main retirement. RingOutput provides bounded
queue consumption, invalid-block dequeue preservation, underflow silence and
relaxed live source/underflow diagnostics. Worker/device rate/priming/wake/join,
clocking and recovery remain explicit; pump has no wait/thread/I/O behavior.
Worker/consumer scratch caps are 32/16MiB outside graph/native/queue caps.

Five portable groups pass constructor rejection, partial/full retention and
frame order, 1000 heap-free pump/consume blocks, graph fault/no-publication/reset,
underflow and malformed dequeue preservation. Worker stop precedes main
retirement, with a compile-fail non-Send session example. The initial lifecycle
assertion omitted graph preparation's main deactivate; corrected counts pass.

The loaded public Sine silent probe passes 256000 consumed offline frames,
counting 17408 underflow frames during deliberately unclocked consumption,
without processing/backend/panic failures. The explicit UMC1820/macOS arm64
20-channel 96kHz silent native run passes 10 seconds, 1875 callbacks/960000
consumed frames, zero queue underflows/processor faults/backend errors/panics,
964352 graph-rendered frames and orderly worker stop/main graph/native return.
Graph pump heap instrumentation passes; offline device rendering also measures
no heap actions. This is one short silent hardware observation, not clock,
long-session/drift/hotplug/audible/deadline or broad device/platform support.

Device/host/graph tests, default/all-feature workspace Clippy, eleven default
doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace pass.
Device/tool packages use existing graph/host/queue/Clack/allocation-counter
packages; lock metadata updated without new external packages. Device probe
keeps its previous default CLI via default-run, with narrow trusted native-load
unsafe allowance under deny policy. Input/events/dynamic graph/device recovery,
per-note/modulation/gestures and broader formats/UI/platform/production remain
independent work; this prototype does not complete the programme.

## W111: Bounded graph-worker event input acknowledgement

Buffered graph workers now permit 0..4096 explicit input slots and a borrowed
ordered stream for each fresh fixed render. Progress acknowledges consumption
only after a successful new render; retained suffix pumps leave new input/context
untouched, avoiding repeated parameter/note delivery under backpressure. Outer
validation rejects malformed input before nodes; errors retain nonrollback graph
semantics and are not safe automatic delivery retries. No timestamp/clock/late
retiming or asynchronous event scheduler is inferred; output capacity stays zero.

Six graph-output groups pass, including once-only offset parameters across full/
partial queue publication, false pending acknowledgements, malformed outer input
without node mutation/publication, existing order/underflow/fault/reset/retirement
and measured heap-free pumping. Loaded public Sine offline note mode passes
selected native MIDI input through the queue, delayed onset/finite stereo sound,
release silence and 256000 consumed frames. Corrected final probe reports 16384
underflow frames during deliberately unclocked offline consumption, no faults/
backend errors/panics, zero measured worker/render heap actions and main return.
Silent offline mode retains acceptance. Notes mode rejects device arguments;
physical output remains silent. This is not audible hardware/MIDI/clock evidence.

Device/host/graph tests, default/all-feature workspace Clippy, eleven default
doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace pass.
No dependency change. Worker output events/async control queues/input/live graph
edits/recovery, per-note/modulation/gestures and broader formats/UI/platform/
production remain work; overall programme remains active.

## W112: Caller-owned generated events from graph workers

Buffered output workers now permit 0..4096 output slots with explicit graph/node
routes. pump_io accepts exact caller-owned capacity and returns a valid event
prefix once per successful fresh render, regardless of partial/zero immediate
queue publication. Pending suffix calls leave storage untouched and return zero
output count. Empty-output convenience methods remain; positive output capacity
requires pump_io. Event offsets describe render blocks, without inferred device
clock, audible correlation or control/GUI delivery. Errors return no valid prefix
or faulty audio publication, without rolling back prior native histories/input.

Seven graph-output groups pass, adding absent/excess output capacity rejection
before node execution, exact generated offset/prefix, no output replay across full/
partial pending publication and measured heap-free pumping. Loaded public native
parameter fixture passes 1000 two-node worker graph blocks/8000 frames, stepped
sample input/read-only output through explicit graph edges, a four-frame queue
retaining eight-frame blocks, once-only telemetry, exact audio and zero measured
heap actions. Owner stop/main graph/native instance return pass. This synthetic
macOS arm64 probe opens no device/GUI and does not promise clocks/deadlines.

Device/host/graph tests, default/all-feature workspace Clippy, eleven default
doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace pass.
No dependency change. Cross-thread input/output control queues, live graph/input/
recovery, per-note/modulation/gestures and broader formats/UI/platform/production
remain work; overall programme remains active.

## W113: Complete bounded graph-worker/control event queues

Added fixed whole-batch SPSC event queues for explicitly scheduled control input
or worker output. Preparation bounds 1..65536 frames, 0..4096 events and 1..64
queued batches, with <=32MiB queue values plus one retained consumer batch. Safe
pinned rtrb chunk APIs atomically publish headers/tags/context/events/unused slots.
Full/invalid input publishes no prefix; observed disconnect rejects. A borrowed
batch remains pending until explicit acknowledgement after consumption/discard;
dropping its borrow cannot replace it. Tags/context stay opaque, with no native
transport/clock/retiming/replay or automatic scheduler inferred.

Three queue groups pass fixed capacity/full rejection, retained/empty batches,
context and mixed parameter/note/MIDI/UMP/same-frame order, malformed/disconnect
and 10000 cross-thread publish/read/ack rounds with zero measured heap actions on
both owners. Loaded public parameter fixture passes 1000 complete main/worker
input-output rounds/8000 native frames, exact mapped read-only telemetry/tags,
retained audio suffixes/no repeated native input, zero measured worker heap actions
and owner stop/main retirement. Corrected final probe handles control teardown
while waiting. Synthetic macOS arm64 only, without GUI/device/clock/deadline claim.

Device/host/graph tests, default/all-feature workspace Clippy, eleven default
doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace pass.
Device now directly uses already pinned rtrb 0.4.0; lock metadata updated without
new external package. Async event queues are usable primitives; actual GUI/device
clock/lifecycle policy, live input/edits/recovery, per-note/modulation/gestures and
broader formats/UI/platform/production remain independent work.

## W110–W113 event-capacity correction

Review during W114 found that earlier W111/W112 observations incorrectly stated
4096 usable graph-render events per direction. Graph rendering has always capped
input/output at 1024; the worker now checks the shared public graph limit before
node preparation. Current guides/API/catalogue are corrected. Independent complete
batch queues retain their 4096-slot limit, and callers must fit receiving graph
profiles. The small native probes above never exercised the overstated range;
their measured evidence remains unchanged. This correction does not increase
render capacity or claim new host/device support.

## W114: Explicit parameter gesture boundaries

Added typed stable-ID global begin/end events, parameter/performance graph filter
semantics and complete worker queue round trips. Generated CLAP output maps
boundaries without a MIDI port, preserving preflight/prefix refusal. Hosted graph
output opts in explicitly, aliases cached writable mappings and permits zero-frame
parameter flush. Disabled/unmapped boundaries filter; read-only/order/offset/
capacity faults clear the block without rolling back native histories. Pairing,
repeated ends, ownership, cancellation and recording remain receiver policy.

Linked f32/f64 tests cover seven output profiles per precision with 1000 blocks,
flushes, disabled filtering, malformed output and measured zero heap actions.
Loaded public gesture fixture passes 2000 graph blocks/32000 frames per precision,
wet/dry routing, two flushes, live swaps, owner stop and main retirement with zero
measured allocations. Portable queue/filter and generated CLAP tests pass. The
capacity correction above adds pre-node rejection coverage for 1025-slot worker
profiles while preserving owner cleanup.

All-feature workspace tests pass 427 tests including sixteen doctests; default
and all-feature Clippy, eleven default doctests, both warning-denied Rustdoc builds,
formatting/docs/whitespace pass. Synthetic native macOS arm64 only, with no new
DAW/device/recording/deadline claim. Native gesture input, modulation/per-note,
actual GUI/device-clock/lifecycle integration and broader formats/UI/platform/
production remain independent work; overall programme remains active.

## W115: Global modulation distinct from saved base values

Added finite signed physical global parameter modulation events, parameter graph
filtering and complete worker queue validation/round trips. CLAP input translation
is explicit and disabled by default on event/effect/main/auxiliary bridges; native
amounts narrow to finite f32, scoped events ignore, and flush offsets normalize to
zero. Generated global output widens to native f64 without a MIDI port and retains
preflight/prefix refusal. Receiver DSP owns combining/clipping/smoothing/reset and
persistence; no saved-state changes or metadata flags are inferred.

Portable input/output tests include signed/out-of-base-range amounts and zero-frame
flush; queue invalid whole batches publish nothing. CLAP tests pass ignored default/
scoped input, same-frame order, finite narrowing, invalid timing/nonfinite rejection,
10000 heap-free translations/flushes, 1000 heap-free generated output blocks and
1000 prepared consumer blocks. End-to-end consumer tests preserve base through
modulation/flush and reject invalid whole input before DSP/history mutation.

All-feature workspace tests pass 432 tests including sixteen doctests; default/
all-feature Clippy, eleven default doctests, both warning-denied Rustdoc builds,
formatting/docs/whitespace pass. No dependency change. Offline fixtures establish
no native hosted modulation, reference plug-in modulation, DAW/device/deadline or
per-note/other-format support. Those and GUI/device/format/UI/platform/production
remain independent work; overall programme remains active.

## W116: Mapped hosted native global modulation

Native graph preparation now independently opts input/output modulation into
explicit writable parameter mappings, caching native modulatable flags on main.
Unsupported input rejects before native mutation, with finite f32 amounts widened
to f64 and null global cookies. Enabled output narrows finite native f64 amounts
to f32 independently of scalar base range/step checks. Disabled/unmapped/scoped
output filters; selected data/order/offset/capacity faults clear the block with
nonrollback BackendFailure. Zero-frame flush works without render advancement.
Reusable CLAP registration has a default-false explicit global modulation option;
metadata alone enables neither bridge translation nor processor DSP.

Linked f32/f64 tests pass twelve 1000-block profiles per precision, fractional
modulation of a stepped base, disabled input, native flag mismatch, scoped/unmapped
filtering, malformed output/overflow, shared capacity and parameter flushes.
Unknown modulation after a valid scalar prefix leaves native base unchanged; a
subsequent empty flush confirms it. A selected output missing the native flag
rejects after native base mutation, proving nonrollback behavior. Metadata and
input/output selection-before-activation checks pass. Loaded public fixture
passes 1000 native blocks/8000 frames per precision, two flushes, exact base/
modulation aliases, measured zero audio heap actions and owner stop/main retirement.

Host/CLAP regression tests, default/all-feature workspace Clippy, eleven default
doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace pass.
No dependency change. Synthetic macOS arm64 only, without broad host/device/
deadline, per-note/modulation matrix/rescan or other-format claim. Queued audio
input, GUI/device-clock/lifecycle/recovery and broader format/UI/platform/production
remain independent work; overall programme remains active.


## Live documentation portal (9 October 2026)

Implemented a MkDocs Material portal over canonical guides, feature metadata,
decision records and workspace Rustdoc. The isolated Python dependency set is
pinned; guide search, responsive navigation, theme switching and copied code
controls are configured. The live server watches canonical input directories,
regenerates its catalogue without changing tracked Markdown, and rebuilds
warning-denied Rustdoc after workspace input changes. The generated staging
folder is explicitly unwatched to prevent recursive rebuilds. Strict site builds
require fresh canonical metadata. Guide-only builds omit stale API output.
API generation clears cached HTML first, excluding obsolete package references
from the combined artifact. CI now builds and uploads a combined site artifact;
no public deployment is set.
Decision 0124 records the publication and source-of-truth boundary.

Local macOS arm64 validation: documentation checker and staging regression tests,
strict complete portal build, workspace Rustdoc doctests and warning-denied
Rustdoc build passed. A browser displayed the portal and confirmed that adding
and then editing a temporary guide automatically rebuilt the page and refreshed
its visible text; the temporary guide was removed afterwards. The site search
index contains guide headings and content; Rustdoc is served beside the guides
with its own search. These observations do not establish hosted deployment,
other Python/platform environments, DAW compatibility or real-time guarantees.

## W117: Queued audio input to hosted graph workers

Added an explicit with_input constructor alongside the no-input worker profile,
matching graph/input/output ring channels with combined worker scratch<=32MiB.
Fresh fixed f32 renders consume one queued prefix, zero shortages and return
source/underflow frame counts. Pending output suffixes consume no new audio or
events. Outer event/storage validation precedes dequeue. Graph faults may consume
input/advance histories; faulty publication is suppressed and the worker retains
its error until explicit reset without draining more input. Reset preserves
queued/pending audio and input ring data. Worker/main native ownership is unchanged.

Four new portable groups pass timing through a half-block output queue, untouched
input on malformed event/storage, input starvation silence, fault latch/reset,
channel/combined-memory rejection and 1000 heap-free rounds. Existing eight
worker-output groups and device/host/graph regressions pass. Loaded public Gain
passes 1000 queue-fed native stereo blocks/16000 source frames plus a starved
block, exact suffixes, no heap actions and owner stop/main retirement. Existing
loaded Sine worker output and queued native parameter telemetry probes also pass.

Default/all-feature workspace Clippy, eleven default doctests, both warning-denied
Rustdoc builds, formatting/docs/whitespace pass. No dependency change. All new
native evidence is offline macOS arm64; no physical capture/duplex, independent
clock alignment, latency/deadline/long-session/hotplug/recovery claim. Those and
GUI/live edits/per-note/format/UI/platform/production remain independent work;
overall programme remains active.

## Small feature examples (9 October 2026)

Added the `ravet-examples` workspace package under `examples/tutorials` with four
small executable walkthroughs. MIDI synth renders C4/E4/G4, sample-timed sustain
and completed release through the existing portable sine instrument. Effect
renders stereo tones through the existing gain processor with 5 ms smoothing,
nonaligned automation and exact bypass timing. Both write explicit stereo PCM16
48 kHz WAV files outside processing and refuse existing destinations. The guide
links the existing Sine/Gain CLAP exports rather than adding duplicate wrappers.
MPE demonstrates RPN6/RPN0, independent member pressure/timbre/bend, shared master
bend and released expression ownership. MIDI 2.0 preserves 16-bit velocity and
32-bit expression through typed codecs, incremental UMP framing and validated
sample-offset events, including ordered same-frame packets.

Executed on macOS 27.0.1 arm64 with Rust/Cargo 1.99.0: all three new executable
walkthrough tests pass, including decoded WAV timing, pedal hold/release, stereo
automation, in-block bypass, overwrite preservation and expression/packet traces.
Default workspace check and 418 workspace tests pass; workspace all-target
Clippy with warnings denied, explicit workspace Rustdoc tests, warning-denied
workspace API build, formatting, documentation checker, 26 Python script tests
and strict documentation site builds with and without API pages pass. Generated both default WAV
artifacts under `target/examples/`. The current Rust Clippy constant-size chunk
lint also required three equivalent iterator changes in the existing device
probe and clocked-input tests; the affected clocked-input suite passes again.
Those iterator changes preserve behavior and need no capability/contract update.

Only local workspace dependencies were added. These are offline teaching runners,
not standalone device applications; the MPE walkthrough does not synthesize
pressure/timbre, and MIDI 2.0 does not add synthesis, negotiation or a plug-in
export. No new platform, DAW, physical controller or real-time deadline support
is claimed. Existing backend validation limits remain separate.

## W118: Buffered worker input-clock conversion

Added an explicit prepared worker stage reusing existing sinc/FIFO correction with
an internal channel-preserving relay, publishing graph-rate f32 capture into a
separate queue. Matched channels/rates/raw capacity/target/sinc bounds validate at
construction; one fixed output block adds<=16MiB to converter/queue memory.
Destination full/partial publication retains exact suffixes without reconversion,
raw consumption or controller advancement. Existing priming/underflow/reprime
and independent statistics policy remains, without inferred timestamps/latency.

Three new groups pass priming-zero/retained/full publication, configuration
rejection and 1000 nominal 96k-to-48k stereo rounds with finite/scaled samples,
settled levels, no heap actions or converter underflow. Existing clocked tests
pass. Loaded public native Gain graph passes 1000 queued blocks/16000 graph input
frames with and without this conversion stage, retained half-block output, a
separately starved graph-input block, no measured heap actions and owner stop/main
retirement. No device/GUI/DAW opens; offline nominal conversion establishes no
independent physical clock/drift alignment or deadline/long-session/recovery claim.

Full all-feature workspace tests and default/all-feature workspace Clippy pass,
as do eleven default doctests, both warning-denied Rustdoc builds and formatting/
docs/whitespace. The independently added tutorial package is preserved and included
in the broad checks; its catalogue entry subsequently arrived. Device probe adds
a direct dependency on existing workspace signal APIs; lock metadata also includes
the tutorial package, without new external packages for this converter change.
Live worker graph replacement, actual GUI/device clock/recovery, per-note and
broader formats/UI/platform/production remain independent work; programme active.

## W119: Live prepared replacement in buffered native graph workers

Shared fixed-worker queue/scratch/event/preflight/fault mechanics with an already-
prepared dynamic f32 engine. Exact configuration is checked without reprepare.
Candidates are considered only on fresh renders after old copied output suffixes
publish. Progress separates swap outcome from processing; pending calls have none.
Failed candidate start or full retirement preserves old rendering. Historical
last_swap remains observable when a swap precedes a processing fault, through
outer rejection/pending/fault-latch calls. No crossfade/tail drain/state migration.

Six portable groups pass old/new suffix order, bounded retirement/reclaim, invalid
outer input preserving offers, failed start, exact configuration, queued input/
underflow, once-only generated prefix and post-swap fault observation. A 1000-block
workload measures zero heap actions and checks owner start/stop/main destruction.
Loaded two-instance public Gain passes 1002 queue-fed stereo native blocks/16032
frames, replacement after the old suffix, main old-native retirement while new
processing continues, 1000 steady blocks and no measured heap actions. Clocked
input, Sine output and queued parameter telemetry native regressions pass.

Device/host/graph regressions, default/all-feature workspace Clippy, eleven default
doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace pass.
No dependency change. Offline macOS arm64 only; actual GUI/device-clock/lifecycle/
recovery, per-note/extended transport and broader formats/UI/platform/production
remain independent work. Overall programme remains active.


## W120: Extended block-start transport metadata

Added optional details for recording/active-loop/pre-roll flags, time signature,
BPM-per-sample tempo increment, bar start/zero-based number and active beat/sample
loop bounds. Both CLAP directions validate advertised metadata and matching known
timelines; sample/fixed-point rounding must preserve endpoint order. Missing
context preserves unknown details. No timeline extrapolation/reset/mid-block update.

Two ingress groups and two additional hosted groups pass flags/units/negative
pre-roll, absence, malformed signature/increment, dependencies and collapsed loop
bounds; 10000 conversions per direction measure no heap actions. Complete worker
batches retain details unchanged. Existing gain bridge/native graph regressions
and full workspace all-feature tests pass. Offline adapter evidence only; extended
metadata has no new loaded DAW/physical transport/deadline claim. Decision 0127.

## W121: Prepared synthetic stereo reverb

Reused bounded mono networks with common L/2+R/2 excitation, sample-rounded
0..10ms right-delay spread and wet width interpolation. Dry stereo is preserved;
opposite-polarity input cancels wet excitation. Complete mix validation preserves
histories; reset preserves configuration. Twelve delays cap at 133094 f64 samples.

Three new groups and existing mono regressions pass exact first impulse timing,
width/dry/cancellation, decay, zero-spread mono equivalence, partition/reset,
maximum memory, malformed configuration/mix and nonfinite recovery. Processing,
reset and mix across 10000 frames measure zero heap actions. Full workspace
all-feature tests pass; portable evidence only, no guaranteed decorrelation,
physical-room/freeze/modulation/native deadline claim. Decision 0128.

## W122: Prepared periodic wavetable oscillator

Added immutable copied 2..65536 finite f64 cycles, wrapped linear interpolation
and a private shared phase accumulator preserving the existing oscillator profile.
Retuning retains phase; reset retains frequency/table. Table allocation/destruction
stay outside processing. No bandlimited/alias-free synthesis or smoothing claim.

Three new groups and all signal regressions pass exact segments/final-to-first
wrap/non-power-of-two tables, analytic sine error, retuning/reset/partition,
invalid pre-allocation rejection and maximum capacity. 10000 sample/retuning/reset
operations measure zero heap actions. Portable offline evidence only. Decision 0129.

## W123: Explicit CLAP parameter gesture input

Added default-disabled raw begin/end input to all CLAP bridges/translator and the
hosted graph builder. Hosted input requires a writable map before activation;
disabled/unknown input rejects the complete prefix before native process/flush.
Timing/order/capacity retain existing policy; flush ingress offsets normalize zero.
Repeated ends are raw boundaries without scalar edits/pairing/recording inference.

Two translator groups pass mixed boundaries/values, ignored defaults, raw ends,
flush/timing/capacity and 10000 heap-instrumented rounds. New linked native f32/f64
groups pass mapped echo/time, audio/base preservation, valid-prefix then unknown
and disabled atomic rejection, zero-frame repeated ends and 1000 heap-free blocks
per precision. Missing input map rejects before activation. Existing output gesture,
modulation/note/native graph and public bridge regressions pass. This is synthetic
offline ABI evidence, not third-party receiver/DAW/UI recording validation.

For W120-W123: default/all-feature workspace Clippy, default doctests, both
warning-denied Rustdoc builds, formatting, docs and whitespace pass. Concurrent
example UI compile errors were corrected (float syntax, JSON expression, optional
template error, mutable snapshot borrow and unused import), preserving its work.
No dependency added by these packages. Wider per-note/standalone/recovery/formats/
UI/platform/production programme remains active. Decision 0130.


## W124: Positive multiplicative parameter smoothing

Added finite strictly positive f32 geometric smoothing with explicit u32 sample
counts, identical-target continuity, current-effective retargeting, reset and
exact final snap. Zero/negative endpoints use NonPositiveValue; nonfinite keeps
its existing error. Invalid edits preserve full progress. f64 log-difference ratio
design avoids extreme endpoint division overflow; sample work uses multiplication
and direction-aware endpoint clipping, including ratios rounded to unity.

Three new groups pass geometric landmarks, independent log interpolation,
positive subnormal-to-maximum monotonic transitions, retarget/reset/partition,
very long rounded-unity rising/falling ramps and invalid edits. Prepared samples,
retuning/reset measure no heap actions. All signal regressions, default/all-feature
workspace lint, default doctests, both warning-denied Rustdoc builds, formatting/
docs/whitespace pass. No dependency change. Portable evidence only; zero/signed
crossings, duration smoothing and native deadline claims remain outside this
profile. Wider programme remains active. Decision 0131.


## W125: Explicit note identities in hosted native graph routes

Added portable NoteWithId On/Off with concrete channel/key and full-u32 local
identity. Scalar checks preserve zero-velocity On; performance graph filters and
complete queues retain the event without voice/uniqueness/lifetime inference.
Opt-in hosted CLAP directions require selected CLAP ports before activation and
preserve native0..i32::MAX IDs. Default input rejects, output filters specific IDs;
wildcard native output retains legacy notes. Invalid input rejects the complete
prefix pre-native; selected raw IDs<-1 reject instead of Clack's wildcard collapse.
Generated MIDI output rejects identified streams before any host prefix, retaining
already rendered audio/DSP rather than dropping identity.

Portable validation/prefix tests, performance graph filtering and 10000 complete
queue roundtrips pass. Native unit tests pass0/i32::MAX/zero On/negative raw IDs.
Linked f32/f64 fixture profiles pass1000 blocks per input/output selection, exact
echo/timestamps/audio and disabled/invalid-prefix state preservation, with zero
measured heap actions. Missing/unsupported direction rejects before activation.
Generated-MIDI rejection passes1000 callbacks per identity with audio preserved
and no host push attempts/heap actions.

Full workspace all-feature snapshot passes488 tests including doctests; targeted
graph filtering, default doctests, both warning-denied Rustdoc builds, default/all-
feature workspace lint, formatting/docs/whitespace pass. Concurrent standalone
window build/object archive and example UI syntax/checked Option branch issues
were corrected while preserving its implementation. No new dependency. Synthetic
offline macOS arm64 evidence only, no third-party voice/DAW/deadline claim. Scoped
expression/parameter/lifecycle and wider formats/UI/platform/production work
continue. Overall programme remains active. Decision0132.

## Interactive examples, default I/O popup and piano controls (9 October 2026)

Replaced default WAV/trace entry-point behavior with live macOS windows. The
synth has eight voices, phase-preserving sine/saw/square/triangle switching,
low-pass VCF, VCA and one shared ADSR per voice. Prepared filter tables and a
fixed patch queue keep design/JSON/device ownership outside processing. The
effect plays demo tones or selected input; MPE pressure/timbre affect audio.
MIDI 2.0 remains an interactive full-resolution codec inspector.

Moved the standalone host window into ravet-webview. Its native title-bar
Audio/MIDI popup is enabled by default and explicitly removable. Standard
settings expose output/input channel pairs, supported output rates, requested
buffer size with nominal milliseconds and independently selected MIDI inputs.
The consumer retains device validation/lifecycle. These examples support up to
16 MIDI inputs and 64 device channels; output pairs duplicate stereo and selected
input pairs average into stereo. Embedded plug-in routing remains host-owned.

Added shared rotary controls and a two-octave piano widget with short overlapping
black keys, chords, glissando, cancellation/blur release and nonfeedback external
note display. Synth and MPE fit at both 900x780 and 760x650 logical content sizes;
voice details scroll inside their bounded panel. Disabled piano keys retain full
opacity, so white-key edges cannot show through the black-key overlay; the native
fixture checks every stopped key's opacity. All four native fixtures passed again
after this fix, and the stopped synth screenshot was inspected. Shared CSS and
SVG semantic palette tokens use Amber, selected by the user after reviewing five
alternatives on 9 October 2026.

On this macOS arm64 checkout, 449 workspace runtime tests pass, including four
portable subtractive tests and three example audio-adapter tests. Measured 1000
synth patch/process/reset cycles and 1000 selected-channel effect blocks have no
heap operations. Seven no-default-feature example tests pass. Native hidden
WebKit fixtures pass all four tabs, title-bar activation/full opt-out, knobs,
channel/rate/buffer/MIDI lists, piano geometry/chords/glissando, release and window
bounds; screenshots were inspected under target/examples-ui. They open no devices.
The live synth preview was relaunched for user evaluation, without automatically
selecting or starting a device. The palette comparison cycles through five
variants in the collaborative browser; its navigation was visually checked.

Workspace Clippy, 11 doctests, warning-denied Rustdoc, formatting, the documentation
checker and 26 Python tests pass. The first workspace doctest invocation overlapped
independent changes to foundation note-expression exports and used stale compiled
exports; rebuilding and rerunning workspace doctests passed. Existing unrelated
work is preserved. These results establish no broad desktop/device/controller
support, MIDI timestamp/sample alignment, long-session stability or deadline.
See [decision 0132](decisions/0132-standalone-settings-and-knobs.md).

## Stereo delay effect example (9 October 2026)

Replaced gain in the interactive effect tab and explicit WAV fixture with a
portable mono/stereo delay composed from prepared RAVET delay lines and linear
smoothers. Added 1..2000 ms time, 0..0.95 feedback and wet/dry knobs, ping-pong
cross-feedback and dry bypass that preserves internal repeats. Time glides over
20 ms; feedback/mix/routing use 5 ms ramps. Storage is prepared for two seconds
per channel; processing and reset allocate nothing. Live demo input now uses
staggered tapered bursts so repeats can be heard without an input device. Amber
styling, the default Audio/MIDI popup and selected live-input routing are reused.
The gain plug-in export remains separate; no delay plug-in wrapper is claimed.

On this macOS arm64 checkout, 14 example tests pass with default features and
11 portable tests pass without them. Four new delay tests check stereo and
ping-pong impulse timing/decay, partition-independent automation, bypass tail
retention, late invalid-event rejection without history changes, mono/max delay
and 1000 prepared control/process cycles plus reset with zero heap operations.
The WAV walkthrough checks echo placement and sample-timed bypass. All four
native UI fixtures pass again, including delay knob/check-box snapshots and
effect bounds at 900x780 and 760x650. Screenshots were inspected and the live
effect preview was opened without automatically starting a device.

The strengthened fixture found that earlier minimum-size checks had tested a
fixed browser viewport after native window resize. Fixed standalone container
autoresizing and now assert actual browser dimensions first. The MPE layout
needed additional compact spacing at the real smaller viewport. This supersedes
the earlier minimum-window validation evidence above.

Example Clippy, formatting, workspace doctests, warning-denied Rustdoc and the
documentation checker pass. These checks open no hardware devices and establish
no broad device/DAW support or real-time deadline.
See [decision 0133](decisions/0133-stereo-delay-example.md).

Follow-up transport, source and preference work: the main button now toggles
Start/Stop using current device selections, opening settings only when output is
unconfigured. Demo loop is an effect-panel control and can switch to live input
while playing; the standard popup contains device inputs/No input only. Demo
bursts repeat every 0.9 seconds, with both channels tested over three complete
cycles and live-source switching checked. Standard standalone Audio/MIDI
selections persist as bounded versioned native preferences keyed by stable
window title. Restore selects available exact IDs without opening devices;
hidden fixtures use process-local memory and do not alter user preferences.

Fifteen default-feature example tests and six webview tests pass. Native fixtures
check main-button stop/restart, source toggling, absence of demo entries in device
settings, complete output/input/pair/rate/buffer/MIDI restoration across newly
created windows, missing-device handling and no automatic start. All four tabs
pass actual viewport and document-bound checks at 900x780/760x650; meter readouts
stay inside their panels. Inspected screenshots now have the expected native
pixel dimensions. The demo sound and persistent native disk integration still
need user evaluation on actual chosen devices; no broad hardware claim follows.

## UI text selection defaults (9 October 2026)

Shared web styles disable label, heading and other body text selection in the
example editors and standard Audio/MIDI popup. Inputs, textareas, editable
content and preformatted/code output retain selection for editing and copying.
The policy includes the WebKit-prefixed property and remains overridable by
consumer CSS.

Collaborative browser checks pass for 53 label/heading/hint/readout elements,
numeric inputs, textarea/editable content, packet output and text-input
selection. Workspace doctests, warning-denied Rustdoc, formatting, documentation
and whitespace checks pass. The existing hardware-free native UI fixture was
attempted twice and stopped at its exact 760x650 viewport-size wait; this run
does not establish native selection or resize validation. No hardware device
was opened and no broader platform support is claimed.


## W126: Scoped standard expressions in hosted native graphs

Added portable wildcard channel/key/ID NoteTarget and seven standard expressions
with finite unit ranges and exact f64 values. Performance routes and complete
worker queues retain scope and bits. Explicit selected CLAP input/output builder
flags require matching directions before activation; invalid/disabled input
rejects whole prefixes pre-native, disabled/unknown/unselected output filters,
and selected malformed/capacity output faults retain native history. Generated
MIDI output rejects expressions before any prefix rather than lose scope.
Clack 0.2.0 omits Expression from its raw type accessor; conversion now matches
all seven public wire discriminators directly.

Portable validation, graph filtering and 10000 queue roundtrips pass; 21000 native
unit conversions retain all expression kinds/scopes/values without heap actions.
Linked f32/f64 fixtures pass 1000 blocks per selected profile, exact echo/time/audio,
complete input rejection, output capacity/nonrollback faults and preparation
requirements without measured heap actions. All-feature workspace tests, default/
all-feature workspace lint, default doctests and both warning-denied Rustdoc
builds, formatting/docs/whitespace pass. An initial workspace snapshot used an
older concurrent example test; the current-source rerun passes. No dependency
change or third-party voice/DAW/deadline claim. Native exported expression profiles,
note lifecycle/per-note parameters and the wider programme continue.
See [decision 0134](decisions/0134-scoped-note-expressions.md).


## W127: Explicit native performance ports in exported CLAP adapters

Added opt-in native note/expression input to the prepared translator/effect/main/
multibus adapters and native generated output to main/multibus. Independent
0..256 selectors preserve raw MIDI/UMP routing. Concrete On/Off retains optional
native IDs and checked f32 velocity; all seven expressions retain wildcard scope
and f64 values. Malformed selected data rejects the complete input pre-DSP with
silence. Native performance is ignored on input flush; zero-frame process rejects
representable selected events. Generated native too-wide IDs/performance in flush
reject whole delivery before host prefixes while retaining rendered audio/DSP.

Seven new groups pass all expression kinds/scopes, endpoints/zero On, raw field
rejection/default filtering, exact native output, independent MIDI/UMP ports,
pre-DSP prefix rejection, host refusal and parameter flush. Prepared translation
10000 calls and main f32/f64/auxiliary f64/mixed-port 1000-block workloads measure
no heap actions. CLAP/host/public reference regressions, default/all-feature
workspace lint, default doctests, both warning-denied Rustdoc builds and formatting/
docs/whitespace pass. Rebuilt public Gain bundle smoke passes 18 blocks/six
activation cycles; validator passes 34, skips 10, fails/warns zero. No dependency
change. Offline adapter/default-reference ABI evidence only; no new native
expression instrument, DAW or deadline claim. Wider programme continues.
See [decision 0135](decisions/0135-exported-native-performance-ports.md).

## Uniform example scaling and aspect lock (9 October 2026)

Replaced width-responsive panels and delay-only knob resizing with one fixed
900x780 example design canvas. Native content resizing now scales the complete
interface uniformly, including text, controls, panels and positions, immediately
from browser resize events and without the previous scale cap. All example tabs
opt into a 900:780 native content aspect-ratio lock. Added an explicit optional
StandaloneOptions lock; other consumers retain freely resizable windows by
default. Locked programmatic requests fit the largest matching rectangle within
the requested bounds, subject to the native minimum size. The device settings
popup retains its independent layout and persisted selections.

The native fixture passes all four tabs at 720x624, 900x780 and 1080x936, and
constrains wide/short and narrow/tall requests. It checks actual browser sizes,
all visible component dimensions and positions against their baseline, absence
of clipped content and bounded meter readings. Smaller delay and larger MPE
screenshots were inspected under target/examples-ui-aspect. This supersedes the
previous partial scaling/reflow policy; the earlier validation remains historical.
No devices are opened by these fixtures. Example/webview tests, Clippy, workspace
doctests, warning-denied Rustdoc, formatting and documentation checks pass on this
macOS arm64 checkout; no broad platform or hardware claim follows.


## W128: Scoped key release and choke

Added NoteOffScoped with finite f32 release velocity and NoteChoke with scalar-
free matching target. Full-u32 portable scope passes performance graph routes and
complete worker queues. Selected hosted CLAP directions opt into scoped input/
output before activation; disabled/invalid input rejects whole prefixes pre-native.
Output preserves wildcard-channel/key Off/choke, leaving concrete Off's existing
identity selection intact. Shared raw scope checks reject malformed negatives
before normalization. Choke's native ignored velocity is accepted rather than
validated as a scalar. Exported native profiles also carry release/choke; default
MIDI rejects whole scoped streams before host prefixes.

Portable validity/prefix and performance-filter tests, 10000 complete queued
roundtrips, raw native direction/range/default tests and linked f32/f64 profiles
pass. Linked 1000-block profiles cover exact timed fixture audio/echo, disabled
input, invalid prefix, capacity/nonrollback and retirement without measured heap
actions; selected malformed output retains native history and unrelated output
filters. Exported 1000-block scope roundtrip/ignored velocity and generated MIDI
1000-callback rejection groups pass. Full all-feature workspace snapshot passes
517 tests including doctests. Default/all-feature workspace lint, default doctests,
both warning-denied Rustdoc builds and formatting/docs/whitespace pass after
removing redundant test parentheses. Rebuilt public Gain smoke and validator pass
18 blocks/six activation cycles and 34 passed/10 skipped/zero failed or warnings.
No dependency or public voice-policy change. Offline scope/ABI evidence only, no
third-party/DAW/deadline claim; note completion and scoped parameters continue
alongside broader formats/devices/UI/platform/production work.
See [decision 0136](decisions/0136-scoped-note-release.md).


## W129: Exported scoped parameter values, modulation and metadata

Added distinct finite f64 scoped value/modulation events with matching target and
routed note input endpoint, independent of saved/global f32 values. Parameter
graph filters include them; performance excludes them and complete queues retain
full bits. Exported translator/effect/main/multibus add independent scoped input
and main/multibus scoped output selectors, exact native IDs/fields/values and zero-
time parameter flush. Raw exact all--1 scope alone is global; malformed negatives
can no longer collapse into a global edit through Clack. Selected malformed input
rejects pre-DSP; missing generated routes/too-wide IDs reject whole host delivery
while retaining rendered audio. Eight scoped automation/modulation metadata flags
are explicit default-false options, with NONE/ALL const presets; public Gain keeps
NONE and global flags remain independent.

Portable range/scope/prefix tests, parameter graph filtering, 10000 complete queue
roundtrips/translator workloads and all 256 metadata flag combinations pass without
measured prepared heap actions. Six adapter groups cover native f32/f64 and f64
auxiliary 1000-block process/flush precision, global independence, raw malformed
fields, atomic input rejection and whole generated route/width rejection with
audio preserved. CLAP/host/public-reference regressions, default/all-feature
workspace lint, default doctests, both warning-denied Rustdoc and formatting/docs/
whitespace pass. Rebuilt Gain smoke passes 18 blocks/six activation cycles; native
validator passes 34/skips 10/fails or warns zero. No dependency or voice-policy
change. Offline adapter evidence only; hosted scoped metadata validation/voice
application and the wider programme continue.
See [decision 0137](decisions/0137-exported-scoped-parameters.md).


## W130: Hosted scoped parameter validation and delivery

Prepared mapped native graph input/output now opt into scoped f64 values and
modulation independently of global parameters, note output and other performance
profiles. Both directions select the originating CLAP note input endpoint. Cached
writable native metadata checks most-specific ID/key/channel/port support, value
ranges and stepped integrality; finite signed modulation has separate bounds.
Invalid or disabled inputs reject whole prefixes before native mutation. Unknown
IDs/unselected ports filter; selected output faults discard complete portable
output without rolling back native history. Zero-time parameter flush and exact
raw global scope remain distinct from performance timing. Metadata changes need
retirement/repreparation. No new dependency or voice-allocation policy.

All 256 metadata flag combinations and six linked test groups pass: f32/f64
1000-block selected direction/capacity profiles, full-f64 echo/timing, separate
saved global state, granularity flags, rejected prefixes, value bounds/steps,
fractional modulation, zero-frame flush and invalid preparation. Prepared heap
workloads count zero allocations/deallocations. Full all-feature workspace
snapshot passes 533 tests including doctests; default/all-feature workspace lint,
default doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace
pass. Rebuilt public Gain smoke passes 18 blocks/six activation cycles and native
validator passes 34/skips 10/fails or warns zero. Linked synthetic evidence only;
no third-party instrument, DAW or deadline claim. Broader format/device/UI/platform
work remains active alongside note completion and further DSP profiles.
See [decision 0138](decisions/0138-hosted-scoped-parameters.md).


## W131: Prepared fourth-order crossover

Added atomic LR4 control-thread coefficient design and fixed-storage one-channel
f64 low/high processing. Each branch cascades two identical second-order
Butterworth sections; cutoff is half amplitude and recombination is a unity-
magnitude second-order all-pass, without waveform identity. Exact block lengths
preflight before outputs/history; empty blocks no-op, reset clears and unsmoothed
replacement retains all four histories. No dependency change.

Five new groups pass independent bilinear response, all-pass impulse/energy,
cutoff sinusoidal power, exact partition/reset/replacement and invalid design/
response/shape checks. 10000 prepared replacement/reset/block workloads count
zero allocations/deallocations. Signal regressions, default/all-feature workspace
lint, default doctests, both warning-denied Rustdoc builds, formatting/docs/
whitespace pass. Electrical offline evidence only, without loudspeaker alignment,
automatic routing/modulation or callback deadline claims.
See [decision 0139](decisions/0139-linkwitz-riley-crossover.md).


## W132: Directional note completion notifications

Added portable NoteEnd matching targets distinct from key release/choke, with
carrier offsets for delivery order rather than exact termination time. Graph
performance filters and complete worker queues retain them. Hosted output selects
the originating CLAP note input endpoint independently of note output/maps/
identities; native timestamp/velocity are ignored and carrier order follows the
previous retained offset or zero. Raw scope validates before normalization.
Default/unselected/wildcard ports filter; selected invalid scope/capacity discard
portable output without native rollback. Portable hosted completion input rejects
whole prefixes; parameter-only zero-frame flush rejects output. Exported main/
multibus adapters select independent originating input endpoints, retain exact
matching IDs/fields, reject missing routes/wide IDs/flush before host prefixes and
retain audio/DSP progression. No automatic voice tracking or dependency change.

Portable prefix/range, graph filters, 10000 complete queue roundtrips and raw
native scope/ignored maximum timestamp/NaN velocity/flush tests pass. Four linked
groups cover f32/f64 1000-block no-note-output/no-output-map profiles, exact
carrier ordering, filtering, rejected controller input, capacity/nonrollback and
saved native history without counted heap actions. Four exported groups cover
f32/f64/main/multibus routes, preflight/width, host-prefix refusal, flush, input
filtering and 1000 prepared processing/flush workloads. Full all-feature workspace
snapshot passes 549 tests including doctests; default/all-feature workspace lint,
default doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace
pass. Rebuilt Gain smoke passes 18 blocks/six activation cycles; native validator
passes 34/skips 10/fails or warns zero. Offline synthetic/ABI evidence only, no
public reference voice/third-party/DAW/deadline claim. Broader independent work
continues. See [decision 0140](decisions/0140-note-completion.md).


## W133: Prepared stereo fractional-delay chorus

Added a stereo f64 modulated delay effect with independent feedback histories,
shared sine LFO/right phase offset and bounded storage for every permitted live
control edit. Preparation validates 8000..192000 Hz and complete controls before
allocating at most 76802 f64 values. Fractional depth scales the center delay,
with a one-sample causal minimum. Atomic live edits retain phase/histories; zero
rate freezes phase, exact dry mode retains wet processing, reset retains controls
and invalid channel lengths preserve samples/history. No dependency change.

Five groups pass independent indexed full-history modulation/feedback recurrence,
fractional impulse/feedback polarity/stereo phase, exact partition/reset/dry
history/frozen phase, rejected construction/edits/shapes and maximum bounded
preparation. 10000 prepared process/edit/reset workloads count zero heap actions.
Signal regressions and default/all-feature workspace lint, default doctests, both
warning-denied Rustdoc builds and formatting/docs/whitespace pass after removing
an unnecessary integer cast in a test. No bandlimited modulation, automatic
latency/tail, headroom/denormal or native deadline claim. Generic native-format
processing infrastructure and the wider programme continue.
See [decision 0141](decisions/0141-prepared-stereo-chorus.md).

## Pre-push completed-work validation (9 October 2026)

An isolated completed-work snapshot passed 515 runtime tests and 11 doctests
with `cargo test --workspace --locked` on macOS arm64. Workspace Clippy with
warnings denied, warning-denied Rustdoc, formatting, the documentation checker
and all 26 Python script tests passed. The snapshot includes the live examples,
standard persistent Audio/MIDI settings, Amber controls and uniform aspect-locked
scaling. The concurrently edited native planar adapter refactor was excluded and
left in the working tree. These checks add no hardware, DAW or deadline claim.
The existing remote remains private; known proprietary paths in initial Git
history still prevent public publication.


## W134: Reusable native planar processing

Added the optional SDK-independent ravet-native crate with prepared f32/f64
main/auxiliary planar adaptation, bounded copied input/output planes and generated
events. Whole-stream validation precedes DSP; every input copies before foreign
output writes, including cross-bus aliases. Shape/alignment/output-overlap faults
silence supported active output ranges and retain tails. Exact typed transport
and generated events remain consumer-delivered; audio/reset panic poisons the
adapter until fresh stopped preparation. Both Gain C precision paths now use
this reusable core while retaining their product symbols/state and event layout.

Six generic test groups and five Gain groups pass, covering f32/f64 cross-bus
aliases, extreme f64 values, scoped/generated events, zero-frame processing,
instrument/no-input initialization, rejection/history, panic poisoning and
prepared allocation/deallocation checks. Default/all-feature workspace lint,
default doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace
pass. Rebuilt actual VST3 passes 47 SDK validator tests plus Cocoa editor and
36-block/six-activation program contracts. AUv2 render/state/Cocoa editor and
420-block factory-program contracts pass; LV2 mono/stereo binary contracts and
AAX binary-entry probe pass. Native f64/program/meter C++ contracts pass. Local
macOS arm64 evidence only; duplicate-library linker warnings remain in SDK builds.
AAX is locally ad-hoc signed, without PACE or Pro Tools validation. Generic
metadata/event/state marshalling and SDK registration remain separate work; no
new DAW/device/deadline claim. See [decision 0142](decisions/0142-native-planar-adapter.md).


## W135: Complete native event marshalling

Added canonical version-one C-layout records for every portable event kind, full
u32 note/parameter IDs, explicit wildcard scope, exact widened f32 global values/
velocities and full f64 scoped values/expressions. MIDI short messages and all UMP
framing types retain their bytes/numeric words. Whole bounded stream conversion
preflights values/timing/capacity before destination mutation; equal-time order,
zero-frame offsets and tails remain exact. Unknown tags/bits, unused fields and
lossy scalar conversion reject. Gain now shares the separate unchanged legacy
global parameter layout/decoder, retaining existing product C signatures.

Six new groups pass exhaustive scopes/types, canonical golden records, invalid
prefixes, precision/offset/order/capacity bounds, 1000 full-capacity conversion
workloads and 1000 complete f32/f64 native instrument audio/event cycles with no
counted heap actions. Six planar and eight all-feature Gain regressions pass.
Default/all-feature workspace lint and default doctests pass; both warning-denied
Rustdoc builds pass after correcting one code span that Rustdoc interpreted as a
link. A repeated planar test launch briefly stalled in dyld before entering its
harness, then resumed and passed without intervention. Formatting/docs/whitespace
pass. Rebuilt actual VST3 validator passes 47/0 plus Cocoa editor and program
contracts; AUv2 audio/state/editor/420-block programs, LV2 binary contracts and
AAX entry probe pass. Five native C/C++ contracts now include both C11/C++17
header layout checks. macOS arm64 native evidence only; AAX remains ad-hoc/entry
only, without PACE/Pro Tools validation. Endpoint routing, host delivery/refusal,
state, generic factories and broader programme remain independent work.
See [decision 0143](decisions/0143-native-event-records.md).

## Hosted documentation source ownership — 9 October 2026

Moved the hosted reference's beginner introduction, practical guides, reviewed
tutorial chapters, line explanations and rendering tools into
[scripts/docs/hosted](../scripts/docs/hosted/README.md). The website publisher
reads this repository; its generated documentation is excluded from the website's
Git history. The website retains its presentation and service configuration.

Validated locally on macOS: the documentation checker, two hosted-navigation
regression tests, an all-features workspace Rustdoc build with warnings denied,
and 15 website/navigation checks. The index includes direct API and method
bookmarks; formatting and external-link behavior were inspected at desktop and
mobile widths. Public deployment was not performed.


## W136: Exact native block-start transport

Added a canonical v1 C-layout snapshot with independently available playing,
sample/beat timelines, tempo, details, signatures, increments, loops and bars.
Unknown states remain unknown; i64/f64/i32 values retain their complete precision
and semantic ranges. Dependencies validate before conversion: active ordered
loop ranges need their timelines, increments need tempo, bars need beats and
signatures need nonzero u16 components. Unknown bits, unused/reserved fields and
invalid values reject without guessed context, quantization or extrapolation.
Direct typed adapter pass-through stays unchanged.

Six groups pass all 32768 availability/flag combinations, full extreme metadata,
negative positions/signed zero, malformed native/portable snapshots and 10000
complete prepared f32/f64 processing cycles per precision with zero counted heap
actions. C11/C++17 header layout and native Gain precision/program/meter contracts
pass. Full all-feature workspace snapshot passes 572 tests including doctests.
Default/all-feature lint, default doctests, both warning-denied Rustdoc builds and
formatting/docs/whitespace pass after making bit-mask precedence explicit. Some
workspace executables stalled before their harness in dyld, then resumed; a
local temporary copy of the limiter binary independently passed too. Existing
duplicate validate example-name Cargo warnings remain. Synthetic local macOS
arm64 evidence only, without additional SDK/DAW/device/deadline claims. Generic
exported processor entry/lifecycle infrastructure and the wider programme continue.
See [decision 0144](decisions/0144-native-transport-records.md).


## Feature overview priority — 9 October 2026

Reordered the [feature registry](features.toml) and regenerated the catalogue
so reusable processing, graphs, plug-in integration and DSP lead the overview.
State, MIDI, device, file, UI and runtime capabilities precede reference
implementations and teaching examples; development tools, plans and exclusions
follow. Stable IDs, statuses, descriptions, limits and evidence are unchanged.
The [maintenance guide](maintenance.md) records the ordering policy for new entries.

Validation: documentation checker, all 12 documentation-checker regression tests
and whitespace checks pass. Parsed registry comparison confirms all 176 feature
records are preserved with identical contents.

Rebuilt the sibling website's static documentation snapshot for the local PHP
preview at port 8088. Browser inspection of `/ravet/docs/FEATURES/` confirms
portable processing leads and the four teaching entries occupy positions
162–165. The hosted publisher completed with a fresh all-feature Rustdoc build
with warnings denied; all ten website tests and both hosted-navigation tests
pass. The hosted tooling guide now explains that static previews require a
publisher rebuild after source edits. No public deployment was performed.

Public follow-up: published the ordering-only documentation change as commit
`e8fff3031ab1482cd53488e7da3c736f73f617a9` from an isolated checkout, preserving
the shared checkout's in-progress native changes. The published registry retains
all 172 existing records with identical contents. Website deployment
[37971051914](https://github.com/mo0kid/djwaudio.co/actions/runs/37971051914)
passed its documentation and website checks. Public browser inspection confirms
`https://djwaudio.co/ravet/docs/FEATURES/` starts with portable processing and
places the teaching entries at 158–161; other site files were still uploading
at that inspection. The local checkout retains additional unpublished features.


## W137: Generic exported native processor lifecycle

Added a consumer-selected immutable version-one entry table and opaque owner
composing prepared main/auxiliary f32/f64 audio, complete native input/generated
events, optional strict transport, lifecycle and latency/tail declarations.
Default+Send processors construct through contained control callbacks; complete
canonical configuration and native output capacity validate before DSP. Foreign
table/handle/precision, library lifetime and record/sample ranges remain caller
obligations. Supported output prefixes silence on faults; status/count always
rewrite valid result storage, with no generated delivery on faults or DSP rollback.
Poison persists until fresh stopped preparation. Gain exports this generic entry
alongside its existing product ABI.

Six new groups pass 1000 complete multibus/scoped/event/transport cycles per
precision with zero counted heap actions, no-input completion/silence behavior,
full latency/tail ranges, configuration/refusal and factory/prepare/audio/reset/
timing-query panic containment/recovery. Existing planar/event/transport/Gain
regressions pass. Default/all-feature workspace lint, default doctests, both
warning-denied Rustdoc builds and formatting/docs/whitespace pass. Actual-library
C++ entry contracts compare both interfaces over 2000 timed/cross-alias blocks,
zero-frame controls, rejected prefixes, wide f64 audio and lifecycle/timing. Six
native C/C++ contracts pass; rebuilt VST3 validator47/0 plus editor/programs,
AUv2 render/state/editor/420-block programs, LV2 binaries and AAX entry probe pass.
Local macOS arm64 evidence only; AAX remains ad-hoc/entry only, with no new DAW/
device/platform/deadline support claim. Generic parameter/state metadata and SDK
registration/editor infrastructure remain independent work; programme continues.
See [decision 0145](decisions/0145-native-processor-entry.md).


## Publication history cleanup — 9 October 2026

Replaced the publishable branch history with a fresh public-framework snapshot
and removed earlier local checkpoint refs. Original history and the complete
tracked/non-ignored working tree were backed up outside the checkout in a private
sibling directory. Existing uncommitted framework work remains uncommitted.
The private remote's main branch is updated with an explicit force-with-lease;
repository visibility remains private. Hosted cached commits and other clones
require separate review before publication; a branch rewrite is not evidence of
server-side object deletion. See [the publication guide](publication.md).


Cleanup validation: the publication checker passes across all remaining local
refs, the documentation checker passes for both the working tree and an isolated
export of the clean committed root, all 28 Python regression tests pass, and Git
object integrity checks pass. The original private root is absent from the local
object database after reflog expiry and garbage collection. The remote advertises
only the clean main branch; GitHub reports no pull requests, releases or workflow
artifacts. Hash comparison confirms every other backed-up working file is
unchanged. GitHub's commit API still resolves the old private root by SHA, so
server-side deletion is unresolved and visibility remains private. A purge
request draft with the original commit IDs is retained in the private backup.


## W138: Bounded native global schemas and state

Added an optional paired companion table with immutable stable-ID global f32
schemas, UTF-8 names/units, linear/log/discrete mapping and explicit automation/
bypass/read-only roles. Complete provider snapshots revalidate/reorder by declared
IDs before native output. Complete canonical state validates before consumer
atomic restore; explicit consumer decoding can migrate legacy records within an
8208-byte bound. State calls require exclusive stopped ownership. Gain-native uses
thin f32/f64 facades, keeping portable DSP backend-independent and retaining its
complete 32-byte-v2 and legacy 20-byte-v1 codecs.

Six groups pass native metadata/ranges/roles/UTF-8 limits, registration/version/
capacity/provider panics, mismatched/reordered snapshot IDs/ranges, native state
length/product/version/value/duplicate/order rejection and shared-owner snapshot/
restore poisoning/recovery. 1000 complete snapshot/load/save/process workloads per
precision count zero heap actions. Existing native/Gain regressions, default/all-
feature workspace lint, default doctests, both warning-denied Rustdoc builds and
formatting/docs/whitespace pass. Actual-library C++ checks pass metadata/mapping,
2000 complete recall/audio cycles, canonical codec parity, legacy migration and
atomic rejection; seven native C/C++ contracts pass. Rebuilt VST3 validator47/0
plus editor/programs, AUv2 render/state/editor/420-block programs, LV2 binary and
AAX entry-only probes pass. Local macOS arm64 evidence only; no additional DAW/
platform/deadline claim. Live publication requires explicit audio-owner contracts
and remains next, alongside generic SDK registration/scoped metadata/UI and the
wider programme. See [decision 0146](decisions/0146-native-parameter-state-companion.md).


## W139: Explicit native live global state publication

Added an opt-in prepared audio-parameter contract and independent schema-bound
atomic publication objects. Control decodes/revalidates complete state; audio
applies newly accepted revisions before process and conditionally reflects base
state afterward. Cached schemas avoid audio metadata-provider calls. Repeated
apply skips restore, own reflection is not reapplied, and newer control restores
win. One-attempt Busy/Stale preserves results without spinning. Coalescing,
retirement, exact pairing/library lifetime and callback contracts remain explicit.
Preparation/deactivation clear accepted revision; reset preserves it. Audio-provider
panics poison the shared owner until fresh preparation; decoder panics have no DSP
owner to poison. Gain-native exposes the extension for both precisions and retains
canonical/legacy decoding.

Five new Rust groups pass complete record/capacity/layout/failed-output validation,
f32/f64 apply/process/reflect revision and identity semantics, atomic rejection,
panic recovery and concurrent controller/audio/observer coherence, including
conditional audio automation reflection. Each precision's 1000 complete cycles
counts zero heap actions and no audio metadata-provider calls. All589 workspace
runtime tests/doctests pass, plus28 Python regressions; default/all workspace lint,
default doctests, both warning-denied Rustdoc builds and formatting/docs/whitespace
pass. One workspace executable temporarily stalled at macOS dyld entry before its
harness and later resumed; this was not a test failure or callback observation.

The actual-library C++ state probe passes2000 f32/f64 apply/audio/reflect cycles,
coherent snapshots/canonical bytes/legacy migration, stale newer-restore protection
and retention of Gain automation smoothing. All8 native C/C++ contracts pass.
Rebuilt VST3 validator47/0 and editor/program probes, AUv2 render/state/editor/
420-block programs, LV2 binaries and local ad-hoc AAX entry probes pass. Local
macOS arm64 contract evidence only, without additional DAW/device/platform/deadline
or PACE/Pro Tools claims. Generic SDK registration/mapping remains independent;
the programme continues. See [decision0147](decisions/0147-native-live-state-publication.md).


## W140: Reusable VST3 controller schema registration

Added an optional external-SDK C++17 header that preflights a complete bounded
native global schema before adding VST3 root-unit parameters. It rejects invalid
metadata/table/roles/defaults, duplicate/conflicting or host-reserved IDs and
oversized/malformed UTF-16 host text instead of clipping. Complete UTF-8 conversion
preserves supplementary characters. SDK parameter objects call native pure
linear/log/discrete mapping on control, retain global f32 physical values and
explicit automation/bypass/read-only roles, and keep their table/library alive.
Numeric formatting retains fractional/arbitrary stepped choices; strict bounded
locale-independent decimal/exponent parsing rejects invalid/suffixed/overflow/
nonfinite text without mutation. Explicit boolean bypass roundtrips On/Off labels.
Gain's actual controller now registers both base controls through this helper;
factory/state/processing/editor/programme behavior remains separately owned.

SDK-linked contracts pass unrelated log/fractional/Unicode/default/role metadata,
whole preflight failure/container preservation, ASCII127/supplementary127 UTF-16
boundaries, invalid numeric text/signs/whitespace and bypass text roundtrips,
conflicts/reserved IDs/table bounds, and actual native Gain f32/f64 schemas/mappings.
Both bundlers that build VST3 contracts now include the new contract target.
C++17 syntax,28 Python regressions and formatting/docs/whitespace pass. Rebuilt
VST3 schema/processing contracts, validator47/0 without the initial bypass text
roundtrip diagnostic, actual Cocoa editor/programs, AUv2 render/state/editor/
420-block programs, LV2, ad-hoc AAX entry and8 native C/C++ contracts pass. W139's
589-test workspace/lint/Rustdoc snapshot covers unchanged underlying Rust APIs.
Local macOS arm64/SDK3.8.1 evidence only; no new DAW/platform/deadline or production
AAX claim. This helper is control-only: audio-cached mapping, other SDK/factory/
state/event/editor infrastructure and the wider programme continue. See
[decision0148](decisions/0148-vst3-native-schema-registration.md).


## Prepared standalone comb filters — 9 October 2026

Added one-channel f64 feedforward and feedback comb filters with fixed whole-sample
delays, unity direct input and positive/negative coefficients. Preparation validates
1..1048576 samples before one bounded allocation; feedback requires finite
absolute coefficient below one. Coefficient edits retain histories and reject
before mutation; zero coefficient returns input exactly while advancing history.
Sample/in-place processing and reset retain allocations. Kind/delay changes require
new preparation. Existing private damped reverb comb behavior remains independent.

Six new test groups pass signed impulse timing/decay, independent indexed input/
output recurrence across coefficient edits, analytic complex frequency gains,
partition/empty/reset/zero-history continuity, atomic invalid preparation/edits
and exact maximum-delay/storage bounds. 10000 prepared process/edit/reset
workloads per kind count zero heap actions. All signal tests/doctests, signal
Clippy with warnings denied, workspace doctests and warning-denied workspace
Rustdoc pass in an isolated macOS arm64 snapshot. That snapshot excludes the
concurrently introduced ravet-embedded and embedded-gain workspace members; its
574 runtime tests/doctests pass. The checkout's six comb tests and signal Clippy
also pass using a fresh target directory. Source formatting and all 28 Python
regressions pass. Checkout documentation coverage remains dependent on the concurrent
embedded-package registration; no device/DAW/platform/deadline claim is added.
See [decision 0150](decisions/0150-prepared-comb-filters.md).

## Embedded processing foundation — 9 October 2026

Completed the no_std foundation boundary, fixed-capacity Rust audio adapter,
public allocation-free immediate stereo gain/C bridge, native Arduino C++11
stereo subset and independent embedded-rust/arduino/pi-linux starters.
See [embedded projects](embedded-projects.md) and
[decision 0152](decisions/0152-embedded-processing.md). The selected available
reference boards are ESP8266-D1 and Raspberry Pi 3 Model B+; STM32/Compute Module
models and audio codecs remain unspecified. No physical board was exercised.

Executed locally on macOS arm64 with Rust/Cargo 1.99.0:

- Foundation and embedded/reference tests pass both with default features and
  with defaults disabled; the six new Rust fixtures cover integer PCM saturation,
  interleaving/tails, complete preflight, zero-frame/instrument paths, timed
  gain/strict rejection, native C aliasing/layout and callback/reset/fault heap
  counts. Measured allocation/deallocation counts remain zero.
- `python3 scripts/check-embedded.py --cross` passes host tests, the actual host
  Rust static-library C fixture and native C++11 contracts with exceptions/RTTI
  disabled. Library checks pass for thumbv6m-none-eabi, thumbv7em-none-eabihf,
  aarch64-unknown-linux-gnu and armv7-unknown-linux-gnueabihf. A Cortex-M4/M7
  hard-float reference static archive builds with explicit abort/spinning panic
  policy. This is not final firmware-link/flash or Linux runtime evidence.
- A generated independent embedded-rust consumer library builds for
  thumbv7em-none-eabihf; a generated pi-linux consumer locks/builds/runs on this
  macOS host and prints `RAVET embedded stereo gain PASS`. Pi execution is pending.
- Generator regression tests pass for independent manifests, no-default embedded
  dependencies, local Arduino library/sketch copies and existing-directory/scope
  protection.
- Full workspace tests, Clippy with warnings denied, required workspace Rustdoc
  tests, warning-denied API build and workspace formatting check pass. All 29
  Python regression tests and the documentation checker pass.
- The Arduino OfflineGain sketch compiles/links for `esp8266:esp8266:d1` (D1 R1)
  and `esp8266:esp8266:d1_mini` (D1 R2/mini), with ESP8266 core 3.1.2 and
  Arduino CLI 1.5.2-rc.1 in isolated target-directory tooling. Firmware reports 29,188 RAM bytes, 59,747 instruction/cache RAM bytes
  and 236,116 flash code bytes including core/SDK overhead. It was not uploaded
  or executed on a board; this establishes build evidence only.

The [embedded workflow](../.github/workflows/embedded.yml) configures Ubuntu
contracts/cross checks, with remote execution pending. These checks do not
establish board/codec/DMA/cache/interrupt behavior, live Pi devices, Arduino
flashing or real-time deadlines. Existing desktop signal/graph/runtime/editor
crates are not converted into allocator-free firmware dependencies.

The embedded user guide and starters stay board-neutral for Arduino. The D1
profiles above are compile fixtures, not recommended audio targets. Select
hardware using measured DSP workload, CPU/FPU, RAM, codec interfaces and DMA/
interrupt behavior; synthetic gain build success does not establish suitability
for substantial audio applications.


## W141: Control-cached native global parameter mapping

Added an immutable control-prepared native schema cache with copied metadata,
static ranges/roles and sorted binary ID lookup, plus a separate paired foreign
table preserving earlier ABI layouts. Cached concurrent metadata/scalar reads and
complete normalized event conversion call no consumer providers or DSP and use
no heap actions, locks or I/O. Full normalized ID/value/timing/capacity validation
precedes output mutation; f64 normalized intent explicitly narrows to the global
f32 contract and physical f32 widens exactly into tagged event records. Equal-
offset order and tails survive. Creation/drop and providers stay on control, with
explicit exact pairing/library/static-reference lifetime and stopped retirement.
No hidden role filtering or unknown-ID dropping occurs. Gain exports both precision
companions.

Six Rust groups pass schema/mapping/roles/full-ID/layout checks, whole bad-prefix/
unknown/nonfinite/offset/order/capacity rejection, zero-frame/empty/oversize caches,
provider-panic isolation and shared foreign-reader concurrency. Each precision's
1000 full1024-event metadata/map/convert workloads and four readers'10000 cached
workloads count zero heap actions/provider calls. Existing native/Gain regressions
and scoped lint pass. Default/all-feature workspace lint, workspace doctests, both
warning-denied Rustdoc builds and formatting/docs/whitespace pass after refreshing
stale generated Signal metadata/archive artifacts that omitted concurrently added
public exports; shared sources were preserved. Workspace package coverage now
includes the independently added embedded reference.

Actual-library C++ checks pass both native Gain precision profiles,2000 normalized
conversions, declared order/ties/tails, whole rejection and failed scalar/metadata
outputs. All9 native C/C++ contracts pass. Rebuilt VST3 validator47/0/schema/editor/
programs, AUv2 render/state/editor/420-block programs, LV2 binary and ad-hoc AAX
entry checks pass. Local macOS arm64 evidence only; no additional DAW/platform/
deadline or PACE/Pro Tools claim. Prepared SDK queue/profile integration is next;
the broader programme continues. See [decision0151](decisions/0151-native-cached-parameter-mapping.md).


## AUv3 and mobile native adapters — 9 October 2026

Added a reusable paired f32 native session, an AUAudioUnit processor adapter,
programmatic CoreAudioKit parameter editor and public Gain extension/containing
apps. AU parameters retain physical stable-ID schema values; complete live state
includes bypass. Rendering validates bounded input before DSP, handles timed
changes and ramps that continue across blocks, caches stopped timing, maps host
transport and supports an explicit short MIDI/framed UMP input profile. Main-queue
parameter refresh keeps cached host values synchronized. Ordinary concurrent
control publication does not turn into an audio-stream failure; a raced ramp-base
snapshot retains its last observed values.

Apple mobile has explicit AVAudioEngine/AVAudioSession lifecycle, preferred
configuration, microphone permission, route/interruption/media-reset handling,
retained state and endpoint-ID MIDI reconnection. Android has AAudio prepared
planar/interleaved conversion, callback chunking, zero-timeout capture, shortage
silence/counters, atomic native failure reporting, stopped stream retirement and a
1024-slot immediate MIDI queue. Public native apps expose Audio/MIDI settings by
default and stop on backgrounding. SDKs remain external. A single build command
creates Apple bundles or Android libraries/development APKs; local fixture
signing, native alignment verification and RAVET/libm/NDK notices stay explicit.

Executed on macOS 27.0.1 arm64/Xcode 27:

- MacOS AU in-process contracts pass three lifecycle cycles, physical parameter
  schema, audio/tails, timed bypass, complete live restore, whole-list rejection,
  cross-block ramps, host bypass and public render/schedule wrapper, cached timing,
  transport conversion, opt-in short MIDI/UMP/truncated-UMP rejection, zero-frame
  calls, resource stop/reprepare and programmatic editor construction.
- Arm64 iOS device and simulator app/extension builds pass with native compiler
  warnings denied. The signed iPhone 17 Pro/iOS 26.0 simulator app installs; its
  explicit in-process AU contract runner passes. A normally launched app shows
  readable gain/bypass/Start/Stop and Audio/MIDI controls in an inspected screenshot.
  This evidence exercises an app process rather than AU extension discovery/IPC.
- Android arm64/API 26 native code cross-builds with NDK r29 (29.0.14206865),
  native warnings denied. JDK 17/API 36 Java compiles with all warnings denied.
  Direct SDK/build-tools 36.0.0 packaging produces a development APK with verified
  signature and 16 KiB ZIP/native-library alignment. No Android runtime was run.
- Scripted AAudio-double contracts pass oversized callback chunking/prefix/tails,
  live controls and invalid recall, bounded MIDI fill/overflow/delivery, short
  capture silence/counting, stream errors, rate mismatch/open/capture failure,
  repeated stop/recovery and balanced ownership. 1000 callbacks race 10000 control
  edits without stopping audio. These are ownership/logic tests, not hardware or
  AAudio deadline evidence.
- 46 targeted native/Gain Rust tests and 29 Python script/checker tests pass.
  Workspace Rustdoc examples pass (13) with a fresh isolated target directory;
  shared-target attempts had linked stale signal exports and failed the new comb/
  waveshaper examples despite their current exports, so the clean-target evidence
  is recorded separately. Warning-denied workspace Rustdoc build, formatting,
  Python compilation, documentation catalogue/link checks and whitespace pass.

The mobile CI workflow is configured, not an executed GitHub platform result.
Full support remains open: named third-party AUv3 hosts/IPC, physical mobile
routes/devices/interruption/MIDI, Android execution, production provisioning and
callback measurements need actual evidence. Auxiliary buses, generated output,
factory programmes, SysEx, f64 selection, a native instrument reference, persisted
standalone sessions and background audio remain further profiles. Android duplex
requires matched rates/channels and has no independent-clock conversion. See
[the guide](mobile.md) and [decision 0153](decisions/0153-auv3-mobile-native-adapters.md).

Follow-up checks in the same work package: canonical native recall wins stale
AU superclass parameter caches; the regression passes on macOS and the iOS
simulator. All AU adapter/editor sources compile with application-extension
restrictions enabled. Apple and Android standalone controls use scrolling layouts
for smaller viewports; final Apple device/simulator builds and Android APK
packaging pass, and the updated iPhone UI screenshot was inspected. Extension
metadata includes the factory class and display name required by the installed
Xcode template/simulator installer. Hardware/host acceptance targets were requested
from the owner; the full-support gate remains open.


## W142: Prepared VST3 cached global parameter input

Added an optional external-SDK C++17 input profile with control-created native
caches, sorted IDs/roles, explicit read-only policy and distinct mapping/routing
results. Gain's actual processor uses separate f32/f64 profiles. A reusable walker
reads each global SDK point once into <=16KiB stack scratch and validates all raw
queue/total counts, getter results, finite normalized values and ordered offsets,
including unknown IDs, before any prepared visitor runs. Program expansion and
stable bounded insertion remain consumer-owned and unchanged. Visitor failure may
retain private scratch, so pending state/DSP/output publication waits for success.
Cached mapping/validation use no provider calls, framework heap actions, locks or
I/O; host and visitor obligations remain explicit.

SDK-linked normal/fast-math targets pass actual native Gain precision profiles,
explicit read-only routing/failed output preservation, whole malformed/unknown/
count/getter/nonfinite/timing/capacity preflight before visitors, zero-frame/full
capacity, single point reads, retained traversal/ties and visitor failure scope.
1000 cached queue workloads count zero C++ new/delete and metadata reads; W141's
Rust cache workloads establish the separate Rust heap/provider evidence. Tests
inject foreign NaN/infinity bytes and classify referenced IEEE memory as integer
bits, avoiding invalid by-value floating assumptions under fast-math.

Both VST3 contract bundlers build both input targets. Rebuilt four SDK contracts,
validator47/0, actual Cocoa editor/programs, AUv2 render/state/editor/420-block
programs, LV2 binaries, ad-hoc AAX entry and9 native C/C++ contracts pass. Syntax,
formatting/docs/whitespace pass; W141's current workspace lint/doctest/Rustdoc
snapshot covers unchanged underlying Rust. Local macOS arm64/SDK3.8.1 evidence
only; no new DAW/platform/deadline or production AAX claim. Continuous host curves,
gestures/scoped/performance/transport, generic SDK factories/state/processing/
output/editors and the wider programme remain active. See
[decision0154](decisions/0154-vst3-cached-parameter-input.md).


## W143: Explicit native audio-owner complete state

Added a separate cache-paired companion for complete physical f32 base-state
restore/snapshot on exclusively prepared generic processors. All IDs/ranges and
completeness validate before one atomic consumer restore; complete snapshots
revalidate/reorder before writes, preserving failed fields and successful tails.
No byte codec/schema/role provider runs in framework audio paths. Audio hooks are
now available without a 64-bit atomic gate; live publication remains separately
gated. Direct calls do not invent generations or modify accepted live revisions;
consumer protocol ordering and needed-recall scheduling remain explicit.

Five Rust groups pass unrelated/full-u32 identities, reordered provider state,
malformed/capacity/null/alignment/atomic rejection, mismatched provider IDs/ranges,
provider failure isolation, shared poison/reset/reprepare, zero-frame automation,
empty/max1024 complete state and 1000 heap/provider-free cycles per precision.
Actual Gain C++2000 restore/render/snapshot cycles pass wide f64 audio, whole-state
rejection, zero-frame automation, retained smoothing, lifecycle and failed/tail
outputs. C11/C++17 layouts pass. See
[decision 0155](decisions/0155-native-direct-audio-state.md).

## W144: Explicit prepared native telemetry

Added independent stable-ID native readout descriptors/caches and a paired generic
processor companion. Control metadata preflights UTF-8/count/duplicate/display
bounds before caching. Complete exclusive prepared snapshots validate all IDs,
finite raw f64 values and known nonfinite-input flags before reordered writes.
Exact u64 windows and extreme/tiny/signed-zero f64 values survive; display hints
do not clip observations. Metadata reads are shared/provider-free; consumer
measurement, retention/reset and UI/host publication remain explicit.

Seven Rust groups pass metadata limits/preallocation rejection, full identities,
reordering/malformed/unknown/duplicate/nonfinite/flag/pointer/capacity failures,
provider isolation, shared poison/reprepare and concurrent cached readers. Both
precisions run1000 heap/provider-free cycles; max1024 snapshots run1000 heap-free
cycles, four shared readers run10000 cycles each, and empty/null/tail boundaries
pass. Gain exposes existing12 meter fields10..21. Actual C++2000 stereo cycles
pass raw wide/tiny/RMS levels, exact counts/nonfinite diagnostics, mono/reset/
zero/rejected retention, failed fields and tails. C11/C++17 layouts pass.

The combined snapshot passes626 all-feature workspace tests/doctests. A test-only
Clippy initializer warning was corrected; refreshed telemetry, default/all-feature
workspace lint, default doctests and both warning-denied Rustdoc builds pass.
Formatting/docs/whitespace pass. Rebuilt four SDK contracts, VST3 validator47/0,
Cocoa editor/program contracts, AUv2 rendering/state/editor/420-block programs,
LV2 binaries, ad-hoc AAX entry and11 native C/C++ contracts pass. The first two
new linked tests assumed unique table addresses; equivalent promoted Rust tables
had different addresses with matching callbacks. Corrected callback-profile checks
pass; table address is not owner/type identity. The prepared C++ client draft also
passes2000 complete actual-library cycles and zero C++ new/delete, with invalid
profiles/optional capabilities/factory cleanup covered; actual SDK migration is
next. Local macOS arm64 evidence only; no new DAW/device/deadline/platform or
production AAX signing claim. Wider programme remains active. See
[decision 0156](decisions/0156-native-prepared-telemetry.md).


## W145: Reusable native C++ client and SDK reference adoption

Added an SDK-independent C++17 owner composing exact native lifecycle and optional
complete audio state/separate telemetry caches. Control acquisition/retirement,
exclusive DSP callbacks, shared cached metadata, optional capability queries,
invalid-profile rejection and partial-factory cleanup remain explicit. Promoted
tables may differ in address; supported bindings match callback profiles, with
exact types/library/foreign ranges still caller obligations. Normal forwarding
uses no C++ heap/locks/I/O; native result/silence/poison and consumer duties remain.

Gain's actual VST3 processor now consumes two precision-matched clients and tagged
events. Preparation marks complete state pending; the first wholly validated
callback, including zero-frame flush, restores the current publication. Complete
base snapshots feed the existing generation/CAS protocol, preserving newer
controller restores and ongoing smoothing. Generic telemetry preserves meter IDs,
normalization and host/editor delivery. Programs/controller codec/editor policy
and legacy public C exports remain consumer-owned; no transport or performance
mapping was added in this package.

Actual-library C++2000 complete callback cycles pass optional capabilities,
invalid/mismatched profiles, equivalent promoted tables, differing callback
rejection, factory cleanup, wide audio/tails/lifecycle and zero C++ new/delete.
SDK contracts add malformed unknown queue preflight, first valid/same-precision
reprepare recall, zero-frame recall/automation and unchanged-publication retained
ramps at both precisions. All four SDK contracts, validator47/0, actual Cocoa
editor/programs, AUv2 render/state/editor/420-block programs, LV2, ad-hoc AAX entry
and12 native C/C++ contracts pass. Syntax/docs/format/whitespace pass; W144's626
workspace tests/doctests/lint/Rustdoc cover unchanged Rust. Local macOS arm64 only,
no new DAW/device/deadline/platform or production AAX signing claim. Transport,
performance, generic factories/buses/state/editors and wider programme remain
active. See [decision 0157](decisions/0157-native-cpp-client.md).


## W146: Explicit VST3 block-start transport mapping

Added an external-SDK C++17 mapper and immutable context requirements. Null context
means canonical unknown transport; non-null context preserves exact i64 samples,
known playing/recording/loop state and available finite f64 music/positive tempo/
u16 signature. Supported scalar preflight preserves failed output. Advertised
cycles are finite, ordered when active, and map only with active loop/known music;
inactive/positionless bounds are omitted with loop state retained. Bar start has
no exact SDK bar number, so no native pair is guessed. Further SDK context fields,
discontinuity, pre-roll and missing availability remain unrepresented explicitly.

Shared reference-based IEEE classification now lives in an SDK-independent header,
retaining cached parameter-input finite checks. Normal/fast-math fixtures pass1024
availability profiles, exact i64 extrema/f64 signed zero/u16 bounds, null/partial/
unavailable malformed fields, complete invalid scalar rejection and10000 mappings
with zero C++ new/delete. SDK processor contracts exercise requested flags and
malformed context before pending restore/automation/DSP, then valid context at both
precisions. Gain forwards canonical context to its generic owner but ignores
musical timing; no transport-driven synthesis or DAW timeline claim follows.

Both bundlers build both mapper targets. All six SDK contracts, validator47/0,
actual VST3 Cocoa editor/programs, AUv2 render/state/editor/420-block programs, LV2,
ad-hoc AAX entry and12 native C/C++ contracts pass. All29 Python regressions and
syntax/docs/format/whitespace pass; W144's626 workspace snapshot covers unchanged
Rust. Local macOS arm64/SDK3.8.1 evidence only; no new device/deadline/platform or
production signing claim. Performance/scoped events, generic factories/buses/
state/editor and wider programme remain active. See
[decision 0158](decisions/0158-vst3-block-start-transport.md).


### Consumer-selected mobile deployment targets — 9 October 2026

The consumer chooses physical iPhone/iPad and Android models, OS versions,
audio/MIDI routes and AUv3 host/version acceptance targets; a fixed project-wide
hardware allowlist is not required. The mobile guide now describes the consumer
validation matrix. This clarifies the earlier request for named release targets;
selection alone adds no runtime support evidence. Existing app Audio/MIDI choices
remain the runtime selection mechanism.

The fixture builder now accepts Apple deployment minimums and Android minimum
API, compile/target SDK and installed build-tools. Adapter API minimums are
validated before build commands. Apple Rust/CMake receive the same OS minimum;
Android compiler/CMake/DEX/manifest receive the same minimum API. The optional
Gradle fixture also accepts minimum/compile/target SDK properties. Defaults and
arm64 architecture profiles remain unchanged.

On the existing macOS arm64/Xcode/NDK r29 environment, an iOS simulator build
with deployment target 18.0 passed compilation and local signing. An Android
API-28/compile-SDK-36/target-SDK-35 build passed native/Java compilation, APK
signing and 16-KiB alignment checks; `aapt2 dump badging` confirmed minimum 28
and target 35, and CMake recorded API 28. Four configuration regression tests
cover selectable versions, defaults and incompatible minimum/SDK combinations.
The Python script suite passed (33 tests). No physical device or third-party
AUv3 host was exercised by this work; optional Gradle properties were not run.
Workspace Rustdoc tests and API documentation with warnings denied also passed
using the isolated `target/mobile/doc-check` directory. The documentation checker
passed after regenerating the feature catalogue.


## W147: Exact portable mute expression and explicit CLAP limits

Portable volume now accepts finite 0..=4, retaining both zeros and positive
subnormals without clamping; zero mutes without implying release/completion.
Native v1 layouts remain unchanged. Hosted/exported CLAP retains its independent
strictly positive range, rejecting selected zero input before DSP and complete
unrepresentable generated streams before host writes while retaining DSP/audio.
Native scope/bit round trips, worker whole-batch handoff and CLAP filtering/input/
output boundaries pass. See [decision0159](decisions/0159-zero-volume-note-expression.md).

## W148: Reusable VST3 performance input

The optional SDK C++17 mapper preserves unquantized velocities, full signed-ID
bits except the absent -1 sentinel, note cents expansion, poly pressure and six
standard expressions. Explicit unknown-expression policy and complete raw/list/
expanded capacity/offset preflight precede private visitors. Fixed scratch reads
getters once and retains tie order; unsupported variable payloads are rejected.
Normal/fast-math fixtures include10000 full-list cycles with zero C++ heap actions.
Gain declares no performance input. See
[decision0160](decisions/0160-vst3-performance-input.md).

## W149: Reusable VST3 performance output

Whole canonical-stream representability precedes host calls. Exact velocity,
identity and normalized expression fields survive; standard expressions require
ID-only scope. Tuning unit conversion may round. Integer IEEE volume checks
reject subnormal division loss even under fast-math. Unsupported wildcard/choke/
completion/payload semantics are not substituted. Mutable host events cannot
alter native sources; refusal reports an exact accepted prefix without replay.
Normal/fast fixtures pass1000 maximum-capacity zero-heap cycles and every refusal
prefix. Gain declares no performance output. See
[decision0161](decisions/0161-vst3-performance-output.md).

## W150: Prepared native VST3 audio mapping and refreshed host evidence

A reusable control-prepared profile maps up to 9 logical buses/64 channels per
direction to independent compact SDK directions at f32/f64 precision. Complete
shape/pointer/alignment/overflow/output-overlap validation precedes DSP/state;
input/output aliases remain allowed. Zero-frame omission is explicit. Failed
configuration/output mappings retain prior values. Fault cleanup clears valid
active prefixes only at either supported SDK precision, including a rejected
processing-precision mismatch, preserving tails and counting cleared channels.
Gain now adopts this mapper while retaining mono/stereo effect declarations.
Normal/fast fixtures cover10000 ordinary maps and1000 maximum576-channel maps
without C++ heap actions. Actual SDK overlap rejection preserves pending state;
corrected callbacks recover. The first rebuilt format run exposed overly narrow
precision-mismatch cleanup; the corrected helper and refreshed contracts pass.
See [decision0162](decisions/0162-vst3-native-audio-buses.md).

The combined W147–W150 snapshot passes628 all-feature workspace tests/doctests,
default/all-feature Clippy, default doctests, both warning-denied Rustdoc builds,
formatting/docs/whitespace and29 Python tests. Rebuilt CLAP smoke/validator passes
44 checks (34pass,10skip,0fail/warn). Refreshed format gates pass12 SDK contracts,
VST3 validator47/0, three Cocoa editor modes/programs36 blocks/six cycles, AUv2
render/state/editor/programs420 blocks, LV2, ad-hoc AAX entry and12 native C/C++
contracts. These are local macOS arm64/SDK3.8.1 observations, not production AAX
signing, generic instrument export, other-platform or callback-deadline proof.

A fresh independently identified disposable REAPER7.82/macOS arm64 instance loaded
the rebuilt VST3 binary on macOS27.0.1, with BlackHole16ch at48kHz/512 frames.
The public guarded synthetic-input harness passed discovery, all12 normalized
meter readings, gain/bypass IDs and native floating-window creation. It muted
hardware destinations, stopped playback and closed the FX window; the stopped
fixture was inspected afterward. The harness now logs its enumeration count and
only public Gain candidates. A prior process retained a stale catalogue; a fresh
process using the committed arm64 VST path resolved it. No visual/accessibility,
performance-event/multibus instrument, physical output or deadline claim follows.
Evidence remains in ignored target/reaper-w150/config/ravet-validation/native-meter-host.log.
The wider programme remains active; portable identified performance voices and
actual instrument adoption are next.


## W151: Explicit normalized performance voice ownership

Added an independent fixed 1..128-slot receiver with full optional u32 identities,
unquantized f32 velocities and seven exact f64 expressions. Conjunctive targets,
same-address ID retrigger, atomic address-conflict rejection, stacked unidentified
notes, release-tail expression updates, bounded chronological stealing and original
target completion are explicit. Complete timed semantic preflight simulates on a
fixed copied bank without changing live state or reserving output implicitly.

Six groups pass every target scope, full identities, signed zeros/subnormals,
defaults/mute/ranges, malformed atomic rejection, release/retrigger/steal/choke/finish
policy, independent chronological oracle, late timed conflicts and zero/max limits.
10000 maximum-bank control workloads and1000 full1024-event preflights count no
Rust heap actions. Existing MIDI/MPE policy is preserved. See
[decision0163](decisions/0163-native-performance-voice-ownership.md).

## W152: Transactional f32/f64 performance sine consumer

Added an independent native-precision mono/stereo reference using this ownership
policy, timed ADSR, linear volume/expression, explicit balance, tuning and vibrato.
Pressure/brightness remain exact readable state without timbre shaping. Whole
generic validation precedes private scratch rendering with fixed copied DSP/voice
state; only success commits live samples/completions/history. Late semantic or
completion overflow faults preserve phase, envelopes, voices and level. Actual
in-block envelope completion permits later address reuse of that ID. Positive
output capacity enables original-target completion; zero suppresses notifications
while internal lifetime still ends. Preparation owns bounded scratch allocation.

Eight precision-specific groups pass analytic unquantized attack/audio, exact
partition equality, timed mute/unmute/balance, zero-frame/-zero ownership, release
completion/order/tails, malformed/identity/output-overflow atomicity and continued
history, actual in-block ID reuse, notification suppression, extreme rates and
prepare/reset/deactivate/state limits. Each precision runs100 maximum128-voice
render/release/completion cycles with zero Rust heap actions. See
[decision0164](decisions/0164-transactional-performance-sine.md).

The combined snapshot passes642 all-feature workspace tests/doctests, instrument
Clippy and all-feature workspace Clippy, default workspace doctests, both warning-
denied Rustdoc builds,33 Python tests and format/docs/whitespace checks. A macOS
loader delay occurred before test code on the workspace volume; sampling showed
only dyld startup. Identical checksum-verified public test executables ran normally
from task-local temporary storage through Cargo's supported runner, including
doctests. No source, security setting or repository runner default changed. These
are local macOS arm64 offline observations, with no new native facade/SDK export,
DAW, physical device, other-platform or callback-deadline claim. The wider programme
continues with a public native instrument reference and actual SDK integration.


## W153: Public native performance reference — completed locally

A separate public 16-voice f32/f64 archive exposes consumer-specific native
processor, schema, cache, direct-audio-state and optional publication entries.
Canonical level state retains the public RAVETPS product identity and does not
serialize voice history. The actual linked C++ fixture passes both precisions,
codec identity and atomic malformed-state rejection, exact performance ownership,
analytic rendering, mute/release/completion, tails and lifecycle. Its 2000 prepared
workload cycles count no C++ new/delete actions. Workspace default Clippy,
doctests, both warning-denied Rustdoc builds, formatting and documentation checks
pass. Execution uses the checksum-verified local runner described in W152.

See [decision 0165](decisions/0165-public-native-performance-reference.md) and the
[native guide](native-processing.md). This archive has no SDK instrument factory,
controller, native editor or DAW validation. SDK instrument adoption is next;
these local results do not establish physical-device or deadline support.


## W154: Public VST3 performance instrument — SDK and offline DAW validation passed

The public performance archive now has a distinct external-SDK processor and
controller: no audio input, mono/stereo f32/f64 output, one 16-channel event input,
Level schema/state and five audible note-expression declarations. Recall shares
the transactional event stream with automation and performance input, preserving
voice progress and pending state after late faults. Completion is suppressed
explicitly because VST3 has no native NoteEnd equivalent. No custom editor is
declared; the host can provide generic controls.

The linked actual-archive SDK contracts pass 1000 prepared cycles across both
precisions/layouts with no C++ heap actions, analytic audio, exact velocity and
signed IDs, cents/absolute expression order, mute/release/ID reuse, exact phase
and envelope preservation after late faults, canonical state/text rejection,
controller connection recall and the shared 1024-event capacity. The dynamically
loaded bundle passes all 47 SDK validator tests, zero failures, on the local
macOS arm64/Xcode toolchain. Strict C++ syntax and formatting checks, four DAW
fixture/guard regressions and the documentation checker pass. The strict fixture
command suppresses an external SDK final-class virtual-destructor warning;
framework warnings remain errors. Rust processing is unchanged from W152/W153's
workspace validation.

A new isolated REAPER test copy initially scanned unrelated system plug-ins.
After cancellation, the guarded script rejected discovery before inserting tracks
or playing audio; no DAW evidence follows from that failed run. Startup notice
automation returned an unchanged evaluation window. Only the owned test process
was restarted with its explicit public plug-in path; validation was still pending
at that point.
No production project, licence, physical route or global audio service was used.
See [decision 0166](decisions/0166-vst3-performance-instrument.md).

The subsequent owned instance discovered and instantiated the actual public
VST3 instrument. Its live transport-region check stalled and supplied no live
audio proof. A guarded offline apply-FX fixture then passed three stereo 48 kHz
DAW renders for Level .8, saved/restored .8 and held .4/.8 automation. Independent
PCM checks pass peak/RMS (.04/.028284 and .02/.014142), A4 frequency, channel
agreement and pre/post-note silence in twelve interior regions. `lsof` confirms
the prepared public VST3 binary loaded in the explicit isolated REAPER 7.82
process; the script verifies muted hardware destinations and stopped transport.
No native editor, host note-expression editing, live device/deadline or other
host/platform claim follows. W154's implementation/SDK/offline-host milestone
is complete; remaining format/framework work continues independently.


## W155: Reusable LV2 native control/state — completed locally

Gain now adopts a reusable native f32 processor/cache/publication profile and
portable LV2 state extension. The actual module passes existing stereo/mono
audio/lifecycle/alias contracts plus canonical state/default/malformed recalls,
restore versus host control precedence, atomic control/audio faults, optional
URID map and host refusal. 4000 prepared run/save cycles count no C++ heap or
URID-map calls; 20000 audio blocks overlap coherent state saves. Root review
caught invalid-constructor channel indexing; guarded connect/run and direct
invalid-profile fixtures now preserve sentinels for zero/oversized channels and
missing table callbacks. Misalignment and oversized callbacks preserve tails.
Strict C++17 warning checks pass against external LV2 1.18.10 on macOS27 ARM.
No Atom/MIDI/UI/f64/auxiliary, DAW/Linux-runtime or hardRT claim follows. See
[decision0167](decisions/0167-lv2-native-schema-state.md).

## W156: Verified indexed native-FLAC seeks — completed locally

Explicit CompressedReader::new_indexed_flac verifies a known-duration native
FLAC stream once and retains compressed packets rather than complete PCM. Exact
seeks binary-search packet offsets and decode one containing packet. The default
FLAC/MP3/Vorbis replay policy is unchanged. Existing stopped CompressedReadAhead
generation replacement accepts the indexed reader and retains failed seeks.

Six existing compressed regressions, four new integration groups and the new
actual-decoder-call unit test pass, along with package all-feature Clippy. Tests
use independent PCM16/24 oracles across1/2/8 channels, packet boundaries/EOF,
malformed digest/count/truncation, failed seeks/tails and stopped queue generations.
Instrumentation proves one decode per arbitrary seek in a shared33-packet index
and zero at EOF/invalid targets. Input/payload/index/frame limits, upfront decode
cost and worker allocations are explicit. No lossy/Ogg/file-backed indexed seek,
physical clock/recording or deadline claim follows. See
[decision0168](decisions/0168-indexed-flac-worker-seeks.md).

## W157: Bounded composite focus and piano ownership — completed locally

Reusable RovingFocus manages1..256 fixed members with orientation/RTL/wrap,
Home/End, disabled/hidden/inert filtering, explicit refresh and original-tabindex
restoration. Piano now has one Tab stop and chromatic arrow navigation, separate
Space/Enter ownership and exact note release on focus moves, hiding/removal,
disabling and teardown. Observer/listener lifetime is explicit.

Thirty focused checks initially passed in collaborative Chromium and hidden native WebKit;
the surface crate's15 all-feature tests/doctest pass. Actual T3 Tab/ArrowRight/Tab
traverses first member, second member and following control; piano navigation
moves MIDI60to61 and Enter emits one matching original-source on/off pair.
Chromium document.hasFocus remains false; this is DOM keyboard/contract evidence,
not visual/native focus or assistive-technology validation. An initial WebKit
relative-script fixture was blocked by local-file read restrictions; inline
fixture assembly matching production embedding then passed. No screen-reader,
other-platform or device claim. See
[decision0169](decisions/0169-bounded-web-focus.md).


### Shared W155–W157 verification

The fixed source snapshot passes647 all-feature workspace tests/doctests,
all-feature all-target workspace Clippy with warnings denied, default workspace
doctests, both default/all-feature warning-denied Rustdoc builds, formatting,
33 script regressions, four DAW fixture/evidence regressions, catalogue/link
checks and whitespace checks. Cargo was serialized across agents with two
compiler jobs and the checksum-identical local test runner. No redundant native
host/device/platform claims follow from these workspace checks. Next packages
resume after this accepted snapshot.

Subsequent W157 review hardened iframe ownership: node creation, hit testing and
keyboard/blur/visibility listeners now bind to the piano root's owner document
and live window. Constructors reject missing/closed windows before replacing
markup or tab state. The expanded38 Chromium and38 hidden WebKit checks pass,
including parent-versus-child event isolation and exact-source release/teardown.
Embedding/frame lifetime remains consumer-owned. Updated Rustdoc/workspace
checks are grouped with the next parallel packages.


## W159: Automatic read-ahead and buffered playback — completed locally

W159 — automatic read-ahead ownership and buffered playback completed.

ReadAheadWorker<P>/ReadAheadPump and prepared BufferedPlayback now integrate
existing WAV and replay/indexed compressed read-ahead sources. Explicit start
runs an automatic bounded worker with retained suffix/backpressure and 2 ms
polling park timeout. Callback play/pause/render uses prepared ring storage and
atomics; source cursor advances only for consumed frames; running underruns stall
source position and silence suffixes; paused callbacks consume nothing. Verified
EOF requires full pending publication and queue drain. Control seek/stop joins,
positions and restarts; foreign/failed seeks preserve original generations.
shutdown returns owned pump plus original pump error; panic cannot report EOF.

Executed on local macOS 27 ARM64:
- python3 target/programme-command-runner.py cargo test -p ravet-audio-file --all-features --test automatic --test compressed_stream --test compressed_buffered --test compressed_indexed --test buffered --locked: 21 passed (7 new automatic groups; 14 scoped regression groups).
- python3 target/programme-command-runner.py cargo clippy -p ravet-audio-file --all-features --all-targets --locked -- -D warnings: passed.
- scoped fmt passed; joint workspace doctests/warning-denied Rustdoc/docs remain root-owned.

Tests cover concurrent retained suffixes, 1000 heap-free paused callbacks and active
heap-free consumption, exact pause/resume/EOF drain, gated true underrun/cursor stall,
malformed buffer atomic rejection, failed/foreign stopped generations, successful
stop/seek, source reclamation while ring-full, pump error prefix/original-error/source
ownership, successful fault recovery, panic distinct from EOF, exact whole indexed
FLAC playback/recall and unchanged NaN/signed-zero/infinite/subnormal WAV float bits.

Limits: explicit source-rate/channel playback and start; control-only join/seek/drop,
cooperative stop cannot cancel backend I/O or guarantee deadline. No clock alignment,
resampling, streamed loop/live seek/mailbox/crossfade/device/physical recording claim.
Thread creation failure retains pump; restart failure cannot undo a completed seek.
Panicked pump ownership is lost during worker unwind and shutdown returns Poisoned.
Custom ReadAheadPump implementers own bounded pumping/matching/seek contracts.

## W161: Distinct CLAP validator example names — completed locally

Sine and sidechain now explicitly name their Cargo examples validate-sine and
validate-sidechain, preserving the validator source paths. Commands and runtime
workflow follow those names; workspace builds no longer infer colliding validate
artifacts. Cargo metadata confirms exactly one distinct example per package.
Both renamed commands pass against the existing public built CLAP bundles:
sine MIDI/state/audio acceptance passes3024 blocks over three activation cycles;
sidechain native f32/f64 port/audio/fault/heap acceptance passes6006 blocks over
three activation cycles. Catalogue regeneration and documentation checking pass.
No newly built bundle, DAW/platform or device claim follows from this command fix.


## W158: Packaged LV2 MIDI performance instrument — completed locally

The public native performance archive now has a zero-input mono/stereo f32 LV2
instrument with ordered bounded Atom MIDI notes, generic Level control and
canonical complete recall. Actual-module contracts cover analytic pitch/velocity/
timing, exact splits, late-fault voice-history rollback, pending restore,
channel-scoped release, full event budget and2000 prepared heap/URID-free cycles.
The explicit --plugin performance bundler produces a self-contained .lv2 directory
with metadata, module and licence notices. Both Performance and original Gain
packaged-module contracts pass; three Python boundary tests pass. Shared build/
package/verification mechanics reside in scripts/lv2_bundle.py. No host install,
LV2 DAW/Linux/device/deadline claim. See decision0170.

## W160: Bounded shaped paragraph layout — completed locally

ParagraphLayout combines explicit width/line spacing/physical alignment with
Unicode line opportunities and retained paragraph bidi context across soft wraps.
Original UTF8 content/separator ranges and glyph clusters remain available;
TrueType SVG paragraph outlines roll back complete output on late faults.
Six new paragraph groups and21 surface tests/doctests plus package lint pass.
The collaborative Chromium preview renders joined Arabic/Latin paragraphs with
192 finite outline paths inside the720x420 source bounds; no SVG text nodes.
Unicode15 line breaking and Unicode16 bidi, explicit font/style, greedy wrapping,
unbreakable overflow, whitespace and byte/line/run/glyph limits are documented.
No implicit fallback/dictionary/justification/caret/native GPU/accessibility or
other-platform claim. See decision0172.

## W162: AIFF prepared worker handoff — completed locally

W162 — AIFF PCM read-ahead/recording worker adapters completed.

AiffReadAhead/AiffRecorder reuse extracted private PCM handoff mechanics with
unchanged public WAV buffering/recording semantics. AIFF read-ahead also implements
ReadAheadPump, so automatic worker ownership and buffered playback accepts AIFF.
Matching scratch/channel bounds, retained suffixes, stopped-generation clearing,
retry-preserving reads, dequeue poison and one-ring-capacity final drains remain
explicit. Stream PCM conversion/count/FORM/padding/flush rules are unchanged.

Executed locally on macOS27 ARM64:
- python3 target/programme-command-runner.py cargo test -p ravet-audio-file --all-features --test aiff_buffered --test aiff --test buffered --test automatic --locked: 19 passed (5 new AIFF groups + 14 stream/WAV/automatic regression groups).
- python3 target/programme-command-runner.py cargo clippy -p ravet-audio-file --all-features --all-targets --locked -- -D warnings: passed.
- package fmt passed; root owns grouped workspace doctest/docs/registry/diff gates.

Exact tests cover PCM16/24/32 at 1/2/8 channels, backpressure suffix/seek generations,
reader I/O failure retry, allocation-free callback transfer, bounded final recorder
drains with mono PCM24 odd padding, dequeued nonfinite/partial-write poison and
actual automatic AIFF playback/stopped recall. Existing WAV/automatic fixtures
pass after extraction. No AIFF-C/new widths/metadata/markers/resampling/device/path/
physical clock/recording/deadline claim added.


### Shared W158–W162 verification

The accepted source snapshot passes 665 all-feature workspace tests/doctests,
all-target all-feature Clippy with warnings denied, default workspace doctests,
default/all-feature warning-denied Rustdoc, formatting, 39 script regressions,
four DAW evidence regressions, catalogue/link checks and whitespace checks.
Cargo was serialized with two compiler workers and checksum-identical local
executable launching. Later W163–W167 work begins after this accepted snapshot;
its evidence is recorded separately. No new DAW/device/platform support follows.


## W165: Explicit VST3 reference bundle profiles — completed locally

The public bundler now accepts --plugin performance while preserving default
Gain behavior and its existing paths/options. Build/package/validation mechanics
are shared in scripts/vst3_bundle.py. Performance uses its own build root and
contract target; Gain-only editor/program switches reject before mutation.
Selected generated bundles receive public notices, local signing and dynamic SDK
validation; unrelated stale outputs are neither signed nor loaded.

The complete Performance pipeline passes its archive contract and 47 SDK tests;
the original default Gain pipeline passes all 12 CTest contracts and 47 SDK tests.
Three new Python profile/rejection/error-propagation boundary groups pass and were
included in the accepted shared regression snapshot. Local execution used external
SDK 3.8.1 on macOS27 ARM64 and checksum-identical test launching for the known
volume startup delay. No host installation, production signing, native editor/
program revalidation, publication or other-platform claim. See decision0176.


## W167: Explicit local-video worker extraction — completed locally

The new separate ravet-media prototype opens bounded explicitly selected local
video and extracts first-track timestamped complete RGBA frames through a narrow
AVFoundation boundary. Native assets stay on their creation worker/UI thread;
returned data is owned and no window/audio/device/network URL/playback clock is
selected. Paths/files/duration/dimensions and complete frame output are bounded;
native decoder transient allocations/I/O/time remain OS-owned and unbounded.

Three actual H.264/MP4 integration groups plus the thread-transfer compile-fail
doctest pass on macOS27 ARM64. They check independent size/duration/source times,
out-of-order frames, red/blue regions and top-white-strip orientation, malformed/
missing/empty/oversized/non-UTF8 paths, rejected timestamps and twelve worker
create/extract/drop cycles. A native draft probe exposed an inverted raster row
transform; the final renderer removes it and the independent orientation test
passes. Lossy/native color conversion tolerance is explicit. All-target package
Clippy with warnings denied passes. Shared workspace/API docs are grouped with
the next accepted parallel snapshot. No general codec/colorimetric/AV sync/live
playback/GPU-window/other-platform support follows. See decision0178.


## W164: Prepared paragraph source carets and selections — completed locally

CaretMap derives Unicode extended-grapheme byte boundaries, bidi/wrap affinity
positions, nearest-line/pen hit tests and complete caller-buffer visual selection
boxes from immutable paragraph source/layout identity. Lookup/hit/selection avoid
heap actions after preparation; invalid ranges/capacity preserve output slots.
Ligature interiors use explicit equal-grapheme interpolation; unsupported backend
clusters reject. Unicode17 graphemes remain separate from Unicode15 breaks and
Unicode16 bidi. Hard separators have no selection pen width.

Six focused groups and all 27 surface tests/doctest pass, plus package all-feature
all-target Clippy with warnings denied. The first-party generated ffi fixture
regenerates byte-identically and shapes as one glyph; independent caret positions
are 0/24/48/72 at size72. Tests include RTL combining, bidi/wrap affinity,
CRLF/empty lines, invisible ties/zero-width selections, extreme finite coordinates,
full source capacity and 3000 heap-free prepared operation iterations. Collaborative
Chromium renders 73 finite paths in720x320 with distinct byte3 affinities and two
disjoint bidi selection boxes. No interactive editor/IME/clipboard/native caret,
screen-reader, font GDEF/ink precision or complete Unicode support claim. Root
review found no new source-mapping/selection issue. See decision0175.


## W163: AUv2 native performance MusicDevice reference — completed locally

Implemented separate public aumu/RvPS/Ravt component, native performance archive plus embedded VST3, zero input/mono-stereo f32. Owned SDK subclass/factory provides absent-identity MIDI channel/key release; fractional StartNote tuning and unquantized float velocity with nonwrapping opaque group/key IDs; bounded expanded queue and timed Level; malformed canonical state and AU dictionary preflight before SDK mutation; coherent processor-authoritative saved streams; caller-supported silence/tails on rejected render. Explicit maximum-frame factory-selector preflight is necessary because AUBase bypasses the virtual property setter. Invalid formats/capacities reject before configuration; null property outputs are guarded. Full-width UInt32 MIDI words are preflighted before SDK byte narrowing; malformed status/data, undefined system statuses and nonzero unused arguments reject while retaining queued notes. Defined non-note short channel/system messages are ignored; SysEx start/end are unsupported. SDK files remain external.

Shared au_bundle.py reuses root vst3_bundle runtime; --plugin performance produces a self-contained public component including embedded module and applicable notices. Gain stays default and retains editor/program flags. No global host installation or distribution signing.

Executed:
- Whole final Performance AU bundler via programme runner PASS: target/w163-final-bundle.log, actual packaged component native contracts plus nested performance VST3 contracts/SDK47 tests, 0 failures. Component target/au-performance/release/AU/ravet_performance_au.component.
- Whole default Gain AU bundler regression PASS: target/w163-gain-regression.log; original VST3 SDK47/0 and actual AU gain lifecycle/audio/bypass/state/tails contracts.
- Actual probe mono/stereo48k: analytic timed440Hz attack/velocity, same-key channels, fractional identified notes/group/float velocity, same-frame MIDI zero/order, offset8 Level, zero-frame control, coherent complete Level state, malformed product/identity/controller-type rejection, maximum1023 MIDI batch/rejection, invalid active offset recovery, AUBase4097 vs4096 prefix silence/tail, SysEx reject, 200 render/release cycles. Invalid f64/interleaved-stereo/nonfinite rate and zero/65537/UINT32_MAX capacities reject; previous capacity retained.
- Final focused actual packaged-module probe after full-width MIDI hardening PASS: target/w163-midi-final-contracts.log; 9 malformed full-width/short-message cases preserve the previously accepted timed note and six defined ignored channel/system cases leave analytic audio unchanged. C++ rebuild warning-free: target/w163-midi-build.log. Bundler/runtime were unchanged after the whole-bundle pass.
- Python AU runtime boundary suite4/4 PASS, strict C++17 fixture syntax -Wall -Wextra -Werror PASS, git diff --check PASS, docs checker PASS before root registry integration. No Rust API/manifest changes in W163.

These are local macOS arm64 actual-binary/in-process AudioComponentAPI results, not DAW discovery/session/auval, physical devices, sandbox, allocation/deadline, other architecture/platform, accessibility or production support.


## W166: Automatic recording worker ownership — completed locally

W166 / ADR0177 automatic recording worker ownership COMPLETE, pending root grouped workspace gates.

Implementation:
- RecordingWorker<P>/RecordingPump/RecordingStatus/RecordingExit drive existing WAV, AIFF and optional FLAC bounded recorders with explicit prepare/start/stop/bounded-final-drain/finalize and owned source/original-error reclamation.
- Existing automatic read-ahead thread ownership extracted into private BackgroundWorker without changing playback contract; OS spawn failure retains source, cooperative stop/join/drop remain control-only, worker panic never reports success, no implicit Drop finalization.
- Queue token preflight rejects foreign producer before stop; direct finish_stopped requires reclaimed unique producer. from_recorder accepts hidden callback producers; AudioProducer Drop permanently publishes Release retirement, AudioConsumer diagnostics/worker Acquire retirement proves no later publication before joined bounded final drain. Live/leaked producers reject before stop. CPAL/physical shutdown timing is external, not established offline.
- Recording faults preserve original error/source until reclaimed; saved error returned by finish once. Consuming finish fault loses sink and remains Failed; Finalized only on successful complete sink return. Explicit accepted-prefix overload and backend count semantics unchanged; prepared capture push remains heap/lock/I/O-free.

Exact successful scoped commands, all via serial programme runner:
python3 target/programme-command-runner.py cargo test -p ravet-audio-file --all-features --test recording --test automatic --test aiff_buffered --test buffered --test flac_recorder --locked
  PASS 26 tests (6 new recording groups + 20 regression groups).
python3 target/programme-command-runner.py cargo test -p ravet-device --test recording_worker --test input --locked
  PASS 7 tests (2 actual PreparedCapture hidden-producer ownership integration tests + 5 input regressions), default features.
python3 target/programme-command-runner.py cargo test -p ravet-audio-file --test recording --test automatic --locked
  PASS 11 tests (5 default recording + 6 default playback groups).
python3 target/programme-command-runner.py cargo clippy -p ravet-audio-file --all-features --all-targets --locked -- -D warnings
  PASS.
python3 target/programme-command-runner.py cargo clippy -p ravet-device --test recording_worker --locked -- -D warnings
  PASS.
python3 target/programme-command-runner.py cargo fmt -p ravet-audio-file -p ravet-device --check
  PASS.
git diff --check
  PASS.

Environment correction: default ravet-device dependency initially reused stale pre-RecordingWorker audio-file artifact despite changed unconditional source. Narrow cargo clean -p ravet-audio-file removed only generated package artifacts and forced correct dependency rebuild; all above then passed. No broad clean, Cargo.lock modification, private project/process changes or backend claims.

Files: new src/background.rs, src/recording.rs; extracted src/automatic.rs; adapters buffered.rs/pcm_buffered.rs/aiff_buffered.rs/flac_recorder.rs; public ring retirement/lib exports; tests/recording.rs and ravet-device/tests/recording_worker.rs. Guide audio-files.md and ADR0177.
Root integration record: target/w166-feature-record.toml. Add automatic background.rs to existing automatic-read-ahead sources; existing audio-handoff-ring summary can mention permanent producer retirement + diagnostic, tests add recording.rs and device recording_worker.rs, limits clarify live/leaked producers never prove stopped. Input feature existing no-integrated-recording limit should narrow to no physical validated integrated recording; synthetic automatic adapter is now exercised.
No physical capture/route, deadline, long-session clock drift, resampling or platform claim.

Root review confirms original source/error retention, explicit consuming-finish
failure and producer-retirement acquire/release ownership. No new issue found.


## W163–W167 accepted shared validation snapshot

After integration of AUv2 performance, paragraph caret/selection, public VST3
profile packaging, automatic recorder ownership and local-video frames:
`target/w163-167-gates.log` completed successfully through the serial programme
runner, two compiler jobs and checksum-preserving local executable runner.
All-feature workspace tests and doctests:683 passed, zero failed/ignored.
Workspace/all-target/all-feature Clippy with warnings denied, default workspace
doctests, default and all-feature warning-denied Rustdoc, workspace format,
documentation catalogue,43 script regression tests,4 host-evidence regression
tests and git diff whitespace checks all passed. This accepts the current scoped
contracts; platform/DAW/device/deadline limitations remain as recorded per package.


## W168: Explicit Metal offscreen rectangles — implemented locally

Separate `ravet-metal` macOS package outside the umbrella/processing graph owns
an explicit thread-confined device/queue/pipeline/shared texture and bounded
instance buffer. Portable f64 rectangles clip then convert to f32 endpoints;
ordered source-over premultiplied RGBA8Unorm readback returns independent owned
images, with no window or implicit gamma/color-space conversion. Limits:
1..4096 dimensions,0..4096 raw input rectangles,64 MiB output and67,108,864
conservative clear/covered-pixel work. Complete input/work faults reject before
submission; native failure exposes no partial public frame. Native resources,
creation/submission/wait/readback/destruction are not globally time/memory bounded.

Actual local macOS27 ARM64 M4 GPU strict native probe passed after correcting a
reserved shader parameter and adding explicit failed-library/function guards.
`target/w168-metal-tests.log`: three independent GPU integration groups PASS,
including13x9 non256-aligned packed readback, CPU half-open/source-over pixel
oracle (component tolerance2), clipped extremes, empty capacity/work rejection,
owned prior image stability, repeated reuse and12 worker-owned rendering cycles.
Thread-transfer compile-fail doctest also PASS. All-target package Clippy with warnings denied PASS (`target/w168-metal-clippy.log`);
next shared workspace/doc gates remain pending. No display-window/path/text/image rendering,
antialiasing, physical presentation cadence, other-GPU/platform or deadline claim.


## W171: Reusable AUv2 native schema/control/state adoption — completed locally

Added NativeAuParameters fixed-bound immutable native-cache owner: full UInt32 IDs, complete retained CF names/units and normalized AU f32 metadata/list; declared writable-ID/value/offset preflight before SDK mutation; native range/log/step/ties-up canonicalization; full physical round-trip formatting and bounded strict ASCII parsing with unchanged outputs on failure. CF lifecycle/text is control-owned, cached scalar conversion does not borrow DSP or consumer providers. AU normalized-f32 and Apple SDK property-dispatch quirks remain explicit.

Added NativeAuState private publication validator paired to the exact schema/native state tables: decodes both native streams and exact typed AU dictionary identities before SDK restore, accepts explicit native migrations, cohere validates processor-authoritative saved streams before dictionary mutation. It never borrows live DSP or publishes a restore consumed by audio. Backend rollback/host bypass/notifications remain consumer responsibilities.

PerformanceAU consumes schema/list/info/getter/setter/text/state helpers, refuses invalid-helper initialization and replaces duplicated codec/CF guards. Render/note/lifecycle/preparation limits remain unchanged. CMake and performance bundler execute the new native-helper fixture alongside actual component tests.

Executed:
- Whole normal Performance AU bundle command via shared programme runner PASS, target/w171-bundle.log: actual self-contained component, native helper fixture, nested Performance VST3 contract and external SDK47tests/0failures. Selected performance fixture retains all mono/stereo200-cycle timing/state/fault checks and now tests actual native schema IDs/name/role, physical text complete conversion, malformed text outputs preserved, unknown UInt32 getter/setter and element/offset preflight preserving queued notes.
- Actual default Gain component regression PASS, target/w171-gain-regression.log (registration/schema/stereo/audio/bypass/complete gain recall/preserved host bypass/tails); no Gain-source or default-profile behavior changes.
- Native helper fixture PASS actual Gain/Performance tables and Gain20-byte explicit legacy-state migration; full UINT32_MAX IDs; log/step/ties-up and readonly roles; complete128-UTF16-unit supplementary name past String128 limit; caller-retained names after helper retirement; bad text/embeddedNUL/oversize/mapping failure and malformed schema/table/factory/companion rejection with safe retirement. State wrong types/partial-preset/codec/save coherence rejection is isolated and atomic before live mutation.
- Both fixtures strict C++17 -Wall -Wextra -Werror syntax PASS; normal CMake build warning-free, target/w171-build.log; AU runtime Python4/4 PASS; docs checker and git diff --check PASS before root registry integration. No public Rust API/manifest change; shared Cargo was only invoked by normal native bundle under programme lock after the audio-agent lock-safe notice.

Local macOS arm64 external-SDK/native actual-binary/component evidence only. No global installation, SDK source vendoring, DAW discovery/session/auval, physical devices, sandbox, other architecture/platform, all runtime heap or callback deadline claims.


## W169: Bounded source text editing and layout identity — completed locally

TextEditBuffer owns<=4096 UTF8 source/replacement bytes and<=64 exact snapshots
across undo/redo. Grapheme-safe directional selection, replacement/backward/forward
delete, post-join forward caret snapping and exact no-ops/errors preserve contracts.
Unique nonwrapping process-local owner identity and monotonic revision invalidate
foreign/stale/undo-restored tokens and PreparedTextLayout source caches. Selection
changes conservatively invalidate cache identity without entering/clearing history.
Font/style identity remains caller-owned; no native/DOM/IME/clipboard adapter inferred.

Root source review found no new issue. target/w169-tests.log:21 focused checks
(2 owner/revision exhaustion,7 editing,6 paragraph,6 caret) PASS.
Current compiler deprecated Atomic.fetch_update; rename to try_update preserves
contract, then2 exhaustion checks PASS (target/w169-exhaustion-final.log).
All-feature/all-target warning-denied package Clippy PASS (target/w169-clippy.log);
2 docs PASS including new edit/undo example (target/w169-doctests.log).
Documentation/diff PASS before registry integration (target/w169-docs.log).
No dependency/lock change, native input/accessibility/deadline or platform claim.


## W172: Explicit Metal bitmap scenes — implemented locally

Added immutable validated packed premultiplied RGBA8 assets, zero-copy validated
readback adoption and synchronous ordered rectangle/image composition. Whole
scene/resource/work preflight enforces prepared command count,16 distinct bitmap
objects,64 MiB distinct uploads (clipped objects included),64 MiB output and
67,108,864 conservative clear/covered pixels. Clipped f32 destination/source UV,
nearest edge-clamped numeric sampling and source-over preserve explicit color
semantics. Fresh native shared textures stay private to each call; immutable CPU
assets transfer independently of thread-confined renderer/native ownership.

Actual strict native two-by-two nearest-scale/top-bottom probe PASS. Public Rust
`target/w172-metal-tests.log`:3 new actual-GPU groups plus3 rectangle regressions
and thread-transfer compile-fail doc PASS. Independent mixed-scene pixel oracle
covers clipping/scaling/translucency/ordered overlay; invalid exact shape/premul,
17 distinct inputs/raw command/64 MiB upload budgets, duplicate dedup, retained
images and post-rejection reuse are checked. Explicit local VideoReader fixture
frames at0/1s upload at exact2x with every pixel matching decoded source bytes;
no implicit reader, hidden color transform or playback/window is created.
All-target package Clippy with warnings denied PASS (`target/w172-metal-clippy.log`).
Positive16-object boundary follow-up PASS (`target/w172-images-boundary.log`,3groups). Shared workspace
API/documentation gates follow the next stable snapshot. No native window, text/
path rendering, high-quality resampling, cross-GPU/platform or deadline claim.


## W174: Prepared AUv2 native planar bus mapping — completed locally

Added NativeAuAudioBusMap stopped configuration and fixed caller-owned NativeAuAudioBlock. Full native config/layout preflight, explicit independent compact input/output AU indexing for main/input-only/output-only/mixed logical buses, exact planar list/channel shapes, sample byte capacities/alignment/address overflow and all-output-disjoint checks precede pointer publication. Inputs may alias any output. Failed configure/map retains previous valid profile/native views; mapping never writes sample data/tails. Mapped-block copies retarget views to owned arrays. Explicit zero-frame omission keeps declared native channels with null arrays; exact zero-frame lists still validate metadata. No host routing registration is inferred.

Performance Initialize configures mapper to its prepared zero-input/mono-stereo f32 layout; Render validates SDK output buffers before transferring parameters/native processing. Its owned caller selector also preflights valid prepared raw output lists before AUBase processing/copying, rejecting aliases without advancing live phase/envelope or consuming accepted queued notes. Existing AUBase oversized rejection and bounded supported caller silence/tails remain explicit. Cleanup clears retained pointer views only after callbacks stop.

Executed:
- Whole normal performance AU bundler via shared programme runner PASS, target/w174-bundle.log: native audio + parameter/state fixtures, actual self-contained component and nested VST3 contracts/SDK47tests/0failures.
- Final owned caller guard and allocation-counter changes focused rebuilt/re-signed actual component PASS, target/w174-final-component.log. Mono/stereo timing/Level/state/fault/200-cycle suite plus aliased caller supported-prefix/tail/unrelated-buffer preservation; exact recovered oscillator+ADSR decay proves unadvanced live state; accepted queued note survives invalid caller buffers and appears at its original offset next valid block. The phase oracle explicitly includes the native10ms decay following5ms attack.
- Final native audio fixture PASS, target/w174-final-audio.log: four logical main/input-only/output-only/mixed buses retain source order through compact directions; maximum9by64(576output channels), late full/partial overlaps, unaligned/overflow/insufficient capacities, single-buffer interleaving, disconnected late input, wrong counts, oversized active slice, invalid configured/unused/empty layouts, copy view ownership, zero-frame policies and actual native Gain output/in-place + Performance analytic bindings/tails. 2000 maximum accepted and2000 late-fault mapping calls while tracking C++new/delete record0 allocations/retirements, exercising libc++std::sort on both paths. This does not prove all runtime/SDK/Rust heap behavior or deadlines.
- Existing default Gain actual component regression PASS, target/w174-gain-regression.log; no Gain source/profile/default behavior changes.
- Strict C++17 -Wall -Wextra -Werror audio and component fixtures PASS; owned CMake final build warning-free, target/w174-final-build.log; final component fixture rebuild target/w174-final-fixture-build.log. AU Python runtime4/4 PASS; docs checker and git diff --check PASS before root registry integration. No Rust/manifest/dependency changes.

Prototype limits: macOS arm64 actual native/component/offline evidence only. No generic AU host auxiliary routing, global installation, vendored SDK sources, DAW discovery/session/auval, devices, sandbox, other architecture/platform, format negotiation, f64 AU buffers or production/deadline support claims. Framework buffer mapping is separate from SDK scheduling, host registration and native processor ownership.


Validation observation: the first recovered-phase oracle omitted the native10ms
ADSR decay after240samples; its failure alone did not establish advanced phase.
The final complete oscillator/envelope oracle above passes. Raw-caller preflight
is now explicit before AUBase rather than relying on SDK-substituted buffers.


## W170: Optional bounded Ogg Vorbis worker encoding — completed locally

W170 / ADR0181 COMPLETE: optional bounded single-worker Ogg Vorbis encoding.

Public encode_vorbis/VorbisWriter/VorbisFormat, default-disabled vorbis-encode exact vorbis_rs0.5.6 default-features=false; safe local !Send codec, explicit rate/quality/serial, 4096-frame planar batches, total/call/output bounds, finite [-1,1] whole-call rejection, accepted-source/actual-output counts, original partial I/O faults, explicit EOS/flush and no implicit Drop output. Complete source <=4,194,304samples; calls<=65536frames; total<=16,777,216frames; encoded<=64MiB. Mono8..96kHz, stereo16..96kHz, quality-0.2..1.

Bounded first-priming-packet Ogg framing identifies completed identification/comment/setup headers; max physical page65,307bytes, fixed282header scratch, no sync search/trimming. Packet/lacing/body/serial preserved, first packet isolated at granule0, remainder/following sequences/CRC rewritten with checked counters; transformed bytes budgeted. Continued/mixed headers/spanning priming reject; explicit finish rejects buffered partial or missing EOS. Every parser/partial flush/I/O/budget failure gates backend retry/automaticDrop immediately, including constructor failure.

Measured stereo8k backend sustained second-channel degradation isolated independently from framing: channel0 whole46.25dB, channel1 whole7.26/interior6.98dB, direct and reframed native output identical, lag/gain/window edges do not repair. Safe builder offers no coupling control; wrapper rejects stereo<16k before sink writes. No hidden resampling/quality policy. Accepted11 mono/stereo profiles 8/16/22.05/44.1/48/96k as applicable measured FFmpeg native whole-stream37.8849..48.1078dB, exact8193frames and Symphonia PCM agreement<1e-4. Native FFmpeg6.1.1 used /Applications/miniconda3/bin/ffmpeg -c:a vorbis (not libvorbis decoder). Tiny1/17 tails exact independently; backend packet oracle bitexact and exactly+27 transformed bytes in tested streams.

Dependencies: lock added only vorbis_rs0.5.6,aotuv_lancer_vorbis_sys0.1.6,ogg_next_sys0.1.5,tinyvec1.13.3, no existing-version changes. Existing cc/errno/thiserror suffice. Binding exactpublished sourcecommit6934784e7188e98dd053370c79604c72b201e97a and complete BSD binding/libogg/aoTuV/libvorbis notices retained third-party/vorbis.md; LICENSING and guide updated. C compiler needed; no systemcodec/bindgen/libclang/defaultRNG.

Scoped gates (all Cargo through target/programme-command-runner.py, serial lock/jobs2/local identical runner):
- cargo test -p ravet-audio-file --all-features --locked PASS69 unit/integration/doc-tests (2 native tests ignored), before last additional parser retirement test.
- FINAL cargo test -p ravet-audio-file --all-features --lib --test vorbis --locked PASS6 library+7 integration tests,2 native tests ignored; latest parser retirement regression included.
- FINAL cargo test -p ravet-audio-file --all-features --test vorbis ffmpeg_native_decoder --locked -- --ignored --nocapture PASS1 independently invoked native decoder oracle,11 supportedprofiles.
- cargo test -p ravet-audio-file --features vorbis-encode --test vorbis --locked PASS5tests,2 native tests ignored (compressed-specific tests absent).
- cargo test -p ravet-audio-file --all-features --test vorbis diagnose_low_rate --locked -- --ignored --nocapture PASS1 diagnostic before lower-stereo wrapper rejection, evidence retained above.
- FINAL cargo clippy -p ravet-audio-file --all-features --all-targets --locked -- -D warnings PASS after last parser test/type alias.
- cargo clippy -p ravet-audio-file --features vorbis-encode --all-targets --locked -- -D warnings PASS.
- FINAL cargo test -p ravet-audio-file --all-features --doc --locked PASS1 !Send compile-fail contract.
- cargo fmt -p ravet-audio-file PASS; git diff --check PASS.

Parent owns registry/features/work-status/index and joint workspace docs/lint/gates. Suggested registry target/w170-feature-record.toml. ADR0181 scopedimplemented. No codec working-memory ceiling, bitexact/perceptual/automaticcapture/physical recording/RT deadline or cross-platform claim. W175 worker-created !Send capture integration approved separately, target-only until shared snapshot releases freeze.


## W173: Bounded browser textarea editing — completed locally

TextEditor owns one consumer-labelled connected live-document textarea, configured
maximum1..4096 UTF-8 bytes (committed source0..maximum), LF scalar source, browser
Intl.Segmenter grapheme-safe directional byte selections and combined0..64 undo/redo
snapshots. Exact object-identity revisions, nonwrapping exhaustion, quiet consumer
resynchronization, native DOM reconciliation and one pending composition/history
transaction preserve explicit local ownership. Browser Unicode data is unpinned
and independent of Rust EditRevision/Unicode17; no Rust transport/ack protocol is
implied. Native preedit, browser layout/history and OS work remain opaque.

Root review found token changes during selection synchronization could incorrectly
classify a later validation failure as a callback exception. Explicit notification
exception tracking now preserves original direct-call exceptions and correct native
InvalidInput/Capacity/RevisionExhausted diagnostics; committed source/history remain
intact, while legal synchronized selection may advance its revision.

Actual Chromium and hidden WebKit both20/20 DOM contract groups PASS,
target/w173-native-final.log, including changed-selection validation failures and
throwing callbacks. Focus/piano38-case native regression PASS,
target/w173-focus-regression.log. Trusted Chromium typing, backward selection,
deletion, undo/redo, newline and Tab exit also passed. document.hasFocus=false
limits that evidence to DOM keyboard behavior; synthetic composition does not
validate native IME, visual focus, screen readers or mobile keyboards. CMake/docs/
diff focused checks passed; workspace Rust/API gates follow the shared snapshot.


## W168–W174 programme snapshot — accepted locally with concurrent-member limit

Before an independently developed Foundry workspace member was added during the
run, all-feature workspace tests/doc-tests passed712 with zero failures and2
explicit native Vorbis diagnostics ignored (already executed separately).
Evidence: target/w168-174-gates.log. The later Clippy step detected its newly added
manifest; offline metadata resolution added Foundry plus serde_spanned/toml/
toml_writer, no existing package identity/version changes. A complete retry found
its then-missing HTML include; no unrelated application source was repaired.

Remaining programme Cargo gates passed with that concurrent member explicitly
excluded: all-target/all-feature Clippy with warnings denied, default doc-tests,
warning-denied default and all-feature Rustdoc. Evidence:
target/w168-174-gates-remaining.log. Whole-workspace format check reported only
concurrent Foundry differences; explicit programme package formatting passed.
The owner subsequently added Foundry's registry evidence, allowing unmodified
check-docs.py --write/check to pass.43 packaging/documentation Python and4 host
evidence Python tests plus git diff --check passed:
target/w168-174-final-checks-retry.log. This snapshot does not validate Foundry or
claim a final complete-current-workspace gate; its full build/format evidence
remains separate. W175–W180 implementation tracks resume after this scoped snapshot.


W175 / ADR0186 Worker-created automatic Vorbis capture ownership — complete 2026-10-10.

A dedicated prepared-ring controller creates/uses/finalizes/drops the genuinely !Send codec on its single worker. Only transferable Send preparation/result crosses threads. Acknowledged pause/resume, bounded stopped ring-tail drain, exact producer identity/permanent retirement preflight, retained startup/pump/finish errors, private-constructed Finalized/Aborted/Failed completion evidence, and recovered destination/consumer/counts implemented. Outer Ok proves recovery only and must not be confused with finalized file evidence. Codec/sink panic loses unwound local owners and reports Poisoned; no synthetic recovery. Spawn failure preserves preparation. Abort/Drop never drain/EOS/flush; downstream sink destructor separate. Existing W170 public writer consuming new/finish error signatures unchanged.

Default-disabled ravet-device vorbis-recording forwards optional vorbis-encode for actual PreparedCapture hidden-producer integration. Synthetic concurrent capture validates no callback heap use, accepted source counts and exact final Ogg granule; no physical CPAL shutdown, input clock drift, device deadline or total backend memory claim. Existing W170 supported profiles/parser/frame/output bounds remain.

Focused tracked receipts, all via python3 target/programme-command-runner.py:
PASS cargo test -p ravet-audio-file --all-features --lib --test vorbis --test vorbis_recording --locked: 7 library +7 writer +11 worker =25;2 optional external decoder/diagnostic groups ignored (independent evidence recorded W170).
PASS cargo test -p ravet-device --features vorbis-recording --test vorbis_recording --locked: 2 actual PreparedCapture integration groups.
PASS cargo test -p ravet-audio-file --features vorbis-encode --lib --test vorbis_recording --locked: 6 library +11 worker=17, without compressed dependency.
PASS cargo clippy -p ravet-audio-file --all-features --all-targets --locked -- -D warnings.
PASS cargo clippy -p ravet-device --features vorbis-recording --test vorbis_recording --locked -- -D warnings.
PASS cargo test -p ravet-audio-file --all-features --doc --locked: 2, worker example +writer !Send compile-fail.
PASS cargo fmt -p ravet-audio-file -p ravet-device -- --check before final ADR status update; final queued recheck no Rust changes.
PASS git diff --check scoped audio-file/device forwarding/tests/audio-files guide/ADR.
Isolated earlier unique own-lock package independently passed25 tests,Clippy,2 doctests before tracked release.

Tests exercise prepared abort/no codec headers, shape/format/budget rejection, injected OS spawn failure/retry, worker-local sink thread evidence, concurrent fixed-ring capture/backpressure, repeated32 pause/resume handshakes, failed live/foreign/leaked proof preserving state, stopped/retired tail drain including neverstartedworker, exact independent decoded timing+lossyPCM, empty stream failed ownership, wholebatch invalid source with preserved queue suffix, separate dequeued/accepted counts on capacity, startup partial-header original sink error, postacceptance sink/output faults, flush failure, paused abort preserving suffix, no finalization I/O on abort/Drop, panic without false ownership/completion.


## W177: AUv2 block-start host transport preflight — completed locally

Added immutable NativeAuTransportSnapshot and stopped-independent native availability sidecar. Each supported supplied callback executes at most once; State2 takes precedence with no legacy fallback after error. Missing/error tuples remain unknown. Successful incomplete/malformed tuples reject atomically. Foreign returned float sentinels use integer-bit initialization and IEEE finite checks so optimized fast-math cannot silently retain zero fields. Samples receive finite/integral/range checks before i64 casts; signature numerators/denominators receive exact bounds before u16 casts. Actual changed/discontinuity/cycling and State2 recording survive direct native collection, while unsupported bar/clock/pre-roll/sample-loop meanings are not inferred.

Canonical native signature requires DETAILS. Signature-only collection therefore uses unknown/default detail flags, with callback sidecar distinguishing those placeholders and legacy unknown recording from actually supplied State2 recording. All pre-roll values remain unknown/default. The native codec has no per-flag DETAILS availability; consumers needing that distinction must use the sidecar. The external VST3 projection requires actually known playing and samples before publishing a nonnull context, retaining partial independent native music/signature in the snapshot while explicitly omitting it from that bridge. No system clock or other missing fields are invented.

Performance Render adopts collector/projector after audio/note preflight and before transferring pending SDK parameter intent or invoking native processing. Malformed successful host callbacks produce supported-prefix silence/tail preservation without consuming accepted timed MIDI/Level or advancing native voice history. Errors/absence remain valid unknown transport. AUBase still owns timestamp caching including failed renders; tests recover with a new timestamp. Performance DSP currently ignores transport, so acceptance proves boundaries/scheduling rather than transport-driven DSP.

Executed:
- Whole normal Performance AU bundler through shared programme runner PASS, target/w177-bundle.log: nested public Performance VST3 contracts plus SDK47/0, Apple SDK build, self-contained ad hoc signed public component/notices, all five AU validators. Actual mono/stereo component tests include typed HostCallbackInfo, +2^63 fault silence/backing tails, preserved accepted timed MIDI and Level recovered at -2^63, fractional/missing returned signatures, callback errors/State2 precedence, legacy and absent callbacks, plus existing note/state/audio fault and 200 synthetic voice cycles.
- Normal and -O3 -ffast-math native transport fixtures PASS through the same packaged CMake targets. Four thousand accepted and four thousand rejected collector/projection cycles each use zero counted ordinary C++ heap actions. Initialized missing values, -2^63/+2^63 and neighboring range bounds, fractional sample/signature, nonfinite foreign bytes, strict Boolean/loop/order, independent availability, sidecar/default DETAILS and incomplete SDK projection are covered. Actual authoritative Rust performance zero-frame processing accepts complete/signature-only/music-only/legacy-only/unknown collected snapshots and rejects missing DETAILS dependency, reserved-field and unused-coordinate corruption.
- Existing default Gain actual component regression PASS, target/w177-gain-regression.log. Gain sources/profile/transport behavior unchanged.
- AU Python runtime4/4, documentation checker and git diff --check PASS before root feature registry integration. No Rust/Cargo/lock metadata changes on this track.

Prototype limits: local macOS arm64 native/actual component/offline evidence. Foreign callback coherence/ownership and callback allocation remain host obligations. No DAW timeline/auval/global registration, device/sandbox/other architecture, all-SDK/Rust heap or deadline/production distribution support claim. SDK sources stay external.


## W178–W179: Local document ownership and popup menus — completed locally

Parameter gestures now share ownership per original live document, with connected
same-document paired inputs validated before mutation and owner-window lifecycle
routing. Rejected end/disposal retains ownership until accepted retry. PopupMenu
composes one level of 1..256 consumer-created native action buttons, unique bounded
action strings, roving focus, native keyboard activation, outside dismissal and
exact owned-attribute restoration. Focus restoration and notification share a
reentrancy guard; disposal during focus cancels action and nested trigger labels
retain trigger ownership.

Integrated Chromium and hidden WebKit each passed31 owner-document,25 popup,
20 textarea and38 focus/piano groups (114 total). Receipts:
target/w178-179-integrated-chromium.json and native owner-documents/popup-menu/
text-editor/focus logs. Trusted keyboard cases explicitly reopen between keys
because hidden automation blurs the window; document.hasFocus=false establishes
DOM behavior only. No actual screen-reader/native-window focus certification,
cross-origin/shadow access or automatic frame/stylesheet retirement claim.


## W176: Optional CPU paths/strokes and local-font pixels — completed locally

Default-disabled surface/umbrella raster features pin tiny-skia/path0.12.0 with
std only, plus arrayref0.3.9/strict-num0.1.1; exact primary archives/checksums and
complete upstream BSD/MIT notices retained. Minimal offline resolution adds only
those four packages, no removed/version-changed identities; current concurrent
Foundry dependency changes are preserved separately. Owned PMA RGBA8 canvases,
finite affine typed paths, explicit fill/stroke/AA and whole-call budgets reject
before pixels/counters change. Optional text prepares local TrueType geometry
without retained source/font borrows. Positive thin AA uses backend coverage;
zero-width/dash/fallback/window/presenter/gamma policies are not inferred.

Promoted focused tests PASS5 raster-only and7 raster+text groups,
target/w176-focused-gates.log; actual local Metal upload/readback PASS1 comparing
every curved translucent path/Arabic glyph component within1byte. The first Metal
lint rejected a test-only constant chunks_exact; corrected as_chunks passes
all-target/all-feature Surface/Metal Clippy with warnings denied and3 surface
doctests, target/w176-remaining-gates.log. Warning Rustdoc exposed an unescaped
numeric interval; corrected interval code formatting now passes. Final default-free
all-feature LinuxARM64 and iOSARM64 compiler checks PASS,
target/w176-doc-cross-gates.log; those are not target-runtime support claims.

Independent rectangle/source-over equations and fontTools synthetic glyph bounds,
winding/cutout/affine/caps/AA, raw4096draws/cumulative65536commands/67108864work and
whole-run failure preservation cover bounded contracts. A first font oracle forgot
horizontal-side-bearing normalization; corrected independent FontTools BoundsPen
matches actual x[3,100),y[20,90) ink. Public480x240 preview includes Latin/Arabic,
cubic/quadratic stroke and evenodd cutout; target/w176-draft/preview.png inspected.
Backend working memory/flattening/time remain outside input/output/work budgets;
no native editor/GPU path/deadline/other-runtime certification follows.

## W181: Local labelled tabbed panels — completed locally

Fixed1..256 consumer native-button/panel pairs, stable IDs, manual/automatic
activation, independent focus/selection, native Tab navigation and exact owned
attributes restore are implemented. Original-document membership/type/ID/nesting
retirement and null/first eligible lifecycle fallback preserve bounded association.
Root review added full selected/eligibility/actual-focus checks before and after
notification; fallback focus handlers cannot return stale success and callback
membership mutation retires immediately. Quiet focus:false updates explicitly
leave retiring-panel focus policy to consumers; failed focus retains committed
selection with observer/explicit refresh reconciliation.

Integrated Chromium and hidden WebKit both25tabs+31owner-doc+25popup+20textarea+
38focus/piano=139 groups PASS, target/w181-integrated-chromium.json and native
integrated logs. Trusted manual arrows/Enter/Tab and automatic arrows/Space pass,
target/w181-keyboard.json. Hidden document.hasFocus=false/explicit focus-event
wrappers limit proof to DOM behavior; no screenreader/native OS focus certification.
CMake5targets/check-docs/scoped diff pass; shared Cargo/API gates follow.


## W180: Prepared LV2 block-start time:Position transport — completed locally

Added standalone Lv2TimePositionTypes and Lv2TimePosition collection with stopped cached exact URIs and fixed bounded parsing. Complete sequence/header/timestamp/order/property/padding/type/context/duplicate preflight precedes candidate publication. Long signed sample coordinates preserve full i64 range, Double beats preserve exact f64 including signed zero, Float tempo widens exactly; signature bounds/integrality precede casts. Unknown properties and unrelated bounded events stay opaque without recursive validation claims. Missing independent values remain unavailable. At most one offset0Position supplies context, later positions fully validate then omit, no extrapolation/field carry/mid-block dispatch. Unsupported valid speeds reject rather than become stopped. Signature requires canonical DETAILS; recording/loop/pre-roll false flags remain unknown/default placeholders because native DETAILS lacks per-flag availability and this profile supplies no such flags.

NativeLv2Processor gains an explicit fourth template bool Transport=false requiring its existing instrument Atom input. Gain and default MIDI-only processing/URI requirements remain unchanged. The public Performance opts in, advertises Position alongside MIDI, caches all URIs on stopped control and passes the exact direct native snapshot or null when unknown. Full audio/control/MIDI/time preflight precedes pending complete-state apply, control publication and DSP, preserving accepted recall/control/phase/history on malformed time and supported prefix silence/tails. Performance DSP currently ignores transport.

Executed:
- Whole normal Performance LV2 bundle through shared programme runner PASS, target/w180-performance-bundle.log: self-contained public .lv2 with manifest/performanceTTL/module/notices/local ad hoc signature; actual mono/stereo full instrument suite plus normal/optimized-fast-math native transport and forwarded helper validators. Mixed Position+MIDI analytic split/audio timing; missing required mapped time URI; later unsupported speed whole-block faults preserving exact phase/Level/pending complete state; valid exact-minimum sample recovery; supported silence/tails;2000prepared note/release cycles with zero counted ordinary C++ heap actions or URI maps.
- Whole default Gain LV2 bundle PASS, target/w180-gain-bundle.log: default CMake optionalPerformance flag cleared, actual packaged Gain lifecycle/controls/bypass/state/zero-frame/tails/alias/fault contracts,4000ordinary C++heap-free run/save cycles and concurrent coherent save test. Gain factory/control/state behavior unchanged.
- Direct collector normal and -O3 -ffast-math strict C++17 -Wall -Wextra -Werror builds/execution PASS. Exact i64min/max, original Double precision/signedzero, Float widening, independent/partial/unknown fields, type/context/body/extent/padding/unit/order/aliasURI/duplicate-object/property/bound faults, unsupported speeds/nonfinite foreign bytes, maximum128properties/1024events and prior-output retention.2000accepted plus2000late-rejected cycles have zero counted ordinary C++heap actions. Authoritative actual Rust native zero-frame processing accepts complete/signature-only/music-only/sample-only/unknown snapshots and rejects missing canonical DETAILS dependency/unavailable details flags.
- Strict forwarded-native helper fixture PASS: controlled matching native tables deliver exact transport to real Rust processor; late invalid Position yields zero apply/process calls and supported-prefix silence/tail retention; recovery applies pending complete recall and connected control intent; unknown is null; default MIDI-only helper requires no time URIs, ignores opaque unsupported time body and delivers timed notes with null context. No arbitrary foreign-table authentication claim.
- LV2 Python runtime3/3 and docs checker PASS; focused build logs contain no owned warnings. No Rust/Cargo/lock metadata changes on this track. Final git diff --check PASS after root registry integration.

Prototype limits: local macOS arm64 actual-native/packaged-module/offline checks only. No Linux runtime, DAW discovery/session timeline, devices, metadata validation, hardRTCapable, all-runtime/backend heap or deadline/production support. Foreign pointers/ranges remain host obligations, no generic rollback after backend failure/state apply, and SDK sources stay external.


## W175–W181 shared programme snapshot — accepted locally

All-feature full workspace tests/doc-tests PASS746, zero failures,2 known external
Vorbis diagnostic groups ignored (previously executed independently). Current
concurrent Foundry member now participates successfully; the earlier W168–174
exclusion remains a historical limit for that older snapshot only. Whole workspace
all-target/all-feature Clippy with warnings denied, default doc-tests,
warning-denied default+all-feature Rustdoc, full formatting, unmodified docs check,
43 packaging/documentation Python tests,4 host-evidence Python tests and diff check
PASS. Evidence target/w175-181-gates.log, final command process exit0. No skips or
member exclusions used for this snapshot. Individual AU/LV2/native/DOM/Metal/device
recording evidence and limits remain in the package records above.

Tracked source hold released. Next independently prepared packages MP3 encoding,
worker image codecs, modal dialogs and native AU MIDI output remain drafts until
focused review/validation; shared success does not make those APIs implemented or
establish physical device/other-platform/DAW/production/deadline support.


## W182: Optional bounded MP3 worker encoding — completed locally

Default-disabled mp3-encode supplies complete/incremental MPEG1 Layer III encoding
with explicit mono/stereo32/44.1/48k and CBR192/256/320 highest-quality profiles.
Whole finite normalized source preflight precedes codec work; prepared4096-frame
batches/output scratch, total source and physical-write budgets include finalized
Info-frame replacement. Write+Seek must start at EOF; original seek/I/O failures
retain sink ownership and diagnostics without unsafe restoration retries. Failed
writes expose a borrowed retained original error; consuming finish recovers it.
Abort/Drop never encode/finalize/write/seek/flush and intentionally discard public
abort diagnostics. Exact parsed MPEG/Info/LAME3.100 CRC/delay/padding/source/file
counts precede final rewrite, end restoration and explicit flush.

Tracked focused gates PASS:17 all-feature library/matrix/fault tests;10 MP3-only
library/fault tests;all-target/all-feature Clippy with warnings denied;3 doctests;
scoped formatting/diff. Explicit independent native FFmpeg completed-file test
PASS72 bit-rate/rate/channel/length profiles with exact source counts and maximum
PCM difference3.359e-5 versus independent Symphonia. Physical data-frame bitrate,
header/frame counts/tag CRC and one-frame partition scratch stability pass.
Sink fixtures cover retryable whole-call rejection, payload-identical original
errors, partial/zero/overreported writes, failed seeks, metadata rewrite/flush,
poisoned no-retry and nonfinalizing abort/Drop. Evidence target/w182-status.txt.

Raw gain/SNR exposes the pinned192kbps preset's0.97 input scale without framework
compensation;256/320 use1.0. Gapless-aware completed-file readers preserve exact
source count; seekless stream players can retain padding. Bundled static LAME3.100
requires Unix configure/make/C tools; only local macOS arm64 evidence. Complete
independent LGPL3 binding, LibraryGPL2-or-later core and LGPL2.1-or-later ReplayGain
notices/40 component attributions are retained; RAVET commercial terms do not
relicense or certify combined/static distribution. No physical capture, generic
perceptual, all-platform or deadline claim. Automatic MP3 capture is W186 pending.

Joint W182/W183 offline lock resolution adds14 exact package identities, removes
none, preserves every existing package version/identity and changes only the two
owned direct package dependency lists. Shared/API documentation gates follow.


## W183: Optional owned PNG/JPEG worker assets — completed locally

Default-disabled Surface/umbrella image features provide explicit byte codecs,
validated immutable straight RGBA8, consuming integer PMA conversion and bounded
private-sink PNG output. Whole PNG chunk/JPEG marker-and-entropy preflight checks
source/extents/dimensions/completion before framework output allocation; backend
strict validation retains CRC/Adler checks. Static PNG expansion/Adam7/high-byte
16-bit and selected8-bit mono/RGB/YCbCr baseline/progressive JPEG are deliberate
profiles. Numeric components receive no ICC/gamma/EXIF orientation transform.
Source16MiB, dimensions4096/output64MiB,8192records and32scans are framework limits;
backend working memory/allocation/time remain unbounded worker obligations.

Tracked focused PASS:6 independent-oracle/fault/budget groups;actual Metal decoded
PNG PMA adoption/readback1group, all components within1byte;Surface+Metal all-target
all-feature Clippy warnings denied;4 Surface doctests;warning-denied Rustdoc;
ARM64Linux+iOS compilation only. Evidence target/w183-public-gates.log, final
process exit0. First-party Python/Pillow generator reproduces all24 PNG/JPEG/raw
fixture files byte-for-byte; provenance retains hashes and exact local tools.
Seven JPEG profiles differ at most3 numeric component bytes from Pillow, with4byte
fixture tolerance. Truncated prefixes, trailing bytes, CRC/Adler corruption,
animation, header/scan/marker bounds and output budgets reject explicitly.

Exact png0.18.1/zune-jpeg0.5.15/core0.5.3 and complete backend/transitive notices
were reviewed against primary archive hashes. JPEG std-only/safe runtime; selected
PNG pure-Rust miniz backend, optional zlib-rs lock entry unselected. Default normal
graph adds no image dependency. No JPEG encoding/GIF/WebP/animation/SVG parser,
window, color-management, all-platform/device or deadline claim. Shared/API gates
follow alongside W182–185.


## W185: Bounded AUv2 native MIDI output delivery — completed locally

Added exact canonical native short MIDI and ordinary nonidentity-note conversion without invented identity/completion semantics. Finite/exact-f32 scalar, unused positive-zero/detail/word fields, MIDI status/length/data and active-slice checks precede publication. Fixed external CoreMIDI owner validates whole ordered lists, stages complete packet storage and delivers one callback with exact render timestamp/output index. Reentrant stage/delivery cannot mutate borrowed serialized packet bytes. Fresh failed stages preserve diagnostics but invalidate readiness, and every delivery attempt consumes readiness. Empty output needs no callback; missing callback/time and host refusal cannot replay stale data. Typed receipts preserve exact host error and attempted records, with acceptance unknown and no output/history rollback.

A separate synthetic actual external-SDK MusicDevice AudioComponent owns retained output-name metadata and copied callbacks, performs prepared staging before a real public native Performance process, then delivers controlled fixture records. Production Performance continues advertising no MIDI output. Raw factory selectors prove owned exact-size/scope/element/stopped setter validation; observed public AudioToolbox size/element normalization and cached protocol routing are explicit limits. No original-facade rejection/atomicity is claimed from raw SDK results.

Executed:
- Normal Performance AU bundle through shared programme runner PASS, target/w185-bundle.log: nested VST3 SDK47/0, bundled real Performance component and all eight AU validators. Actual synthetic component properties/render/native ordering, max1024notes, exact render timestamp/index/bytes, host refusal/no replay, supported late-fault silence/tail retention/pre-DSP process count and recovery PASS.
- Default existing Gain actual-component regression PASS, target/w185-gain-regression.log: local checksum-identical executable against unchanged Gain bundle, native parameters/stereo rendering/bypass/coherent recall and tails.
- Promoted direct normal and -O3 -ffast-math strict C++17 -Wall -Wextra -Werror builds and execution PASS, target/w185-strict.log and target/w185-fast-strict.log. Real Rust native ingress accepts every supported defined MIDI channel/system form and ordinary note; malformed canonical cases reject authoritatively. Actual native full-identity NoteEnd is unsupported and produces no host callback. Exact packet contents/order, minimum1/max1024template bounds, foreign address overflow, late faults/prior retained diagnostics, readiness consumption, status-77 refusal and actual serialized packet bytes across reentrant calls PASS.2000maxaccepted/late-fault cycles count zero ordinary C++ heap allocation/deallocation; this excludes backend/host/CoreMIDI deadline and allocation guarantees.
- AU Python runtime4/4, documentation checker and git diff --check PASS. No Rust/Cargo/lock metadata changes on this track. Final source headers/fixtures are public; external SDK remains outside repository.


## W184: Local native modal dialogs — completed locally

ModalDialog composes a consumer-created native dialog with bounded labels/values,
explicit initial focus, browser-owned modality and guarded close/form/cancel
completion. One original-document owner survives queued live close through
notification; one node owner prevents replacement replay. Disposal cancels
callbacks, restores original attributes/returnValue and releases document
admission only after synchronous native close/focus and callback unwind. A
per-node tombstone remains until actual trusted close; synthetic events cannot
drain it. Different nodes remain usable after retired-node event abandonment,
while same-node reuse requires actual drain without guessed timeout/deadline.
Unsupported external reopening/open removal receives guarded native cleanup;
previous-focus restoration under that ownership violation is not guaranteed.

Chromium and hidden WebKit29 modal groups each PASS, then public integrated
29modal+139existing=168 groups each PASS. Six native CMake targets build PASS;
trusted Tab/ShiftTab/Escape closes exactly once and restores prior outside focus.
Receipts target/w184-integrated-chromium.json, target/w184-integrated-*.log,
target/w184-keyboard.json. Hidden preview hasFocus=false/browser focus-boundary
and explicit fixture focus dispatch limits remain. Native Chromium close events
were delayed but completed without framework source/timing workaround. Removing
retired nodes or closing frames/windows may abandon scheduling; no node reuse,
OS focus, accessibility/screenreader or drain deadline is inferred. Strict docs
passes, tracked source frozen for shared W182–185 validation.


## W182–W185 shared programme snapshot — accepted locally

All-feature workspace tests/doc-tests PASS766, zero failures,3 external decoder
checks ignored in the shared run (2 Vorbis diagnostics and1MP3 FFmpeg matrix,
explicitly executed independently). Workspace all-target/all-feature Clippy with
warnings denied, default doc-tests, warning-denied default+all-feature Rustdoc,
full formatting, strict documentation check,43 packaging/documentation Python
and4 host-evidence Python tests and diff check PASS. Evidence
 target/w182-185-gates.log, final process exit0. Foundry participates, no member
exclusions. Individual MP3/oracle/Metal/DOM/AU receipts and limits remain above;
no new DAW/device/other-platform/runtime-deadline support follows.

Accepted package source hold released. W186 automatic MP3 capture, W187 AU native
64-bit profiles, W188 concrete control semantic/disabled contracts and W189 JPEG
writing remain isolated drafts until their own review/gates. Concurrent native
context-menu work belongs to its separate owner and requires its own runtime
receipts; shared Cargo commands do not establish that native capability's behavior.


## W187: Prepared AUv2 native f64 mapping and complete formats — completed locally

Generalized fixed prepared audio block/map storage to explicit float/double types while preserving original f32 aliases/API. Double mapping uses8byte samples/alignment without conversion, preserves native main/input-only/output-only/mixedaux order through compact independent directional AU element arrays, and performs full shape/capacity/address/outputdisjoint preflight before candidate publication. Copy-retargeting, failed-map retention, zero-frame shape/omission and foreign ownership rules remain unchanged.

Added complete stopped ASBD/native-entry profiles. Exact structural table precision/functions and native configuration/directional counts, finite equal sample rate, native packed float encoding, mono planar-flag exception, chosen bits/bytes/frame/packet, channel shape and reserved0 all validate before accepted profile/config/map publication. No temporary mixed profile is dispatched. SDK mutations/allocations/notifications remain consumer owned; candidate helper atomicity is not arbitrary SDK rollback. Stock Apple StreamFormat setter copies36bytes/discards original reserved/padding, and unchanged active ASBD can succeed before writable checks, explicitly excluded from direct helper original-facade guarantees. Parameter companion creates its handle via its own processor_entry; separate exported table address equality is not assumed.

Separate actual fixed64 MusicDevice component retains distributed Performance32 factory, owns stopped mono/stereo format negotiation and native64 setup/render, rejects changed initialized formats and supports exact unchanged no-op. Complete f32 Level ClassInfo state validates component identity/native codec before stopped recall; voices/history, normalized AU parameter UI and cross-precision migration remain outside this fixture. Caller faults retain backing tails and perform no native DSP before supported silence/recovery.

Executed:
- Normal public Performance AU bundle through shared runner PASS, target/w187-bundle.log: nested VST3 SDK47/0 and all eleven AU validators, including unchanged distributed32 component plus actual64 helper/component validators. No owned warnings.
- Default existing Gain actual component regression PASS, target/w187-gain-regression.log: checksum-identical local executable validates parameters/stereo render/bypass/coherent complete recall/tails against unchanged Gain component.
- Strict C++17 -Wall -Wextra -Werror normal and -O3 -ffast-math draft builds/execution PASS, target/w187-audio64-normal.log and target/w187-audio64-fast.log; promoted equivalent binaries execute in normal bundle. Actual Gain64 .25+2^-40 survives exactly through mapped output/in-place processing, real Performance64 timed analytic notes and zero-frame bindings, full main/aux direction order, float-aligned/double-unaligned pointer rejection,8byte capacity, alias/overlap/address/count/order/copy/late fault retention and maximum576outputs.2000maximum accepted/late-fault maps count no ordinary C++ heap allocation/deallocation. Complete format profile tests cover exact32 matching regression,64 metadata precision, malformed late ASBD bytes/flags/rates/nonfinite/reserved/channels, foreign array overflow/missing/count faults and prior profile retention.
- Strict C++23 actual synthetic AudioComponent PASS, target/w187-component.log and normal package validator: stopped stereo64 negotiation/readback and f32/interleaved/bytes/rate/channel rejection; initialized changed format rejection/unchanged no-op; timed MusicDevice notes/real native64 AudioUnitRender analytic stereo; late caller-byte fault before DSP with bounded complete-sample silence/tails/recovery; complete native64 Level save/changed recall, wrong subtype/malformed stable-ID rejection retaining accepted Level; stopped mono44100 reprepare preserving Level and exact221-sample attack oracle.
- AU Python runtime4/4, docs checker and git diff --check PASS. No Rust/Cargo/lock metadata changes on this track. External SDK remains outside repo.


## W188: Local control semantic ownership — completed locally

Custom Knob enabled state coordinates owned tabindex/ARIA and paired native number
disabling/restoration. Rejected or throwing end retains original ownership and
blocks edits/re-enable until accepted retry. Healthy disabled host refresh remains
eligible under existing focus/active rules. Original pair retirement permits only
end recovery; callbacks revalidate admission after existing-owner end before a new
begin/edit and again before DOM painting. An accepted host message is irreversible
if its callback retires the target; stale UI painting is suppressed and accepted
begin retains end recovery. Meter explicit labels remove competing aria-labelledby
and restore names exactly; live pair checks and formatter guards reject stale
display writes after callback disposal/removal/adoption.

Public integrated25new+168prior=193 groups PASS Chromium and hidden WebKit; seven
native CMake targets build PASS. Trusted keys skip disabled knob/number, produce
one enabled begin/value/end and no disabled edit, target/w188-keyboard.json.
Receipts target/w188-integrated-chromium.json and seven native integrated logs.
All12 component roles/names/states use native/DOM properties, not native AX or
screenreader evidence. Hidden hasFocus=false/window-focus limits remain explicit.
Strict docs/diff pass; no new dependencies. Shared W186–189 gates follow.


## W190: Owned WebKit native accessibility inspection — unavailable content evidence

A bounded informational probe inspected an owned synthetic WKWebView through
public modern and legacy NSAccessibility getters, then through own-process
AXUIElement reads in a separately owned visible validation window. Native
application/window/group/title metadata was readable. The remote WebKit proxy
returned kAXErrorCannotComplete (-25204) for its role; neither test button nor
input native names could be observed. All variants reported unavailable content
evidence (exit 3), not successful control-tree validation.

Strict C++17 ARC builds with -Wall/-Wextra/-Werror PASS; legacy deprecation
suppression is confined to the documented compatibility helper. Traversal was
bounded to 512 records, depth 16 and 1024-code-unit strings. Each owned window
and native view was closed/destroyed. No application activation, accessibility
actions, trust/settings change, private SPI, other-process traversal or DOM
substitute was used. Receipts remain local under target/w190-feasibility-*.log
and target/w190-feasibility-notes.md. This exact profile does not establish native
content access, screen-reader support or general platform inaccessibility;
existing DOM semantics remain separately validated.


## W189: Optional bounded opaque JPEG encoding — completed locally

The separate default-disabled Surface/umbrella jpeg-encode feature adds checked
quality 1..100 and explicit 4:4:4/4:2:2/4:2:0 baseline 8-bit encoding. Whole-source
alpha preflight requires opaque pixels before constructing codec/private output;
consumers composite first. A private 1..16 MiB gate publishes only complete
owned bytes, preserves source pixels and performs no caller I/O. Average chroma
filtering and standard Huffman tables are explicit; metadata/color management,
perceptual quality, working allocations and deadlines remain consumer limits.

Exact jpeg-encoder 0.7.1 selects std only, defaults/SIMD disabled, no runtime
dependencies/native C. Primary archive hash and upstream commit were verified;
complete MIT/Apache-2.0/IJG texts, original DCT attribution and required executable
acknowledgement are retained independently of RAVET commercial terms. Minimal
locked metadata added this one package, removed none, changed no existing
versions and changed only Surface's existing dependency list. The image-only
dependency graph excludes the encoder.

Promoted public tests PASS three groups, including the explicitly invoked
independent Pillow decoder across all 36 generated dimension/quality/sampling
profiles. Actual headers, quantization, sampling, source preservation, exact
output capacity and alpha/quality faults validate; strict Zune/Pillow maximum
component difference is 2 within a 4-byte fixture tolerance. Six existing image
groups, all-target/all-feature Surface warning-denied Clippy, five doc-tests,
warning-denied Rustdoc and Linux/iOS ARM64 compiler-only checks PASS. Evidence
target/w189-public-gates.log, exit 0; strict documentation/diff checks PASS.
These are local codec fixtures, not a general fidelity or runtime platform claim.
See [image assets](image-assets.md) and
[decision0201](decisions/0201-bounded-opaque-jpeg-encoding.md).


## W186: Worker-created automatic MP3 capture — completed locally

The shared private local-codec recording controller retains encoder construction,
operations and destruction on its worker, without a Send requirement on codec
sessions. Existing Vorbis public APIs and genuinely non-Send ownership remain
unchanged. The MP3 factory adds matching stopped/retired producer proof, acknowledged
pause/resume, bounded final tail and explicit Finalized/Aborted/Failed(original)
completion with recovered seekable destination/consumer. Outer Ok proves recovered
ownership, not finalization; accepted/dequeued source frames, file bytes and
physical writes remain distinct. First operational failure stops later dequeue
and returns its original error; abort/Drop never drain/finalize/rewrite/seek/flush.
Control joins and sink I/O may block without a deadline; panic can lose unwound
local ownership. Existing writer profiles/gapless/gain/licence limits apply.

Tracked focused checks PASS 49 audio-file groups and four actual PreparedCapture
retirement/overload/heap groups, warning-denied all-target/all-feature audio-file
and device Clippy, and four doc-tests: 57 passed, three external codec oracles
ignored here and previously executed independently. MP3 worker decoded PCM clears
the existing 256 kbps >45 dB raw SNR fixture threshold. Individual MP3-only and
Vorbis-only feature/doc/lint receipts pass; scoped formatting, strict docs and
diff checks pass. Packet target/w186-integration.md; no audio-file manifest or
lock changes, only default-disabled device mp3-recording forwarding. Native CPAL
capture/shutdown, drift, cancellation/deadlines and other targets remain outside
these synthetic capture checks. Shared W186–189 workspace gates now follow.

## W186–189: Shared workspace validation — completed locally

The combined worker-created MP3 capture, typed AU precision helpers, local
knob/meter semantics and optional opaque JPEG writer snapshot passes the complete
workspace gate, including Foundry without exclusions. The all-feature test run
reports 786 tests/doc-tests passed, zero failures and four ignored external
oracles; the codec/Pillow oracle profiles were executed independently as recorded
in their package evidence. Workspace all-target/all-feature warning-denied
Clippy, default doc-tests, default and all-feature warning-denied Rustdoc,
formatting, strict documentation checks, 43 script tests, four host-evidence
Python tests and diff whitespace checks pass. Receipt
target/w186-189-gates.log, exit 0. Builds used the serialized two-job programme
runner; source was held through this snapshot. These are local compiler/test
results and preserve the separately recorded limits on hardware, host,
accessibility, external SDK and runtime deadline validation.

## W191: Public AUv2 instrument audio precision — completed locally

The actual Performance component now adopts selected planar float32/64 native
audio with exact companion-table provenance, checked stopped initialization and
serialized offline transitions. Controls/text/complete Level state remain f32;
stopped cross-precision recall excludes voice history. Successful offline changes
reset voices and preserve accepted queued timed MIDI/Level intent. Failed setup
retires readiness/intent and requires uninitialize/initialize; arbitrary backend
failures were reviewed, not fault-injected. Stock intermediate32 initialization
and inherited Reset selector behavior remain explicit.

The normal public package passes 47 SDK tests and all 15 AU validators, including
actual f32/f64 analytic renders over 200 cycles, worker/main initialization,
offline/reset/cross-precision state and normal/fastmath native schema checks.
Existing Gain bundle and four bundler Python tests pass; strict docs/diff pass.
Receipts target/w191-bundle.log, target/w191-gain-regression.log and
target/w191-python.log; integration packet target/w191-integration.md. This is
local macOS arm64 component evidence, without a DAW, auval, complete-wrapper
fastmath, physical device, other-platform or deadline claim. See
[decision0203](decisions/0203-auv2-performance-audio-precision.md).

## W192/W195: Local split panes and fixed lists — completed locally

SplitPane adds measured two-pane grid constraints and a labelled separator with
axis/RTL keyboard input, owned pointer deltas, subpixel pixel minima and exact
style/attribute retirement. Non-native/non-editable empty-element separator and
bounded fixed member/ancestor profiles prevent child input or consumer layout
from displacing owned tracks. Too-small/transformed layouts suspend input; they
do not claim to satisfy infeasible minima or infer transformed coordinates.

ListBox adds separate virtual active focus and bounded single/multiple selection
over fixed options. Unavailable existing selection is retained; only explicit
replacement removes it and new unavailable selection rejects. Root-only Tab,
owned role/name/tabindex repair, range baseline retirement, focus/callback
reentry and exact attribute restoration are explicit.

Nine actual public Chromium fixtures and nine warning-clean CMake/native WebKit
fixtures pass 265 groups (37 split-pane, 32 listbox and 196 prior groups).
Existing owned AppKit knob/fader pointer regressions pass and their window closes.
Fresh trusted Chromium separator Tab/Arrow/Home/End and list Tab/ArrowDown/Space
receipts pass separately; preview hasFocus=false remains recorded. Receipt
target/w192-w195-final-packet.md and public keyboard JSON files. Pointer/focus
reentry contract fixtures remain synthetic and DOM semantics do not establish
native AX/screen-reader support. Required shared Rustdoc/doc-tests follow.
See [split panes](split-pane.md), [lists](listbox.md),
[decision0204](decisions/0204-local-split-pane-layout.md) and
[decision0208](decisions/0208-fixed-list-selection.md).

## W193: Explicit static GIF assets — completed locally

The separate default-disabled gif-image feature owns exactly one complete GIF
image with explicit transparent/global-palette canvas fill. Whole bounded
structural preflight and Indexed decoding validate palette/transparency entries,
interlacing, descriptor extents and complete strict LZW, including extra decoded
pixels after the required buffer. Animation/looping/plaintext/unknown extension
profiles reject; bounded GIF89a comments and non-loop application bytes are
ignored. No timing/disposal/color-management/file-I/O policy is implicit.

Five tracked groups pass exact independently generated Pillow pixels, literal
LZW partial canvases, every truncated prefix, invalid profile/end-code/palette
faults and inclusive 8192-record limits. Reviewed isolated all-target Clippy and
warning Rustdoc pass; public Linux/iOS ARM64 compiler-only checks pass and the
image-only dependency graph excludes gif/weezl. Receipt target/w193-public-gates.log,
exit 0, with target/w193-gif-draft/gates.log for isolated lint/doc evidence.
Exact gif0.14.2/weezl0.1.10 add only those lock packages, remove none and change
no existing versions; independent full MIT/Apache notices are retained. Shared
workspace gates follow. See [image assets](image-assets.md) and
[decision0206](decisions/0206-explicit-static-gif-assets.md).

## W194: Prepared file playback at an output rate — completed locally

Bounded off-audio preparation converts WAV/AIFF and optional FLAC/Vorbis/MP3 into
the existing MemoryPlayback transport with explicit same-rate Copy or causal
Sinc policy. Nominal duration, half-sample filter delay, retained finite tail and
actual zero padding remain distinct. Failures recover the original advanced
reader/error, committed read/submission/output progress and incomplete prefix;
no seek rollback or implicit trimming occurs. FilePlaybackOutput integrates
the complete clip with actual PreparedOutput using preallocated callback scratch
and exclusive transport ownership before device handoff.

Tracked focused checks pass ten file groups, three memory groups, five actual
output-adapter groups and five doc-tests (23 passed), including independent
analytic rate conversion, tiny/extreme-ratio partition identity, fractional
impulses, actual compressed gapless frames, corrupt-FLAC EOF recovery and repeated
output lifecycle with zero measured callback allocations. Audio/device
all-target/all-feature warning Clippy and default-off dependency graph pass;
isolated warning Rustdocs pass, shared workspace docs follow. Packet
target/w194-integration.md. Only an optional own-crate signal dependency is added.
Live read-ahead conversion, clock/drift mapping, cross-thread transport, physical
device and deadline evidence remain separate work. See
[decision0207](decisions/0207-prepared-output-rate-file-playback.md).

## W191–W195: Shared integration validation — passed locally

The source-held public snapshot passes 807 workspace tests and documentation
tests, with four explicit external-oracle profiles ignored by this run. Their
independent executions remain recorded under their work packages. Workspace
all-target/all-feature warning Clippy, default documentation tests, default and
all-feature warning-strict Rustdoc, formatting, generated documentation checks,
43 packaging/tool Python tests, four REAPER evidence-check Python tests and
diff checks pass. Foundry is included without exclusions.

Receipt: target/w191-195-gates.log, exit 0. This accepts the shared source
integration of the AU precision, split-pane/list selection, static GIF and
prepared file-playback changes. It adds no device, DAW, native accessibility,
other-platform runtime or real-time deadline evidence.

## W196: Native AAX schema and registered controls — completed locally

Reusable immutable native schema/control helpers stage real Avid SDK parameter
objects, recover partial manager insertion through public ownership APIs and keep
shared display metadata live through its last borrower. Exact public effect IDs
select registered mono/stereo Gain factories independently of descriptor order.
Uninitialize invalidates readiness before parameter retirement; stale callbacks
reject and fresh initialization readopts. Native f32 canonical control semantics
and exact IDs/text/read-only preflight apply; arbitrary SDK sideeffects have no
rollback and stock last-variant registration status may mask earlier refusal.

Normal public packaging passes14 native/underlying CTest contracts, actual bundled
registered mono/stereo Gain/bypass processing at1024frames and export/SDK-version
probes. Four packaging Python tests and strict documentation/diff checks pass.
Receipt target/w196-public-bundle.log, exit0, plus target/w196-integration.md.
Separate normal/fast helper fixtures include100000 allocation-counted cycles and
actual SDK vector/map insertion faults. Ordinary initial stock realtime mono
remains rejected/silent while stereo renders; positive mono/stereo fixtures use
explicit offline/realtime reprepare. Checked backend initialization, full AAX
state/other mappings and real ProTools/PACE acceptance remain separate work.
See [decision0209](decisions/0209-aax-native-schema-control.md).

## W198: Fixed hierarchical tree selection — completed locally

TreeView owns a fixed bounded preorder tree, root virtual focus, separate branch
expansion and single/multiple selection. Collapsed/unavailable selected descendants
remain model state; new unavailable additions reject. Native toggle default focus
precedes guarded root admission. Synchronous custom-element disposal after owned
writes stops later presentation changes and restores exact attributes/styles.

All309 integrated public contract groups pass in Chromium and actual hidden
WebKit across ten warning-clean CMake fixtures (44 new tree groups). Fresh trusted
Tab/Right/Down/Space, nested-label native button pointer and native Enter receipts
show exact-once expansion/collapse, retained selection and restored root focus;
subsequent Tab exits even with expanded descendants. Existing owned AppKit knob
and fader drag regressions pass and windows close. Receipt
 target/w198-final-packet.md and its public trusted/integrated files. Ordinary
fixture completion hasFocus=false remains explicit; pointer delivery true is
browser evidence only. Native OS focus/AX/screen-reader validation remains open.
Shared Rustdoc/workspace gates follow. See
[decision0211](decisions/0211-fixed-tree-expansion-selection.md).

## W197: Explicit trusted VST3 catalogues — native composition passed locally

The default-disabled ravet-host/vst3-catalogue ABI owns explicitly trusted native
modules and whole bounded immutable metadata without exposing factories/handles.
Rust provider/module owners are not Send/Sync; native context and exact factory
identity reservations prevent independent live owners overwriting singleton
contexts. Highest available class metadata is validated without clipped text or
hidden extended-getter failures. Explicit retirement distinguishes consumed
native-clear failure from rejected retirement that recovers the original owner.

A clean external-SDK public CMake build passes strict owned-source compilation
and both catalogue/provider CTest executables. Actual Gain and Performance
modules load/query/retire three times each, with duplicate live ownership,
wrong-thread retirement and unchanged rejected-output checks. Receipt
 target/w197-public-build.log and target/w197-public-ctest.log, exit0. The isolated
Rust draft passed four ownership/cache/retirement tests, two non-Send/non-Sync
doc-tests, warning Clippy/Rustdoc and actual main-thread reference-module loads.
Shared public Rust gates and the public Rust/native composition probe follow.
Context fault/module-termination ordering injection remains additional validation.
Component/controller/audio/state/editor/callback/graph hosting remains work;
no third-party compatibility, DAW, other-platform runtime or deadline claim.
See [decision0210](decisions/0210-trusted-vst3-catalogues.md).


## W196–198: Shared workspace integration accepted locally

The unchanged public AAX schema/control, optional trusted VST3 catalogue and
fixed TreeView integration passes the complete shared gate, receipt
 target/w196-198-gates.log, exit0: all-feature locked workspace tests813passed,
0failed,4ignored; all-target/all-feature warning Clippy; default workspace
doc-tests; default and all-feature no-dependency Rustdoc with warnings denied;
workspace format check; strict generated/local-link documentation;47Python
packaging/documentation tests;4REAPER-evidence Python checks; whitespace check.
Foundry remains included. The four ignored external-oracle tests remain separate
recorded receipts, not claimed as executed by this shared run. Cargo used two
workers and the existing local executable runner. This accepts the named local
integration, not whole format/hosting/UI completeness, real-time deadlines,
additional DAW/device/platform support or release/signing acceptance. Follow-on
native context fault fixes and table/audio/native-AAX packages remain separate.


## W197 follow-on: Public VST3 context faults and Rust/native composition

Controlled pointer-valid SDK factory/module fixtures reproduce two baseline
faults: a failed query returning an acquired interface leaves a reference, and an
exceptional setter preserves stale caller diagnostic123. The provider now owns
query outputs before checking status and maps caught C++ exceptions to SDK
InternalError. Eight cases verify absent/optional/failed/exceptional installation
and clearing, malformed late metadata, exact clear once, final reference count,
and host probes through factory release and module termination. The authored
Module::create substitute is confined to this fixture; actual SDK loading remains
separate. Baseline/query-only/fixed receipts target/w197-context-{baseline,
query-only,fixed}.log have expected exits1,1,0.

Public external-SDK CMake compilation and all three CTest executables pass after
the fixes, receipt target/w197-context-public-fixture-build.log and
 target/w197-context-public-ctest.log, exit0. Actual Gain/Performance loads and
wrong-thread/duplicate/unchanged-output checks remain positive. The public Rust
provider/owner composition passes against the rebuilt static native library,
receipt target/w197-public-rust-native-fixed.log, exit0; the probe build watches
that archive to force relinking. Shared813/0/4 gates cover the unchanged Rust API.
No invalid-pointer/native-code isolation, third-party/DAW/instance/audio/state/
editor/graph/other-platform/deadline claim follows.


## W201: Fixed readonly table and tree navigation boundaries accepted locally

The new TableView owns a bounded exact native readonly table profile with virtual
cell navigation/header association and separate single/multiple row selection.
Quiet eligibility fallback retains unavailable selected rows. Complete no-movement
keyboard preflight cannot select an unselected active row or create a range; only
genuine vertical Shift range boundaries preserve an established anchor. The same
boundary correction adds two TreeView regressions; prior309 receipts remain
historical. Bounds, cleanup/reentry and browser/layout limits are in the guides
and [decision0214](decisions/0214-fixed-readonly-table-grid.md).

Final public Chromium and actual hidden WebKit each pass349groups across eleven
warning-clean native fixtures (Table38, correctedTree46). Existing owned AppKit
knob/fader drag regressions pass and windows close. Fresh public trusted keyboard,
Shift boundary and nested-span pointer receipts show changed-only exact selection,
root-focus admission and Tab exit. Modal browser checks required foreground
interaction to resume native-close scheduling; no timeout contract was changed.
Receipts target/w201-final-packet.md and its integrated/trusted files. Ordinary
inactive-document focus behavior and synthetic focus/setter fault tests remain
explicit; no native OS focus/AX/screen-reader, sorting/editing/virtualization,
other-platform or deadline claim. The following grouped Rustdoc/workspace gate
will cover the sixteen-control overview and integration with other packages.


## W200: Checked native AAX Gain model accepted locally

The reusable authored SDK model/algorithm composes matching native processor,
schema and complete state companions. The default public Gain reference checks
actual SDK identity/rate/stems and native preparation before publishing its ready
owner; first ordinary realtime mono/stereo1024 renders without an offline repair.
Accepted queued global intent is included in complete native chunks, with checked
source ordering, whole known-fault preflight, rejected-prefix silence/tails,
stopped recovery and distinct exact lifecycle/latest AUDIO diagnostics. Public IDs
remain; local native RvS1 including bypass deliberately changes the prototype
stock chunk, without released-session migration or production compatibility.

Normal public native bundler passes18/18 CMake contracts and actual registered
factories/SDK Initialize/native algorithm/required exports/version0x20801345980,
receipt target/w200-public-native.log, exit0. Explicit stock bundler passes14/14
plus actual stock regression/exports/version, target/w200-public-stock.log, exit0;
its initial mono rejection remains explicit before positive public reprepare.
Six current Python packaging tests pass. Normal/fast-math helper/model fixtures
and10000 allocation-counted prepared C++ cycles pass; separately initialized
synthetic matching owner through the actual registered void algorithm proves
retained original backend error2, distinct from normal Gain-provider evidence.
Independent smoothing/history checks cover rejected late buffer maps. Registry
ID aax-checked-native-processing and [decision0213](decisions/0213-aax-checked-native-lifecycle.md)
record bounds and ownership. Grouped shared gates follow with other integrations.

CONTROL serialization, one AUDIO consumer, stop/join and valid foreign storage
remain required. SDK completed/exceptional notification effects have no rollback;
foreign allocation/pointer validity is not proved. Read-only reflection, programs,
aux/MIDI/transport/instrument/f64/editor/AudioSuite/latency/tail/output mappings,
Pro Tools/PACE production signing, other platforms and deadlines remain work.


## W199: Converted buffered playback implemented; public checks underway

ConvertedReadAhead performs explicit bounded worker conversion into an output-rate
queue with retained input/output suffixes, independent source/queue counts and
fresh-candidate stopped replay. Failures preserve original reader/phase/queue/
generation; previously verified unknown EOF rechecks actual input duration even
when downsampled output totals coincide. A pure worker progress hook preserves
input-only downsampling progress without changing native pump defaults. Actual
BufferedPlaybackOutput owns the unique callback transport and exposes explicit
worker Failed/Stopped/EOF status without reader/converter work on callback.

Final isolated evidence totals42tests/docs:11library,13converted,3actual prepared
output,2release full-ceiling phase,6existing automatic and7doctests. Both package
all-target/all-feature warning Clippy and warning Rustdoc pass. Runtime receipt
 target/w199-expanded-gates-before-docfix.log and final example/lint/docs receipt
 target/w199-final-gates.log, exit0. Corrections to power-of-two test ring capacity
and the Rustdoc RingError example changed fixtures only. Ceiling phase checks use
independent rational checkpoints and actual16,777,216-frame terminal impulse;
retained44.1k output15,414,081 from48k input agrees with the finite tail formula.

Owned public source/tests/guides and [decision0212](decisions/0212-converted-buffered-file-playback.md)
are integrated. Public focused and subsequent shared gates remain underway.
Source duration16,777,216frames/arrays32MiB per state, causal fractional delay,
explicit Copy/Sinc and O(target) stopped replay remain exact limits. No arbitrary
long-file, native endpoint reclamation/live seek, clock/drift/mailbox, physical
support or deadline claim. W203 output reclamation remains target-only.


## W199 public acceptance and W199–201 shared gate underway

The public converted-playback equivalents pass all42tests/docs, both package
all-target/all-feature warning Clippy and warning Rustdoc, receipt
 target/w199-public-gates.log, exit0. This completes the named public buffered
conversion scope within its documented limits. W199/W200/W201 and the VST3
context fixes are now held stable for the next full locked workspace gate,
 target/w199-201-gates.log. All subsequent W202–205 prototypes remain target-only;
no shared acceptance is claimed before the runner completes successfully.


## W199–201 complete shared validation and next integration

The serialized locked workspace runner completed successfully, receipt
`target/w199-201-gates.log`:833passed/0failed/4ignored in the all-feature workspace
test command, including its doctests and Foundry. All-target/all-feature warning
Clippy, default workspace doctests, both default/all-feature warning Rustdoc,
format check, strict documentation,49script tests,4REAPER evidence-check tests and
Git whitespace checks passed. These Python REAPER checks validate evidence handling;
they do not represent a new DAW session. Four ignored external-decoder fixtures
retain their separately recorded historical receipts.

Default doctest compilation exposed an unused private ring getter when
resampled-playback is disabled; the getter now has its matching feature gate.
Next shared validation will also run default all-target warning Clippy. The VST3
context fixture now includes a valid retained interface pointer followed by a
throw: ownership is acquired before the translated InternalError is checked,
preventing a reference leak. The unchanged-provider counterexample and target
fix passed before this narrow public correction; fresh public native checks follow.

The public source hold is released. W204virtual-list and W205AAX short-note
instrument sources, guides and decisions have been integrated. Target receipts
remain distinct from pending final public native/browser/package and next shared
workspace acceptance. W203reclaimable device output has passed its isolated
canonical-sibling30tests/docs, warning Clippy/Rustdoc and a silent default CoreAudio
two-cycle ownership probe:20channels/96000Hz, completed1536and1024frames, exactly
two prepares/deactivations and no recorded panic/backend error. This is a local
processor recovery/reopen observation, not audible quality, clock/drift, general
device support or a callback/teardown deadline. Its public integration is underway.


## W205 public AAX short-note instrument acceptance

Normal serialized public bundles pass Performance4/4, native Gain18/18 and
explicit stock Gain14/14 CMake contracts, receipts
`target/w205-public-performance.log`, `target/w205-public-gain-native.log` and
`target/w205-public-gain-stock.log`, all exit0. All three self-contained bundles
pass applicable notices, local ad-hoc signatures, required exports and SDK
version0x20801345980 checks plus mandatory actual registered algorithms.
Performance factory→real SDK Initialize→declared MIDI-node callbacks validate
analytic timing, Level state and independent voice-phase history after late
malformed input, supported fault silence/tails and retirement.8Python packaging
tests and strict documentation pass. These are local synthetic-host/bundle
receipts; Pro Tools, production PACE signing and deadlines remain unproved.

W206initialized transport sources/model/reference aliases, fixtures, guide and
ADR0219 are now integrated after full source review. Fresh target eight normal/fast
contracts, actual registered Gain/Performance forwarding and self-contained
Performance package passed before promotion. Final public Gain22/Performance4/
stock14 and next shared workspace gates remain pending.

The narrow W197 retained-pointer-then-throw correction passes all3fresh public
CMake tests including actual repeated Gain/Performance module loading, receipt
`target/w197-public-query-throw-ctest.log`; no new instance-hosting claim follows.


## W203–206 public focused acceptance

Public W203device30tests/docs, all-feature/all-target warning Clippy and warning
Rustdoc pass, receipt `target/w203-public-gates.log`. The reusable silent probe
also has an explicit default-disabled resampled-playback feature for converted
EOF/drain and recovered stopped seek; its native composition receipt follows
separately when its own runner completes.

W204public UI passes418groups in both actual Chromium and WKWebView:
Virtual60/Table41/Tree49/List35 plus233unchanged groups. All12native fixtures and
the separate own-window pointer fixture compile with Wall/Wextra/Werror and
execute successfully. Final public trusted browser Tab/End/Space, boundary token
cancellation and exact matching pointer lifecycle pass. Own-process AppKit events
observe native Option release and Command-capacity identity in trusted WK
PointerEvents; these are generated native events, not human hardware or global
OS-focus/AX/screenreader evidence. Hidden DOM focus observations remain separate.
Exact packet `target/w204-final-public-packet.md` links individual raw receipts.

W206normal public native Gain22/22, Performance4/4 and preserved stock14/14
contracts pass, receipts `target/w206-public-gain-native.log`,
`target/w206-public-performance.log` and `target/w206-public-gain-stock.log`.
All three packages pass notices, ad-hoc signature, required exports, SDK
version0x20801345980 and mandatory actual registered callbacks. Native Gain and
Performance prove initialized transport forwarding and accepted state/history
retention after malformed late loop; original statuses, exact raw positions and
rounded PPQ remain distinct.8Python packaging tests and strict documentation pass.
No production signing/Pro Tools/platform/deadline claim follows.

These focused receipts establish their named scopes. The next full shared gate
will cover W203–206, default feature-warning regression and retained-query-pointer
correction together, including Foundry and workspace Rustdoc.


## W203 silent converted native EOF, recovery and seek composition

The final public reusable probe passes both default-only warning Clippy and
actual generic/converted native runs, receipt
`target/w203-public-probe-gates.log`, exit0. Exact default CoreAudio profile is
20channels/f32/96000Hz. Generic silent cycles complete1024and1536frames with
prepare/deactivate counts2/2. Converted zeroWAV257frames at44100Hz with Sinc16
retains590output frames from an independent integer oracle. Actual output drains
590, recovers the exclusive processor, preserves cursor/queue after rejected
stopped seek, seeks17, explicitly reopens and drains573remaining frames;
cumulative consumption1163. Underflow observations3562and7145 remain reported;
source cursor does not advance through underflow. Both cycles finish at worker
EOF, with no recorded processor fault/backend error/panic and no reader worker
error. This is a silent local ownership/EOF/seek composition receipt, not audible
quality, clock/drift synchronization or a scheduling/teardown deadline guarantee.

All W203–206 public source is held for the full shared acceptance runner
`target/w203-206-gates.log`; future W207–209 implementation remains target-only.


## W203–206 combined public acceptance — completed locally

The final shared runner target/w203-206-gates.log exited0 after all11commands.
All-feature workspace tests passed846/0failed/4ignored. Both all-target all-feature
and default warning-Clippy configurations passed, including the default ring
feature-warning correction. Default workspace doctests, both default/all-feature
warning-denied Rustdoc builds, formatting, strict docs,51script unit tests,
4REAPER evidence-checker tests and git diff --check passed. These commands include
owner-maintained Foundry; they add no new DAW/device/deadline claim. The source
hold has ended. Historical focused/physical receipts above retain their scope.

## W202: Scoped trusted VST3 instances — source integrated

Separate ABI-v2 provider retains main-owned component/controller lifecycles and
lends a non-escaping audio capability from Rust parent/child ownership. V1table
remains unchanged. Non-discardable pin precedes even failed first creation;
consuming ProcessPinned facade close, exact retained context, canonical-path
reopen and foreign control reentry exclusion are explicit. Provider executable
code must also remain process-lifetime. Prepared main mono/stereo f32 slice
processing preflights whole caller metadata/ranges and protects tails, accepts
validated raw input/output alias via scratch and silences known DSP faults.
Events/parameters/state/editor/graph/f64/aux/timing reconciliation remain work.

Staged final-source external-SDK CMake9/9 passes, receipt
 target/w202-promotion-cmake-final.log. Staged complete Rust/native consumer links
those fresh archives and processes10000blocks per Gain/Performance mono/stereo
profile with measured0Rust allocator/deallocator calls; native regular C++ heap
measurements are independent. Joined panic/forget/main teardown,8negative compiler
contracts, authored pointer-valid provider cleanup/retry/poison faults,
5Rust unit groups/10doctests/warningClippy/warningRustdoc pass. Source now lives
under crates/ravet-host plus explicit tools/vst3-validation, guide and ADR0215;
fresh public Cargo/native composition and next combined shared gates are running.
No arbitrary third-party/DAW/device/deadline/platform claim.

## W207–208 source integration

Reviewed explicit grid layout and measured standalone getter/workspace consumer
are integrated with ADR0220, guides and stable capability records. Target/source
acceptance covers5grid groups,10000zero-allocation resize transitions, actual owned
hidden windows12measured transitions (2TooSmall exact-retention states), default
popup/editor-before-parent teardown and actual Chromium Rust-derived geometry,
transport and trusted text/checkbox focus/selection retained across resize.
Programmatic requests demonstrably bypass Cocoa user minima; measurements and
request sizes are distinguished. No AX/screenreader or human-drag claim.

Reviewed IntegerSinc scheduling and long converted-stream profile are integrated
with ADR0221 and explicit caller selection. Existing float/live paths and complete
file memory caps remain. New exhaustive enum variant is documented; exact bounded
integer phase and checkedu128retained counts allow usize-limited sources with
u64::MAX-65536terminal callback headroom. Actual long16,777,249-frame WaveReader
reaches exact1,048,579output/EOF; failed/successful replay preserves original-error
ownership and correct queue/generation/impulse/tail. Stopped seek remains O(target).
Target acceptance38signal/23audio/8adapter groups,9doctests and all three warning
lint/doc builds pass; new common-rate numerical comparisons are explicit in the
guide. Focused public final-source checks and next shared gate follow separately.
W209multibus and W210content constraints remain isolated pending final review.


## W202, W207 and W208 final public focused acceptance

W202 public locked ravet-host with vst3-catalogue passes110tests/doctests,
zero failed/ignored, receipt `target/w202-public-rust-tests.log` (terminal exit0).
Fresh public external-SDK native build passes9/9CTest and the complete public
Rust/native instance consumer, receipt `target/w202-public-native.log`, exit0.
The actual consumer processes10000Gain/Performance mono/stereo blocks per profile
with zero counted Rust allocations/deallocations and checks joined panic/forgotten
audio capability cleanup. These are trusted local reference/SDK ownership and
audio receipts; parameter/state/events/editor/graph adoption remains separate.
Focused public warning lint/doc and combined workspace acceptance follow below.

W207 final public grid passes5Cargo groups including10000allocation-free resize
transitions. Surface/webview library and complete layout_workspace consumer
warning-Clippy pass. Actual public Chromium geometry5groups plus trusted control
value/caret/focus retention across4resizes pass. The public locked Cargo consumer
fixture exits0 after12measured owned native window/grid transitions,2TooSmall
exact-retention states and24rapid requests per window settling the latest geometry.
Default popup, invalid-request retention and editor-before-parent cleanup pass.
Exact packet `target/w207-public-packet.md` retains raw receipts and source hashes.
No human drag, global focus, AX/screenreader, device or platform certification.

W208 final public focused checks pass80tests/doctests:38signal,25audio-file,
8device-adapter and9doctests; all three package all-feature/all-target
warning-Clippy and warning-Rustdoc commands exit0. Receipt
`target/w208-public-gates.log` uses one exclusively owned Cargo build directory.
The actual long WaveReader/oracle, finite tail/EOF, compatibility and error-owned
seek evidence above retain their named scope. Indexed integer PCM seeking is a
separate implementation package; existing stopped conversion seek still replays
O(target) frames. No additional physical device/DAW/deadline evidence follows.


## W209 and W210 source adoption

Reviewed W209 prepared native AAX main/aux mapper, existing default-context
metadata guards and actual registered Rust bus-routing consumer are integrated.
Workspace adds only ravet-bus-routing-native0.0.1 with existing foundation/native
dependencies; offline lock resolution changes no existing package/version.
Target14/14normal/fast native checks and5Rust groups/lint/docs pass; public
Python8pass. NativeGain26/actual packaged module, Performance4/stock14 and focused
public Rust/shared acceptance follow separately. Public references remain main-only;
no Pro Tools/PACE/generated-output/f64 AAX/deadline claim. ADR0222 records scope.

Reviewed W210 portable ContentConstraints, owned opt-in native installation and
shared resize/policy reentry guard are integrated. The public reproducibility
script builds exact Rust/Cocoa sources and exits0 after22measured own-window/grid
transitions and4synchronous native callback rejection observations, receipt
`target/w210-public-native-build-run.log`. Public webview11unit groups pass,
including5new pure-policy groups; focused allocation/consumer lint follow. One
locked allocation attempt occurred between member insertion and lock resolution
and is preserved as exit101; it is not a validation success. Default popup/borrowed
editor ownership and absent default policy remain; interactive workspace opts in.
ADR0223 distinguishes measured per-dimension native rounding from human drag/AX.

New W202/W207–210 public sources are stable for the next combined workspace gate.
W211 indexed integer PCM seeks and follow-on window/AAX output work stay isolated.


## W209 final public focused acceptance

All format-owner public processes are terminal exit0. Native Gain26/26,
Performance4/4 and stock Gain14/14CTest pass, plus each actual packaged module's
required exports, SDK0x20801345980 and registered algorithm checks. Receipts are
`target/w209-public-gain-native.log`, `target/w209-public-performance.log` and
`target/w209-public-stock.log`; final packet `target/w209-integration.md`.
The default authored Gain/Performance module callbacks protect common context/
frame/owner metadata and preserve pending complete restore, Gain smoothing and
Performance voice phase after late rejection. Actual registered eight-profile
multibus fixture uses public Rust/native processing and exact independent audio,
state/history/tail/source-order and alias oracles. Public references stay main-only.

New public Rust bus-routing reference passes5tests/doctests, warning-Clippy and
warning-Rustdoc;8Python packaging tests, strictdocs and diff checks pass. Native
Gain requires the routing archive for mandatory validation; stock/Performance
retain prior requirements. No SDK source/vendor licence or proprietary consumer
was copied. Local ad-hoc signatures remain distinct from PACE/Pro Tools acceptance.
Combined shared W202/W207–210 runner is live at
`target/w202-207-210-gates.log`; final workspace acceptance is still pending.

## W210 focused acceptance and next isolated work

The final public W210 packet `target/w210-public-packet.md` confirms11webview
unit groups,1tutorial allocation group/19998zero-allocation calls and exact public
native22transition/4synchronous reentry fixture exit0. Redundant queued lint was
retired only after its exact own wrapper was proven child-free/WAITING; shared
warning-Clippy/default+allfeature warning-Rustdoc now supplies remaining coverage,
which is pending until that runner exits0. No active compiler was terminated.

W211 indexed IntegerSinc WAV/AIFF conversion seeks, W212 owned window closure
lifecycle and W213 generated short-MIDI AAX output remain isolated implementation/
design packages. Root W214 component/controller state IO has a reviewed design
and actual public-SDK bounded IBStream primitive test (strict C++ build/run0):
finite cursor/ranges, owned/frozen foreign AddRef retention, worker ref release
without final payload deallocation, CONTROL keeper reclamation and16slot/64MiB
exhaustion. It is not yet integrated instance-state hosting; compound restore
failure poisoning, native-core pin and complete Rust/native consumer remain work.
No new public state-hosting, DAW/device/deadline claim follows from that primitive.


## W211–216 isolated implementation and review evidence

Public runtime/Cargo snapshot remains held for shared W202/W207–210 runner39661.
That exact live process has passed all-feature workspace tests, all-feature and
default all-target warnings-denied Clippy, and is running default workspace
Rustdoc tests; combined final acceptance is still pending. No active compiler was
terminated and no shared runner was restarted. Target-only receipts below are
not public feature adoption or broad platform/DAW/device support.

W211 indexed IntegerSinc WAV/AIFF stopped seeking is implemented/reviewed for
promotion:92unique tests/docs, original error/advanced candidate recovery,
bounded original-history reads and exact complete-conversion suffixes including
16,777,249-frame generated WaveReader fixture. Packet target/w211-integration.md.
W212 owned standalone closure/state is implemented/reviewed:15own native lifecycle
groups and constraint22transition/4callback regression, default popup and borrowed
editor cleanup; packet target/w212-review-packet.md. W213 generated AAX short-MIDI
output and actual initialized NoteRouting Rust consumer are implemented/reviewed:
10Rust tests plus docs and8normal/fast native groups. Final late-throw status retains
last returned SDK result, and lock-free processing admission removes a CONTROL/
AUDIO plain-bool race; meaningful concurrent registered callback/control rejection
and post-join acceptance pass. Packet target/w213-review.md. No packaged generated
consumer/Pro Tools/PACE/f64/aux-output/physical MIDI claim follows.

W214 now implements the complete stopped component/controller state transaction,
not only the previously recorded stream primitive. Fresh exact native recipe
passes13/13CTest, including all original nine instance/catalogue regressions;
exact relative-path safe Rust owner links fresh archives and actual local Gain/
Performance bundles, verifies complete state/connected controller order, explicit
reprepare,10000mono/stereo blocks/profile with0Rust allocations/deallocations,
real late controller refusal, poison/safe retirement and new-child recovery.
Eight mocked Rust groups, twelve compile-fail examples, warning Clippy/Rustdoc pass.
Independent review corrected QI known-alias writes and contradictory restore
reports; fixed SDK stream tests include1000worker/freeze races and retained module/
context/host pin through failed/throwing capture. Packet target/w214-review-packet.md.
Opaque UID/bytes do not authenticate schema/version; no live restore, automatic
restart/parameter/editor/event/graph, arbitrary third-party or DAW/deadline claim.

W215 output-clock file/capture composition is implemented/reviewed:44tests plus
2doctests, default/all-feature all-target warning Clippy and strict Rustdoc pass.
Canonical simulated independent capture±500ppm, mono/stereo file-first grouping,
actual PCM worker failure, retained post-consumption DSP fault, explicit stopped
seek/reprepare and zero-output/32MiB scratch rejection pass. File queue receives no
drift estimator; no physical timestamp/start alignment/long-session claim.
Packet target/w215-integration.md. W216 explicit sRGB/linear-light preparation
is implemented/reviewed:11Rust groups including normalized subnormal alpha,
8192allocation-free operations, independent Decimal256scalar/4096transfer/2048
composite cases, Rustdoc, own WK11groups and scoped trusted Chromium Rust-data/
resize evidence; packet target/w216-review-packet.md. Existing renderer defaults
retain their numeric policy; no ICC/DisplayP3/font/SVG/AX/physical display claim.

All six packages still require coherent public promotion and public grouped gates.
W217 dynamic AAX initial/request/ACK/adoption and W218 paired native clocked duplex
retirement/recovery have approved target-only implementation designs. W219 bounded
SVG import/render has approved target-only dependency/resource/profile design;
no public dependency/manifest/lock or new support claim is recorded yet.


## W211–216 public adoption and focused acceptance

The previous W202/W207–210 shared runner39661 authoritatively completed exit0:
all11commands passed; the all-feature workspace test result totals880passed,
0failed and4ignored. Both all-target warning-Clippy profiles, workspace doctests,
both warning-Rustdoc profiles, formatting, documentation and script checks passed.
Terminal receipt: target/w202-207-210-terminal.json; full log:
target/w202-207-210-gates.log. This acceptance predates W211–216 adoption.

The six reviewed packages W211–216 are now publicly integrated:81 authored
source/test/guide/decision/metadata files were adopted through exact hash guards,
then the generated catalogue was refreshed. No Cargo dependency, member or lock
change occurred. Catalogue149implemented/112prototype/3planned/2excluded is a
scope/status inventory, not a programme completion percentage.

Public W211/W215 focused receipt target/w211-w215-public-integration.md proves
49unique tests/doctests, all-feature/all-target warning-Clippy across the three
changed packages and strict Rustdoc. Public W212 lifecycle fixture passes15/15;
public W216 independent Decimal256/4096/2048cases and native11/11consumer pass,
with public512model export identical to the retained browser validation input.
Receipts: target/w212-public-build-run.log, target/w216-public-oracle.log,
target/w216-public-native-fixture.log. Public W213 Gain30/30native contracts and
actual initialized packaged-module callbacks pass; remaining Performance/stock
and Rust receipts are still being collected. W214 integrated native/state-owner
and the new combined public workspace gate are next; source is held unchanged.
W217 latency adoption, W218 paired clocked recovery and W219 SVG remain isolated.
No new DAW, physical device, screen-reader, signed release or deadline claim.


### W213/W214 final public focused receipts

W213 all public owned validation processes are terminal exit0: Gain30/30,
Performance4/4 and stock14/14native contracts plus each actual packaged module;
Rust10tests/doctests, warning-Clippy/Rustdoc, Python8 and strictdocs pass.
Final receipt target/w213-public-final.md. Guide/ADR now record public evidence.
Public W214 fresh strict native build is terminal0 and CTest13/13passes. Public
state-owner and original audio-owner scripts both run exact current Rust source
against fresh public native archives and explicit Gain/Performance bundles, exit0.
Complete state/order/late refusal/fail-closed/joined retirement/new-child recovery
and10000mono/stereo/profile0Rustalloc/dealloc pass; original audio-owner panic
messages are intentional caught regression inputs, not an unreported failed run.
Receipt target/w214-public-acceptance.log. Shared new W211–216 session12304 remains
live; no final grouped acceptance claimed until that exact process exits0.


### W211–216 combined gate correction and retry

The first combined runner12304 authoritatively exited101 in Gain's concurrent
worker allocation fixture: the worker's outstanding allocation assertion reported
2 instead of0, before the callback allocation assertion ran. That failed log is
retained at target/w211-216-gates.log; it is not integrated acceptance. The fixture
now uses an atomic started handshake instead of entering blocking startup
synchronisation inside its measured allocation loop. All original positive-worker,
net-worker-zero and callback-zero assertions remain. Focused all-feature locked
Gain realtime tests pass4/4, terminal session83890 exit0, log
target/w211-216-realtime-fix.log. Retry99866 uses the corrected public snapshot and
new log target/w211-216-gates-retry.log; joint acceptance remains pending.

Reviewed target-only W217 dynamic AAX timing passes corrected admission/lifecycle
strict6/6 and full32/32native contracts plus its actual module. W218 paired native
clocked retirement additionally ran two350ms silent cycles on explicit UMC1820
CoreAudio input18/output20 at96k: both recovered the original processor after
joined four-guard retirement, with zero backend/processor/capture faults and zero
drops. This is short silent ownership evidence, not acoustic/clock/hotplug or
long-session support. W219 bounded SVG's final actual own-WK fixture passes24/24,
with18 independent render cases and all six integer profiles byte-exact. W222
stack-backed streaming interpolation source/consumer review passes, with116unique
isolated tests/docs and warning-lint/Rustdoc receipts. These packages remain
isolated; no public capability or dependency adoption is claimed yet.


### W220 parameter companion qualification and parallel package checkpoint

W211–216 retry99866 remains live. Its corrected all-feature workspace tests and
all-feature/all-target warning-Clippy have authoritatively passed; default-feature
Clippy is running. This is progress within the joint gate, not terminal acceptance.
Public compiled sources remain held for that exact snapshot.

Root W220/ADR0233 target-only implementation is now frozen with26 exact source/
fixture/guide/public-baseline guards and an additive feature draft. Final native
build/session42833 exits0, CTest23/23 including C11 wire contracts and four fast-
math variants. Isolated root Rust session89969 exits0 for13units/14compile-fail
Rustdoc tests, all-target warning-Clippy and strict Rustdoc. Exact public-relative
safe Rust consumer against the fresh native archive and actual Gain/Performance
bundles exits0: four mono/stereo profiles, controller/DSP separation, independently
predicted timed Gain smoothing/bypass, Performance serialized Level, stopped state
recall/explicit refresh/reprepare and10000variable/zero blocks/profile with zero
observed Rust allocations/deallocations. Performance remains silent without notes.
Packet target/w220/review-packet.md; public promotion/full gates remain pending.

Independent reviews corrected shared-core CONTROL geometry and malformed successful
edit admission. Two prepared children now process with shared keeper geometry fixed,
while every mutable CONTROL operation waits until all borrowed audio scopes joined.
The actual fixture checks wrong-thread end preserving counts and20001aggregate
blocks. Native preparation distinguishes an original SDK refusal, owned synthetic
bus metadata diagnostic and thrown call without fabricating returned SDK status.
Retained individual parameter queues pin/reopen the exact core; payload reclaim is
CONTROL-only and never automatically unpins. No DAW or deadline claim follows.

W217/W218/W219/W222/W224/W225 reviewed target-only packets remain held. W224 fractional
delay isolated124unique tests/docs and W225 selected-order Butterworth132unique
include their shared earlier Signal baseline; they are not new-test totals.
W223 native accessibility has a frozen21-file packet with20native/17Rust/24actual
Rust-native-WK groups, scoped direct-protocol evidence only; root review is pending.
W221 AAX editor final packet is undergoing one final preinitialize-size admission
correction. No new Pro Tools, PACE, screen-reader, physical hotplug/long-session,
Windows/Linux/mobile or release support is established. Independent named DSP/font/
format work continues; no whole programme category is complete.

### W211–216 joint acceptance and next reviewed packet

Retry99866 is now authoritatively terminal exit0. All11 integrated commands pass:
927tests/0failed/4ignored, both all-target warning-Clippy configurations, workspace
doctests, both warning-Rustdoc configurations, formatting, documentation, Python/
REAPER checker tests and whitespace checks. Exact receipt
target/w211-216-terminal-retry.json records the complete log hash and command list;
the earlier allocation-fixture failure remains preserved separately. The public
compiled-source hold for this snapshot is released.

W217–226 are root-reviewed isolated packages; root has prepared124 guarded combined
source/test/guide/metadata outputs without public source adoption. The AAX timing
and optional editor pipeline additions and SVG/native semantics tutorial additions
are merged together. The native semantics command uses the actual ravet-examples
package. The paired device guide now distinguishes two short silent same-interface
CoreAudio ownership runs from acoustic/hotplug/drift/long-session validation.

W221 size and deferred callback retirement/reinitialization corrections are fully
reviewed and qualified. W223 explicit native semantics and W226 FIR coefficient
design are reviewed; direct AppKit protocol evidence remains separate from system
AX/VoiceOver. W229 Chebyshev I/II source and independent numeric/time/Graph evidence
are reviewed, including corrected candidate-response errors retrying higher orders;
final focused qualification/hash refresh precedes adoption.

Review discovered an additional W217 recovery contract defect: sticky timing faults
could survive old-slot collection while stopped preparation reported readiness.
The timing owner correction and refusal-without-foreign-side-effects regression are
required before that package is adopted. W227 internal AAX f64 and W228 contextual
font fallback remain isolated work in progress; W230 elliptic-family design is
approved for independent implementation. No whole programme area is complete, and
no new DAW, signing, screen-reader, platform or deadline support follows.

### W217–226 and W229 public source adoption

After retry99866 terminal acceptance and the W217 sticky-fault correction's final
strict6/full32/actual-module receipts,127 exact guarded outputs from eleven reviewed
packages were publicly adopted. Source maps and the combined record are retained
under target/w217-226-promotion; public-adoption.json explicitly leaves public
qualification incomplete. Both timing/editor pipeline changes and SVG/native
semantics tutorials are retained. W229 spec-loop response errors skip failed
candidates, preserving the documented bounded higher-order retry contract.

Offline full metadata resolution adds14 package identities for optional SVG and
removes none; every existing package version is preserved. Existing strict-num/
tiny-skia dependency lists change through optional dependency/feature resolution.
Actual all-workspace-feature resvg/usvg remain without text/font/raster-image
features, roxmltree positions/std and tiny-skia std/SIMD/PNG are explicit. Shared
dependencies additionally unify bytemuck derive/aarch64 SIMD, numeric traits/euclid
and zlib-rs. Eight effective package rows and twelve previously absent complete
licence files were added to SVG notices; all actual normal dependency versions are
covered. Receipt target/w217-226-promotion/svg-dependency-acceptance.json. This
remains dependency evidence rather than release-wide legal review.

Catalogue regeneration/check and whitespace checks pass. Inventory278 records:
157implemented/116prototype/3planned/2excluded, not a completion percentage.
Post-adoption Rustfmt corrected one multiline include_bytes spelling without
changing behavior. Fresh canonical Bus/Delay release build exits0 at
target/w217-226-promotion/public-delay-release.log. Public processing Rust tests
and a fresh public VST3 native build/actual safe parameter consumer are running;
format owner collects separate public headless/editor/Performance/stock native
receipts. Remaining focused UI/default/all-feature lint and complete workspace
documentation/joint checks are required before combined public acceptance.

### W217–230 public focused evidence and reviewed additions

Public processing session52561 is terminal exit0:401tests/docs,0failures in
Signal, Device, Host and Bus/Delay with all features. Fresh canonical Gain
native-editor release session94599 is terminal0. Receipt
target/w217-226-promotion/public-cargo-terminal.json retains exact log hashes.
Fresh public VST3 native session58195 passes23native tests and the actual safe
Rust parameter owner against Gain/Performance bundles: four mono/stereo profiles,
timed Gain/bypass, recalled state and10000blocks/profile with zero observed Rust
allocation/deallocation. Native semantics session49717 passes20native/17Rust/24own
Rust-native-WebKit checks; these are direct protocol evidence, not system AX or
VoiceOver. Public AAX headless Gain32/Performance4/stock14 and all three actual
module checks pass session37378; the final optional editor is being relinked to
the newly built canonical public Rust archive, with intermediate isolated editor
evidence retained separately.

W228 contextual local font fallback and W230 selected-order elliptic filters
are reviewed and34guarded public source/test/guide/metadata outputs integrated.
Root merged additive fallback declarations/navigation against the current SVG
and native semantics capabilities instead of adopting stale replacement files.
Optional SVG licensing/feature-unification text is preserved. No new dependencies
or lock changes are needed by these two packages. Documentation regeneration and
workspace formatting pass; their public qualification and the combined workspace
gates follow. Catalogue280records,159implemented/116prototype/3planned/2excluded
is an inventory, not a completion percentage.

W228 retains exact face/source ownership and atomic SVG/CPU preparation, backed
by isolated independent HarfBuzz10profiles/19runs, shared-model60selections and
actual owned-window104checks/observations. Its WebKit NFC font-policy discrepancy
remains explicit. W230 bounded elliptic math/tests/guide/ADR received a complete
independent source review against NIST and independent rational/time/Graph
evidence; frozen scoped155tests/8docs are not all new tests. Neither scoped
package establishes general system fonts, screen readers, physical platforms,
continuous numerical certification or real-time deadlines.

Fresh public optional AAX editor acceptance is now terminal0: canonical editor
35/35 plus actual registered native/NSView GUI modules against the fresh public
Gain native-editor Rust archive, session92842 first phase. Exact logs
target/w217-221-public/canonical-editor-{configure,build,tests,module,gui-module}.log
replace the earlier isolated-archive evidence for this public-source claim.
No Pro Tools/signing/host accessibility/deadline claim follows. W227 selected64
editor51/51 with actual native/GUI consumption also passes in its separate
target-only candidate; final packet is still pending.

Current coherent snapshot hold: public Surface all-feature session45286 and
independent HarfBuzz/actual native fallback session39620 are live, with the full
eleven-command workspace acceptance queued session72654 in
target/w217-230-gates.log. HarfBuzz10profiles/19matched-face runs already pass
on the actual public engine; native rebuild/fixture and Surface doctests are
still pending. Compilation is capped at two shared Cargo jobs. No queued/live
job is restarted because its log is quiet. Further public source adoption waits
terminal acceptance; independent target-only work continues.

Public Surface session45286 is now terminal0:99tests/docs,0failed, including
actual SVG/fallback and borrowed-font compile-fail examples. Public fallback
script session39620 is terminal0: independent installed HarfBuzz14.4.0,10profiles/
19matched-face runs, then the actual canonical normal Rust/WebKit consumer with
104checks/observations. All60Rust-selected renders and30SVG/CPU pixel pairs pass,
with six analytical CSS agreements plus the separately retained NFC policy
discrepancy, three measured own-window resizes, default settings and cleanup.
Exact hashes are in target/w228-230-promotion/public-focused-terminal.json. These
receipts support the stated bounded package, not general CSS/system font/screen-
reader or physical-device equivalence.

Joint session72654 has acquired the shared Cargo slot and is compiling/running
all-feature workspace acceptance. All eleven commands, including required
workspace doctests and strict Rustdoc, remain pending terminal receipt. Public
compiled-source hold remains active. W231 system-font catalogue/style-to-render
architecture is approved for isolated implementation; W232 actual weighted
equiripple/halfband FIR design is approved and underway. Root W233 hosted VST3
event/output/context companion architecture received independent SDK review;
context chord/negative clock/SMPTE units and explicit opaque SysEx/pool policies
are retained in target-only design, with no implementation claim yet.

### W217–230 combined validation interrupted by storage exhaustion

Joint session72654 terminated exit101 during the first all-feature workspace
compilation because the filesystem ran out of space. No completed command or
test assertion failure is inferred. The original log and its hash are preserved
in target/w217-230-gates-enospc-first.log and its JSON receipt. The earlier scoped
public test and native acceptance receipts remain separate from this failed
combined run. Disposable terminal compiler caches are being reclaimed before
an exact-source retry; further public compiled-source adoption remains held.

W227 internal native64 AAX has a frozen candidate and local package/default
regression receipts, with root integration review pending. W232 weighted
equiripple/halfband FIR has a frozen target-only packet, twelve focused groups,
nine doctests and nine independent Decimal primal/dual profiles; it awaits
root review and public integration. W231 system fonts and root W233 hosted
VST3 event/output/context remain under isolated implementation. W234 full short
MIDI instrument input design is approved for isolated implementation. None of
these observations closes a broad programme category or supplies a weighted
completion percentage.

### Reviewed candidates and joint retry progress

The unchanged-source joint retry session24755 has completed all-feature workspace
tests:1051passed/0failed/4ignored across276reported unit/integration/doctest groups.
Its warnings-denied lint and remaining documentation/tool commands are still
running; this partial result is not terminal combined acceptance. Incremental
compilation is disabled for this storage-constrained retry, with shared Cargo
limited to two jobs. Further public compiled-source adoption remains held.

Root has accepted the corrected W232 FIR source and actual stereo Graph/direct/
partitioned consumers after verifying all13frozen source/evidence hashes. The
original normalized-Nyquist defect, original reflected-grid assumptions and first
packet remain preserved; corrected179tests/docs and9independent Decimal profiles
pass. W235 configured elliptic allpass halfband/oversampling source and all eleven
focused groups are reviewed, with isolated191tests/docs and strict lint/docs
accepted. Nominal attenuation, actual rounded edge sizing and4097-point sampled
peaks remain distinct. W227 precision and W236 canonical table acquisition maps
pass33/12immutable guards. Root paired generic compilation verifies the new
runtime native acquisition succeeds and const acquisition fails onlyE0015.

Root W233 target-only primitives now pass22C11/normal/fast build/run commands:
all10SDK event kinds, real immutable input/mutable output IEventList, actual
IParameterChanges/individual writable queues (64queues/2048whole-block points),
16retained complete process batches, combined whole event/payload/parameter/count
publication and10000variable/zero-frame ordinary-C++-heap-free phases. Complete
pool size10066712bytes gives six pools60400552bytes including keeper slots within
the64MiB cap. Exact-core CONTROL keeper, refused pin, worker release, actual
reservation-registry reopen and controlled core resource lifetime pass; these are
not actual SDK image-loader retention evidence yet. Full copied ProcessContext
construction passes8192independent presence combinations, signed sample/time/
clock/SMPTE domains and2048request-bit combinations. Exact source/log hashes are
in target/w233-processing-context-gates/terminal.json. The initial context fixture
signed-comparison compilation warning is preserved separately. Native provider,
actual processor/instrument and safe Rust processing-companion integration remain
required before a public hosted-processing capability claim.

Review also identified the existing C++ client's callback-equivalence admission
policy as inconsistent with exact originating companions. W237 separately fixes
it with genuine32/64 exports, byte-identical copied-core refusal before any creator
and balanced acquisition/query-exception fixtures. Its frozen8files and explicit
W236 documentation/CMake overlaps are reviewed; public adoption and fresh native/
format qualification remain pending. W231 system fonts and W234 raw short-MIDI
actual module qualification continue in isolated sources. The coherent reviewed
integration tree is held under target until all current joint commands finish.

### Processing review correction and additional reviewed DSP source

The independent W233 partial-source review reproduced a reused UTF-16 alignment
padding byte from an earlier event payload. The target-only codec now initializes
that owned padding byte before copying/publishing the used prefix. Seven actual
SDK event-list groups cover reused input/output storage, empty text, exact65536-byte
alignment/terminator capacity and whole late-capacity refusal. All22 C11 and normal/
fast build-run commands pass; the original counterexample, pre-fix receipt and
initial boundary-fixture stale-pointer refusal remain preserved separately.

Review also corrected target-only instance query/create and cleanup diagnostics:
thrown calls no longer claim an original returned SDK status. Valid retained
exceptional outputs retire once, all six cleanup throwing phases balance their
acquired duties, and later faults preserve the first returned-or-thrown provenance.
All8 normal/fast protocol and block-fault build-run commands pass. Their initial
direct-driver external SDK warning is retained; the corrected driver uses the same
SYSTEM include policy as the existing CMake build, keeping first-party warnings
denied. Full W233 native provider/actual processor and safe Rust integration is
still required; these partial fixtures do not establish hosted processing support.

W238 native64/32 peak/RMS ballistics is reviewed after all395production/796test
lines and16frozen hashes. Its nine meaningful groups include actual stereo Graph
consumers; full isolated201tests/docs and warnings-denied lint/Rustdoc/format pass.
Actual native timing, independently bounded rounding and retained initial ideal-
tail diagnostics remain explicit. The coherent reviewed six-package integration
tree now contains72guarded outputs under target/w227-232-235-238-promotion; public
compiled-source adoption remains pending the live joint gate. W231 system-font
review and W234 raw-MIDI module qualification continue independently.

The next W233 target-only copied processing catalogue passes four normal/fast
build-run commands and three meaningful actual SDK groups. It copies indexed
Unicode event-bus names/channels/type/flags, explicit selected directional bus
profiles, optional context-requirements presence and all130 input controller
assignments to admitted writable parameter IDs. Actual maxima are32directional
buses/33280mapping calls; no bus activates during discovery. Late native/metadata/
throw failures preserve the previously committed whole snapshot and original
returned-status provenance. Invalid IDs and actual wrong-thread calls refuse
before foreign callbacks. This catalogue is not yet wired into Instance/provider
preparation or the safe Rust companion; its standalone receipt does not complete
W233. The initial test indentation warning remains preserved separately.

W231's corrected portable bookkeeping and immutable native source are accepted
after the source/contract/consumer review. All36mapped hashes and supplemental
portable/native receipts pass. The reviewed seven-package integration tree now
contains108guarded outputs under target/w227-238-reviewed-promotion; its complete
staged canonical documentation check and whole-input/baseline promotion preflight
pass without public source writes. Earlier69/72output staged trees remain
historical. The existing joint gate is still live; post-adoption archive/format
and shared workspace qualification remain required.


### Processing owner integration and filter review

Root W233 target-only production provider now owns complete processing keepers
under the exact NativeCore and checks their whole ranges with other state/parameter
storage. Successful CONTROL admission pins observed retained processing interfaces
before subsequent foreign callbacks, including stop/reprepare/sibling creation
and consuming close. Actual provider/module-factory substitution proves input/
output event-list, output-parameter-list and individual-queue retention, worker
final release, released-before-cleanup sticky retention, ordinary context/module
exit, same exact canonical-core reopen and CONTROL reclamation. This is production
provider ownership evidence with an authored loader, not actual Gain/Performance
module processing or Pro Tools/DAW evidence.

Instance now copies selected event-bus/controller mapping/context-requirements
metadata during explicit stopped refresh, validates topology before allocation/
setup and again after native setup, prepares exact known output IDs, and activates
only selected event buses. Each attempted activation has one reverse deactivation
duty; return/throw failures preserve original provenance and never repeat cleanup.
Reprepare retires prior event duties; parameter reconciliation invalidates mapping
readiness. Actual SDK ProcessData carries admitted input/output event lists,
output parameter changes and copied block-start context. Nonempty and zero-frame
SDK fixtures publish counted output prefixes, full signed output note identities
and readonly known output-parameter IDs. Twenty normal/fast build-run commands
pass against the final exact source composition, including provider context/state/
process-pin lifecycle regressions; source/log hashes are recorded in
target/w233-instance-processing-gates/terminal.json. The first zero-frame fixture
incorrectly retained its nonzero expected offset; its failure/source is preserved,
and only that oracle changed. Earlier sixteen/twenty-command source versions and
receipts remain separate. Complete paired provider ABI, whole block/two-pool
admission and original-span protection, safe Rust prepared carriers/MIDI mapping,
actual module consumers and full contract documentation remain required; this
partial integration does not complete hosted processing.

W239 first-order trapezoidal LP/HP/allpass is root-reviewed after all398production/
811test lines and all12 frozen hashes. Full isolated211unique tests/docs and strict
lint/Rustdoc/format pass, including independent analog/polynomial/time-varying
circuit and actual native stereo Graph/FIR consumers. The coherent eight-package
reviewed integration tree contains111guarded outputs under
target/w227-239-reviewed-promotion. Canonical staged docs and whole baseline/input
preflight pass without public adoption. Earlier108-output staging is historical.

The W217–230 joint retry completed1051passed/0failed/4ignored and all-feature strict
Clippy, then terminated101 at default all-target Clippy because SVG integration
tests imported the optional module without requiring svg-image. The exact
already-qualified one-line test feature guard from W231 is now applied publicly;
its prior source/log/terminal result is preserved under
target/w217-230-default-svg-feature-failure. W231 source is unchanged and its one
already-applied public baseline/packet metadata is refreshed. Session79853 runs
the remaining nine commands after that minimal repair; no final combined pass or
reviewed public source adoption is claimed while it is live.


### Paired processing admission and independent reprepare review

W233 target-only event and input-parameter pools now have pure whole-input plans,
non-copying reservation leases and separate activation. Reserving both pools
precedes either input copy; cancellation and retained second-pool exhaustion
preserve complete first-pool batch bytes. Immutable copied protection ranges
reject SDK result/header writes into native-core, sibling-pool and borrowed caller
spans while preserving original owned protocol faults. The first legacy wrapper
regression changed capacity diagnostic precedence; the original failure/source
is retained under target/w233-paired-capacity-initial. Original bounded-capacity
admission now precedes source-span inspection, and the complete suite reruns.

Independent immutable source review found that a stale topology during reprepare
after an earlier successful prepare left old audio/processing readiness visible.
Admitted reprepare now invalidates readiness before foreign topology inspection;
local invalid profiles preserve prior readiness. Actual SDK regressions prove
start rejects without callbacks after stale metadata and a corrected retry retires
old event activation duties once before selecting new duties. The independent
negative counterexample remains under target/w233-ui-instance-review. All36 strict
normal/fast build-run commands and exact current source/log hashes pass at
target/w233-paired-reservation-gates/terminal.json, including event/output-parameter/
input-parameter primitives, Instance and provider retention/lifecycle regressions.
Paired provider ABI, safe Rust carriers and complete actual module consumers remain
required; this evidence does not complete hosted processing or establish DAW support.

W243 paired dry/wet is frozen after237tests/docs and strict isolated checks, pending
source/integration review alongside W242 panning and W234 raw MIDI. The complete
six-stage phaser design is approved for isolated W244 implementation. Additional
independent DSP/AAX source reviews are prepared for Claude Code from frozen
first-party input copies, but both initial launches failed due to expired OAuth;
no Claude review or implementation is claimed. Existing parallel work continues.


### Panning and paired dry/wet integration review

Root reviewed all374production/794test lines of W242 panning and all591production/
1188test lines of W243 paired dry/wet, exact private curve extraction, guides,
ADRs and full isolated221/237tests/docs receipts. All12W242/all16finalW243 frozen
hashes pass. The final W243 documentation clarifies that integer-valued Thiran
delay bypasses recursion and updates both prior values; Integer reconstruction
is separate. Stripped production expressions and all tests are unchanged, with
14isolated doctests and strict Rustdoc passing after clarification. Both packages
are accepted for guarded integration; the coherent ten-package tree now contains
117outputs under target/w227-243-reviewed-promotion. Composed Signal exports are
canonically formatted; staged canonical docs and all-input/baseline promotion
preflight pass. Public source adoption and subsequent compiled qualification are
still pending. Historical111-output staging and all original receipts remain.

Claude Code authentication is renewed. Both isolated frozen first-party source
reviews have now started and performed real Read/Grep operations: panning/dry-wet
and raw-MIDI/AAX. Their actual live handles, immutable input manifests and captured
streams remain under target/claude-parallel-reviews. Initial authentication failures
are separately preserved. No Claude findings or finished report is claimed while
the jobs run; no external SDK or signing fixture was supplied. Parallel phaser,
paragraph UI and generic raw-MIDI implementation continues independently.

### W227/W231/W232/W235/W236/W237/W238/W239/W242/W243 reviewed source integration

The coherent reviewed source set adopts selected internal native32/64 AAX
processing, corrected weighted sampled equiripple/halfband FIR design and
configured allpass halfband/oversampling and canonical runtime native table
acquisition. Host AAX buffers remain f32; source
adoption does not establish Pro Tools/PACE, continuous FIR certification, platform
or deadline support. The FIR original normalized-Nyquist defect and reflected-grid
observations are preserved in target/w232-review-v1 and the root probe; corrected
frozen179tests/docs plus9independent85-digit Decimal profiles pass. Actual stereo
Graph/direct/partitioned consumers and all13frozen hashes were reviewed. The
W236 original cross-codegen table-address mismatch remains in W234 receipts;
corrected six-family/both-precision normal/fast identity and actual processor
fixtures pass. Root paired generic compilation proves runtime acquisition succeeds
and otherwise identical const acquisition fails onlyE0015. W235 source and all
eleven meaningful test groups were independently reviewed: full isolated191
tests/docs, warnings-denied lint/Rustdoc and all13frozen hashes pass. Ideal nominal
attenuation, actual rounded edge sizing and4097-point sampled stop observations
remain separate; the order63/width.001 ideal-versus-rounded edge refusal is
preserved, with no relaxed sizing bound. Existing fixed oversampling defaults
remain unchanged. A root Rustdoc wording correction changes no numerical behavior.
W237 exact C++ client admission removes callback-equivalent copied-core acceptance;
genuine32/64 clients and copied-proxy zero-creator/four throwing-query/three-stage
null/throw acquisition fixtures plus the existing2000cycle client regression
pass3/3. Its W236 guide/CMake overlaps were merged additively and prior policy
observations are preserved under ADR0157, explicitly superseded by ADR0250.

The frozen local AAX precision packet has prior actual native/editor/package
evidence against its exact original archives. All affected current public native
archives and format consumers require fresh post-adoption qualification because
the canonical table acquisition code changes their build. Focused public Signal,
native and packaging checks and a new complete combined workspace gate remain
required; no post-adoption result is inferred from isolated or older receipts.
W238 adds native64/32 peak/RMS ballistics and fixed independent channel banks;
root reviewed all395production/796test lines, nine meaningful isolated groups,
201unique inherited tests/docs and strict lint/Rustdoc/format acceptance. Native
rounding remains separate from ideal timing, with retained initial tail diagnostics
and independently bounded release evidence; no arbitrary tolerance relaxation.
All16frozen hashes verified, no legacy defaults or dependencies change.
W231 adds explicit copied portable/font tokens and opt-in main-thread native
system-font discovery/load plus immutable selected-axis text instances. Root
reviewed portable/native ownership, exact SFNT reconstruction, source-axis/backend
interpretation, consumer publication and numerical/fixture evidence. Modeled
non-macOS strict compilation exposed native-only bookkeeping dead-code warnings;
the narrow cfg correction and both original failures remain preserved. Corrected
portable model4tests and actual macOS23groups/strict lint/doctest/Rustdoc pass;
this is not cross-target platform qualification. Exact frozen native owners use
precise ARC lifetimes; root's initial concern was not a defect in the frozen
source. Supplemental optimized resources8groups/80glyph numeric agreement and
normal29consumer checks pass. Native/browser/shaping limits and negative style
observations remain explicit; no system accessibility/general font claim.
W239 adds native first-order trapezoidal LP/HP/allpass designs, simultaneous
outputs and independent fixed channel banks. Root reviewed all398production/
811test lines and all12 frozen source/evidence hashes. Full isolated211unique
tests/docs and strict lint/Rustdoc/format pass; independent analog/polynomial/
time-varying circuit and actual native stereo Graph/FIR consumer evidence remains
separate from physical/DAW support. The original subnormal-tail, native quarter-
rate coefficient and cancellation-sensitive spectral observation diagnostics are
preserved; final stable half-angle observation changes no runtime recurrence.
No dependencies or existing defaults change. Post-adoption Signal/API and full
workspace checks remain required.
W242/W243 add prepared seven-law/two-normalization panning and paired delayed-dry
FIFO/wet mixing with shared exact consumption-frame gain transitions. Root read
all374/794 panning production/tests and591/1188 dry/wet production/tests, private
curve extraction, contracts and full isolated221/237tests/docs evidence. All12
W242/all16finalW243 frozen hashes pass. Direct wet-coordinate coefficients preserve
tiny requested values; exact frozen panning expression bits stay unchanged. Full
native64/32 channel1..64/four-delay-policy independent impulse/Thiran/tap and actual
three-channel Graph/wet-FIR consumers qualify the declared bounded profile. Reset,
latency retapping, tails/EOF, headroom and queue/refusal contracts remain explicit.
The final Rustdoc/guide correction distinguishes integer-valued Thiran bypass
from Integer reconstruction; stripped expressions/tests stay unchanged and14
isolated doctests/strict Rustdoc pass. Public composition/API/full gates remain
required; no physical/device/DAW/SIMD/deadline inference or new dependency.
Exact integration guards and draft/baseline hashes are retained under
target/w227-243-reviewed-promotion. No new dependencies or lock changes; W231 adds only feature/bin metadata in existing manifests.


### Reviewed public source adoption and manifest correction

All117 guarded reviewed outputs are now adopted at
target/w227-243-reviewed-promotion/public-adoption.json. Prior command qualification
is explicitly composite: two all-feature commands passed before the exact SVG
feature cfg, and nine remaining commands passed after it. Unchanged SVG test-body
bytes and its exact repair hash are verified; original terminal101 remains
preserved. This is not one uninterrupted byte-identical-source run. A fresh
post-adoption11-command driver captures1013public source/build/script inputs and
checks their hashes after qualification; native SDK archive/format gates remain
separate and required.

The first fresh driver terminated101 before compilation because the system-font
example requests ravet-webview/system-fonts but the adopted native crate manifest
omitted that feature. Existing Rust/native build conditionals already implement
the intended opt-in. The canonical manifest now declares default=[] and
system-fonts=[] without new dependencies or lock changes; the existing registry
record clarifies its canonical Cargo feature and already references the manifest.
The original manifest, failed log and terminal receipt remain under
target/w227-243-public-initial-font-feature-failure. Isolated feature manifests
did not prove canonical workspace resolution; no fresh compiled acceptance is
claimed from that failed launch. The repaired full driver follows.

### Independent review and next source packages

Claude source-only reviews completed with terminal exit0 and all frozen inputs unchanged: `target/claude-parallel-reviews/{dsp,aax-midi}/{REVIEW.md,terminal.json}`. DSP review found no runtime defect, with precise panning/dry-wet contract wording and additional composed fractional-latency evidence to resolve after the current workspace source snapshot gate. AAX raw-owner review found an automation/state-recall admission regression; the format owner confirmed it from source and is preparing actual-module counterexamples and a corrected guarded packet. The old W234 packet remains historical and is not adopted. Static reviews are not executed tests or host validation.

W244 six-stage native phaser source review accepted the complete373-line implementation,1145-line tests, exact private first-order helper addition, guide/ADR/record and all16 frozen hashes. `target/w244-root-review.json` records isolated237 tests plus15 doctests, strict lint/docs/format qualification and selected native32 rounding observations. Public integration and fresh workspace replay remain pending; no arbitrary modulation stability, device/DAW or deadline inference. W245 nonlinear ladder and W246 optional PostScript outlines architecture proposals are approved for separate target-only implementation; those numbers represent plans, not delivered public capabilities.

### Target-only processing metadata ownership refinement

W233 now retains validated VST3 parameter texts inline inside the bounded catalogue vector, so complete SDK argument protection can cover every owned metadata byte with two numeric ranges instead of thousands of separately allocated strings. Full1024 parameters with three381-byte texts, terminators, independent owner copy/move and blocked text destinations pass normal/fast fixtures. `target/w233-inline-metadata-gates/terminal.json` records all48 build/run commands passing for current source, including retained parameter/event interfaces, paired reservations, stale readiness correction and actual processing-pool byte budgets. Earlier36-command evidence is preserved for its source version. This target-only internal storage implementation does not finish the paired provider ABI, safe Rust adapter or full actual format consumers.

### DSP source-review follow-up and coherent phaser staging

Claude DSP contract findings are resolved in target/w243-claude-followup: both precisions reject actual fractional Thiran a rounded1; Integer getters describe unused rounded values rather than executed delay; native delay arithmetic, f64 pair interpolation, rounded-reciprocal panning normalization, caller denormal policy and private lossy wet-coordinate metadata are explicit. Comment-stripped runtime expressions are byte-equivalent to current public panning/dry-wet source. The retarget fixture uses the declared multiplication rather than fragile division equivalence. Six new meaningful groups pass: native64/32 largest fractional taps near maximum across history/FIFO wraps, exact dyadic wet mirrors/energy/headroom, Integer getter-versus-actual-impulse delay and native64/32 actual three-channel Graph/FIR fractional2.5-delay cancellation against independent finite convolution/closed response/low-frequency bound. No broadband perfect-alignment claim is made. Full isolated229 tests plus14 doctests, strict lint/Rustdoc and formatting pass in target/w243-claude-followup/terminal.json.

The accepted W244 phaser and DSP follow-up compose as16 guarded outputs in target/w244-claude-reviewed-promotion, with canonical staged documentation checker/preflight passing. A separate complete composed Signal replay passed243 tests plus15 doctests and strict lint/docs/format; exact source/log hashes are recorded in target/w244-claude-joint/terminal.json. Public apply waits for the current all11 workspace gate and queued fresh native archive/contracts to finish without source changes. The dedicated promoter enforces those receipts, source hashes and every current destination baseline; no public apply yet.

Claude paragraph static review completed exit0 with145 inputs unchanged. Predicted legal trailing BN/ZWJ/tag grapheme failure and publication-recovery issue are being reproduced by the UI owner before revised W240 integration. Original frozen W240 packet remains historical; W246 work is briefly held behind these corrections. Documentation command/URL corruption is also queued for correction. No Unicode conformance or external host support follows from these static reviews.

### Explicit independent reviewer model preference

The user requires Sonnet5.5 for Claude reviews. Future CLI launches now explicitly pass `--model claude-sonnet-5-5`, have no fallback flag, and verify the initialization model in the terminal receipt. The corrected W234 revision review is running with actual initialization reporting exactly that model. A42-second launch that incorrectly omitted the argument reported Opus5.5; it was stopped, all57 inputs remained unchanged, no report was produced, and its negative model-verification receipt is preserved under target/claude-parallel-reviews/aax-midi-r2-sonnet/wrong-default-model-attempt. Earlier completed reviews truthfully retain their original Opus5.5 model receipts and are not relabelled.


### Shared containing-core metadata protection and verified Sonnet review

W233 target-only processing now seals one final-address immutable numeric snapshot per exact native core. Maximum256-class metadata uses1544 of1600 fixed ranges, covering every owned inline/heap string capacity and terminator plus canonical key, catalogue/vector, core and snapshot storage. Event and parameter pools borrow it; exact containing-core ownership/pinning keeps it live through escaped SDK references, and pool destruction precedes metadata destruction. No AUDIO expansion or retired facade/Instance dereference is introduced. Known RAVET-owned allocations are the boundary, not unobservable foreign SDK storage. `target/w233-core-metadata-gates/terminal.json` and root review record all52 normal/fast build/run commands passing with current source/log hashes verified. Three new groups include all1536 meaningfully aligned class SDK write destinations, refused input/output/event destinations and healthy retries, complete maximum metadata/capacity publication, and controlled exact core retirement/reopen with retained frozen SDK queries and worker new/delete0. Original first52 test version is preserved; final alignment refinement does not change production source. Paired provider exports, safe Rust carriers and full actual format consumers remain unfinished.

The corrected W234 independent review completed through explicit Sonnet5.5: terminal0, reported model `claude-sonnet-5-5`, model verification true and all57 frozen inputs unchanged. Its report is source-only and identifies a fresh raw early-layout rejection diagnostics defect, weak concurrent-consumption evidence and incomplete harness/dependency provenance. A separate R3 revision is in progress; R2 stays frozen. Earlier Opus reviews retain their original truthful model records. Future Claude launches require explicit Sonnet5.5 and verified model receipts, with no fallback.

W240 legal trailing formatting and browser-publication failures were reproduced. Revised target-only paragraph rendering applies an explicit atomic-grapheme representative-level policy distinct from scalar-exact Unicode bidirectional conformance; fixed Prepend/ZWJ/tag vectors and independent shaping observations accompany it. Browser recovery uses an exact read-only refresh request and monotonic delivery identity so cached JSON republishes without changing state revision. Actual own-window rejection/recovery/subsequent geometry-change proof passes; final corrected browser/source packet remains pending. W247/ADR0260 typed indexed composition and shared-control mono duplication architecture is approved for isolated implementation, using canonical Graph execution and singleton UnsafeCell storage with exclusive owner/session borrowing. Miri is unavailable on installed toolchains; no Miri pass is claimed. These scoped target packages do not close broad programme rows.


### Reviewed ladder composition and processing preparation completion

Root read the complete605-line nonlinear ladder implementation,1098-line tests, independent Decimal generator/input exporter, guide/ADR/registry/export and qualification driver. All18 frozen hashes and four terminal0 commands verify259tests+16docs; root independently recomputed all816 Decimal450 oracle vectors and exact bytes match. `target/w245-root-review.json` accepts the declared single-saturator/four-RC six-mode contract. Independent explicit Sonnet5.5 review is running from42 frozen first-party inputs; no result inferred yet. Coherent phaser/ladder/DSP-follow-up staging now has22 guarded outputs and passes canonical staged docs/whole preflight. The complete composed Signal replay passes265tests+16docs, strict all-target lint, Rustdoc and format with source/log hashes verified in target/w244-245-claude-joint/terminal.json. No public apply; the dedicated promoter requires fresh all11 workspace/native/editor archive gates plus resolved independent review.

W233 target-only parameter refresh now permits compliant controllerless SDK components to reconcile an empty catalogue and prepare actual event/context processing. Read/edit still requires a real controller; no synthetic parameter IDs. A separate33-range immutable prepared snapshot protects all32 event-bus name capacities/terminators and whole catalogue storage. Actual381-byte maximum UTF8 names plus8/0/1-frame controllerless SDK event callbacks pass; all52 normal/fast native build/run commands and current source/log hashes verify at target/w233-processing-metadata-gates/terminal.json. Initial new test diagnostic string escaping failed compilation and is preserved; correction changes fixture only. Complete paired provider ABI/safe Rust/actual modules still required.

The fresh public all-feature workspace tests passed; current lint/docs/remaining commands remain live. A separate canonical native-editor Gain release archive job is queued at target/w227-243-editor-cache, preserving the default native archive cache/receipts. That future path is not current artifact evidence; downstream actual SDK/editor consumers remain required. Corrected W240 R2 source/evidence and corrected W234 R3 source/evidence are frozen for root review. Independent W246 source work resumes under its approved architecture without mutating either UI packet.


W233 target-only paired VST3 processing companion checkpoint: the exact
ProviderV2 companion now supplies stopped copied selection/bus/MIDI-mapping
metadata and paired input-parameter/event processing with atomic complete
event/payload/output-parameter publication. Whole caller capacities are admitted
before both queue reservations, input copy or DSP. Late failure silences valid
audio prefixes, preserves output counts/storage and records conservative history
progress, original native return or a distinct throw. Strict C11 and normal/fast
C++17 provider/primitive/module fixtures passed all58 build/run commands with
source/log hashes verified; the actual controlled SDK fixture covers9 input/10
output event kinds, context/parameters, local and late failure, reentry,10000
variable/zero calls with no measured framework new/delete and16 retained slots
with paired cancellation/release continuation. This remains outside public
source. Safe Rust, actual Gain/Performance/output-image loading and whole
sibling Instance metadata admission are unfinished; no DAW/platform/deadline
claim. Historical48/52 receipts retain their earlier source versions.

### Reviewed DSP adoption and parallel validation checkpoint

The fresh pre-adoption public workspace qualification completed all eleven
commands with no changes to its 1,013 captured source/build/script inputs.
Independent native archive/contracts qualification completed four commands;
the separate editor archive command also passed. Exact archive and source hashes
were verified before further source adoption. These are local build and fixture
results, separate from DAW, device and deadline evidence.

The revised ladder packet closes the fourteen independent Sonnet 5.5 source
review findings within its declared model. Root regenerated all 2,016 Decimal450
history vectors and 60 alternate-equation vectors, matching the frozen bytes.
The coherent phaser/ladder/panning/dry-wet replay passed 278 tests and 16 doctests,
strict all-target lint, Rustdoc and formatting. Its 29 guarded implementation,
test and documentation outputs are now adopted in the public tree. Registry
generation and the documentation checker pass. Fresh integrated workspace
qualification is running; isolated results do not stand in for that acceptance.
The original ladder model and review receipts remain preserved.

The target-only safe Rust VST3 processing owner passed 24 unit tests and 17
doctests in both debug and release, with strict lint/Rustdoc/format passing.
An actual Rust-to-native SDK-loader consumer passed seven commands against
explicit existing Gain and Performance bundle artifacts. It checks mono/stereo
timed gain and ramp behavior, zero-frame retargeting and audible timed instrument
events, level and context against independent formulas. The instrument maximum
absolute error was approximately 5.596e-11. Native provider archive construction
passed three separately source/dependency-bound commands. This is local macOS
arm64 in-process evidence, with no DAW or arbitrary plug-in support inference.

The subsequent Sonnet 5.5 processing review completed with all 86 first-party
inputs unchanged. It identified missing discriminating tests, incomplete native
test registration/provenance and a native terminator-budget admission mismatch.
Corrections are in a separate target-only revision: four focused Rust groups
already pass for whole-call refusal preservation, cumulative text termination
budgets, CONTROL success facts and complete output capacities. Sonnet is writing
the remaining Rust wire tests while a separate owner extends native registration
and rich callback measurements. Original qualified sources stay frozen. Full
sibling-instance metadata protection and CONTROL exception provenance remain
unfinished; this package has not been adopted publicly.

The actual consumer also passed a separate seven-command replay against fresh
Gain and Performance VST3 modules from the frozen W241 C++ composition, linked
to the canonical W227-243 native archives. Root verified every bound source,
module, SDK dependency, archive, binary and command-log hash; the same independent
instrument error bound holds. The first supplemental provenance receipt had
hashed its outer log before the final print. Root refused that receipt before
executing modules; the original is preserved, and a separate closed-output
receipt verifies all 1,137 paths. No module rebuild was needed for that correction.

### Revised VST3 wire and actual-module qualification

The separate W233 R2 safe Rust owner passes 37 unit tests and 17 doctests in
debug and release, strict all-target lint, warning-denied Rustdoc and formatting.
The expanded tests discriminate complete input/output wire arguments, malformed
reports, native UTF16 terminator budgets, refusal preservation and malformed
CONTROL success. The first complete run caught three incorrect expected
non-payload offsets in the authored test; its source and failure log are
preserved. The correction changes those expected literals to the existing
owned wire cursor and does not change production behavior. A separate C11
compile/run confirms thirteen wire structures and 121 literal field offsets
on the measured 64-bit profile.

Revised native fixtures pass seventy build/run commands plus two compiler
probes. A separate maintained CMake build passes four commands and twenty-two
registered normal/fast fixtures. The 10000-call rich native workload covers nine
input and ten output kinds, all thirteen context presence bits, copied payloads,
queues and zero/variable frames with zero measured C++ operator new/delete.
This measurement excludes malloc and arbitrary foreign SDK allocation. Root
verified the declared source, log, dependency, external SDK and archive hashes.

A separate R2 Rust-to-native SDK-loader consumer passes seven build, actual
module, lint, documentation and formatting commands against the fresh W241 Gain
and Performance module artifacts. Debug/release mono/stereo gain, smoothing,
zero-frame retargeting, instrument timing and backing tails match independent
formulas; maximum instrument error remains approximately 5.596e-11. Its reused
runtime archive has twenty-two compiled first-party dependencies proven
byte-identical to the revised native tree. All 3,306 captured consumer inputs,
binary and command-log hashes verify. These target-only local macOS arm64
results establish no DAW, other-platform or deadline support. Complete sibling
metadata protection and CONTROL failure provenance are being implemented in a
separate native R3 tree; public adoption remains pending.

The explicit Sonnet 5.5 typed-composition review finished with all thirty-four
frozen inputs unchanged. It identifies misleading manual-owner lifecycle
documentation and a non-discriminating compile-fail example, plus meaningful
test gaps. A separate Sonnet 5.5 follow-up is addressing those findings while
preserving the canonical manual Processor hook contract. Its new tests and
documentation are not qualified or publicly adopted yet.

### Peak-meter graph coverage and LV2 source review

Root read the complete W250 finite-FIR peak-meter implementation, both API and
actual Graph/FIR test files, independent coefficient/reference generators,
bounded worker example and proposed documentation. Recomputing all 1,276
Decimal90 reference values produces byte-identical output. The original packet
remains frozen. Its graph test used mutable meter scratch but separate external
graph buffers; a separate R2 test crosses actual separate/in-place Graph buffers
with both meter observation views, in both precisions and three profiles, across
two preparation lifecycles. Twenty-four distinct compositions pass in debug and
release with independent audio, measurement, tail and heap checks. These remain
two runtime test groups, with optimized repeats counted separately.

The revised complete isolated suite passes 287 tests and 17 doctests, with
strict Rustdoc, all-target lint and formatting passing. Source and command-log
hashes verify; only the Graph test differs from the original draft. An explicit
Sonnet 5.5 static review is running from forty frozen inputs. The finite estimate
retains its known near-Nyquist accuracy limits and is not publicly adopted or
claimed to meet a physical audio deadline.

Root also reviewed all seven W248 exact-origin LV2 source/doc changes and checked
the original guards and ninety-file actual-module supplement. Its original
promoter correctly refused four changed Signal dependencies after DSP adoption;
each current byte matches that separately reviewed adoption. A reconciled guard
set preserves the original receipt. An eleven-output source/registry/generated
catalogue/index/status proposal passes the staged documentation checker and
whole preflight. Public source adoption waits for the running integrated
workspace qualification; fresh native/module acceptance remains separate.

### Qualified typed-composition follow-up and independent numerical review

The explicit Sonnet 5.5 typed-composition follow-up finished with all thirty-five
frozen inputs unchanged. Root reviewed its production comments, revised tests,
borrow examples and lifecycle documentation. A separate R2 tree now passes
81 runtime tests and 14 doctests, strict all-target lint, strict Rustdoc and
formatting. Root checked all 295 source inputs and command-log hashes. The first
run exposed a test fixture that moved inline controls after recording their
address; boxing the fixture before preparation preserves the meaningful address
assertion. A test-only type-complexity lint was also corrected. Both initial
failures remain preserved. Executable production behavior is unchanged, including
the canonical manual audio-owner obligations. Root corrected the safety comment
to allow a returned shared stage reference on another thread when its processor
is Sync, while its enclosing borrow still excludes graph callbacks. Public
source adoption and Miri qualification remain separate.

The W250 static Sonnet 5.5 review also finished. Root verified its reported model,
all forty frozen inputs and session-log hash. It reports no confirmed runtime
bug and identifies meaningful evidence gaps: poisoned Graph flags before reset,
independent post-reset FIR outputs, exact accuracy-limit regression assertions,
trailing source support and per-channel exceptional-input isolation. A separate
Sonnet 5.5 follow-up starts from the already qualified R2 graph-view matrix with
fifty frozen inputs; its new tests remain unexecuted. Known finite-grid accuracy
limits remain recorded. The original numerical packets are unchanged.

The W233 R3 native snapshot remains frozen; a separate R4 adds core-owned
controller-handler retention, guarded query destinations, early provisional
publication and accurate local-exception provenance. Dedicated lifetime and
aliasing fixtures pass locally; complete native/CMake qualification and root
review are still pending. Calibrated loudness/LRA has a frozen isolated package
awaiting root review, with a separate Graph flag-evidence correction underway.
The separate native denormal/SIMD architecture is approved for isolated
implementation. These target-only packages establish no new DAW, physical
platform, hardware deadline or public release support.

### Qualified finite-peak follow-up and paired native ownership replay

The Sonnet 5.5 finite-peak follow-up completed with all fifty frozen inputs
unchanged and only its eight allowed outputs changed. Root verified the actual
initialization model, session log and all ninety-seven output hashes, then read
the revised numerical/Graph tests, comments, guide, decision and worker script.
A separate R3 copy passes seven qualification commands: 294 runtime tests and
17 doctests, nineteen optimized peak/Graph groups, strict all-target lint,
strict Rustdoc, formatting, the example build and thirty-five generated worker
admission checks. Four test loops were rewritten as enumerated iterators for
lint, and formatting was applied; numerical assertions and tolerances did not
change. The original two lint failures and format failure remain preserved.
Executable production tokens are unchanged. Exact 16 MiB worker success was not
run; its optional check is explicitly skipped. Known finite-grid numerical
limitations remain visible. Public source adoption is still separate.

Root reviewed the complete native VST3 R2-to-R4 production changes and new
retained-handler, sibling, state and provenance fixtures. Independent evidence
verification binds 574 original/revised packet guards, 98 fixture commands,
two compiler identity probes, four CMake commands and 31 selected CTest profiles
to 4,404 unique current files, including 1,712 external SDK files. A separate
consumer links the qualified safe Rust R2 API to the freshly built R4 provider
and SDK archives. All seven consumer gates pass; actual Gain mono/stereo timed
parameters and Performance timed notes, expressions, parameters and context
match independent audio references in debug and release. The Performance maximum
error is 5.5957650622537325e-11. Loaded reference modules retain their separately
qualified W241/W227-243 provenance. Native failure/retention evidence comes from
the native fixtures, not the positive loaded-module replay. Separately allocated
SDK helper storage and foreign heap remain outside declared range/heap checks;
no DAW, deadline or physical platform support is inferred.

A separate coherent tree composes thirty-two typed/mono, finite-peak and reviewed
LV2 origin source/documentation outputs, with joint Graph/Signal checks running.
Its minimal lockfile update adds only the existing Signal development dependency
to Graph. Graph denies unsafe outside the audited private typed-storage module;
other workspace crates keep their policy. No public compiled source was changed.
The full native32/64 indexed-bus and owned Graph hosting architecture is approved
for isolated implementation, including checked reset refusal and MAIN retirement
of worker tokens. Hosted-editor completion remains independent required work.
Sonnet 5.5 is reviewing twenty first-party loudness/LRA inputs without executing
code. New native denormal, SIMD and generated-LV2-output packages remain subject
to their own root review and integration; they do not establish general support.

### Composed source qualification and result classification correction

The coherent typed/mono, finite-peak and LV2-origin staged tree passes all six
focused integration commands: Graph/Signal tests, strict all-target lint, strict
Rustdoc, workspace formatting, the documentation checker and script regression
tests. The result is 374 runtime tests and 38 doctests, 412 combined. Public
source adoption and complete workspace qualification remain separate.

The isolated typed/mono R2 log contains 74 runtime tests and 21 doctests: seven
ordinary merged Rustdoc tests plus fourteen standalone compile-fail tests. Earlier
81 runtime / 14 doctest statements classified those seven Rustdoc tests incorrectly;
the total of 95 and successful qualification are unchanged. Original receipts
remain preserved with a separate classification correction. Staged current
capability and decision text use the corrected split. No implementation changed.

### LV2 exact-origin source adoption

The W248 reusable helper now admits the exact originating native core and shared
cache/state schema before resource creators. Callback-equivalent copied cores or
separately copied schemas refuse; coherent top-level cache/state copies remain
valid because this ABI supplies no stronger table-address identity accessor.
Metadata queries precede acquisition and violating query exceptions refuse.
The maintained actual-origin validator is required by both bundler profiles.
Root reviewed all seven source/doc deltas, checked the frozen guards and the
ninety-file supplemental actual-module evidence, and reconciled four dependency
changes exactly to the reviewed DSP source adoption. Original packets and the
initial drift refusal remain preserved. Isolated CMake seven tests, Gain three
validators, Performance five validators and four bundler tests pass; the separate
canonical supplement uses the explicitly qualified W227-243 native archives.
Fresh integrated public native module qualification remains a separate next gate.
No DAW, Linux runtime, broader precision/profile or deadline support is inferred.

### Typed composition and finite peak source composition

This staged tree composes the reviewed typed/mono APIs (74 runtime tests and21
doctests), finite peak follow-up (294 runtime tests and17 doctests,19 release
groups and35 worker admission checks), and exact-origin LV2 change. Six focused joint
qualification commands pass (374 runtime tests and38 doctests); guarded public
adoption and full workspace qualification remain separate. Typed storage
uses a narrowly permitted private unsafe module under Graph's deny policy. Other
workspace policy is unchanged. Peak numerical assertions/tolerances are unchanged;
exact16MiB worker success was not run. No DAW, deadline or physical support claim.

### Reviewed LV2 generated-output source composition

The generated-output helper, opt-in processor profile and canonical NoteRouting
reference have completed root source review. All sixteen authored outputs,
twenty-seven packet inputs, 931 dependencies and 850 evidence paths verify.
Five incoming files compose explicitly on the adopted exact-origin LV2 base.
The final isolated package passes six mandatory validators and strict ad-hoc
signature checks; seven final default validators give twelve unique native
profiles. Six Python tests verify mandatory failure propagation. That original
pipeline substituted recorded canonical Rust archives. This source composition
does not itself establish a fresh archive/module replay, public workspace
acceptance, a DAW or Linux runtime result, or a real-time deadline.

### Integrated composition and fresh LV2 replay

The reviewed typed/mono graph composition, finite interpolated peak estimator and
exact-origin LV2 ownership changes are adopted locally. Their fresh integrated
workspace passes all eleven test/lint/doc/format/Python/whitespace commands with
1,043 compiled/build/script inputs unchanged and all thirty-two adopted outputs
verified. The isolated source composition retains its earlier 374 runtime and
38 Rustdoc tests; the typed-only split is 74 runtime and 21 Rustdoc tests.

The generated LV2 output composition now has a separate fresh replay: all
thirty-one commands pass against 1,700 unchanged source/SDK/driver inputs. Actual
current-source Cargo builds Gain, Performance and NoteRouting; all twelve native
validators pass, and the maintained NoteRouting pipeline executes actual Cargo,
its six mandatory packaged validators and strict ad-hoc signature verification.
The earlier helper field-name failure and both incomplete documentation staging
attempts are preserved; they changed no production implementation or assertion.
This is local synthetic macOS native evidence. Generated-output source adoption
requires its own fresh workspace acceptance; no LV2 DAW/Linux/physical device or
real-time deadline qualification follows.

The scoped floating-point policy package has also completed root source/evidence
review. Its separate packed/vector extension is qualified in isolated ARM and
translated-x86 runs and undergoing root/Sonnet 5.5 static review. Loudness/LRA
Sonnet 5.5 follow-up has produced stronger tests and clarified numerical limits;
those new tests still require execution. Hosted processor, Audio Unit and
configurable convolution work continue in separate source trees.

### Generated LV2 output post-adoption public acceptance

The fresh post-adoption public gate for the 21 adopted W252 generated LV2
output source/documentation files passes all eleven commands: all-feature
workspace tests (1,295 passed, zero failed, four ignored external oracles),
all-feature and default warning-denied Clippy, doctests, default and
all-feature strict Rustdoc, formatting, documentation checks, script and REAPER
evidence-check Python tests, and diff checks. All 1,051 bound source inputs are
unchanged in the current tree. Receipt target/w252-public-gates.log and
terminal.json, exit 0. This is offline workspace evidence; native format
modules, DAW, Linux, physical device and deadline qualification remain separate.

### Loudness/LRA follow-up test qualification (R4)

The R3 qualification of the Sonnet 5.5 loudness/LRA follow-up tests passed its
debug and release test runs, then failed strict all-target lint on a needless
borrow; receipt target/w251-r3-root-gates.log and its logs remain preserved. A
separate R4 copy changes only two test lines' lint shape: the closure is passed
by value and a function-pointer type alias replaces an inline tuple type. No
assertion, tolerance or production token changed. All seven R4 commands pass:
348 all-feature tests including 18 doctests, 34 release loudness/Graph/calibration
tests, strict all-target lint, strict Rustdoc, doctests, formatting and the
loudness-file example on an independently generated 997 Hz fixture, with zero
source changes. Receipt target/w251-r4-root-gates.log and terminal.json, exit 0.
This is synthetic numerical/Graph evidence only; full source review and public
adoption remain separate, and no standards, host, hardware or deadline
certification follows.

### W251 calibrated loudness and loudness range public adoption

Root-delegated review completed all 1,256 production lines of the prepared
loudness meter, its public API, example, oracle data and three test files with no
blocking defect (target/w251-root-review.md). K-weighting, channel weights,
400 ms/3 s windows, strict integrated gates and inclusive LRA gates/percentile
ranks match the documented profile; processing is allocation-free after
preparation with whole-call refusal. Non-blocking notes (an out-of-range
frequency reported as a coefficient error, sparse item Rustdoc) remain recorded.

Ten new and seven modified public files were adopted after verifying every public
base and staged hash; only an additive read-only `Biquad::delay_state` accessor
changes existing source, and Cargo manifests/lock are unchanged. The identical
staged workspace passed all 29 public gate commands: 1,339 all-feature workspace
tests (zero failed, four ignored external oracles), 58 doctests, default and
all-feature warning-denied Clippy and Rustdoc, formatting, documentation checks,
58 script and four REAPER evidence Python tests, byte-identical oracle CSV
regeneration and whitespace checks. Receipt target/w251-adoption-gates-r2 and
packet target/w251-adoption.md. FFmpeg comparison figures remain earlier
recorded receipts. This is synthetic numerical and offline Graph evidence; no
standards certification, EBU Mode, host, hardware or deadline claim follows. See
[decision0264](decisions/0264-prepared-calibrated-loudness-range.md).

### W257 native convolution schedules public adoption

The native f32/f64 zero-latency, fixed and nonuniform partitioned convolution
engines, bounded 1–64 channel banks and the additive `FftPlan32` are adopted in
ravet-signal, which keeps no runtime dependencies: the processor/Graph adapter is
test-support code using existing development dependencies. The only existing
source change shares the f64 FFT twiddle preparation without changing its
expression order; existing FFT tests and assertions are unchanged. Root accepted
the bounded `ConvolutionBank32/64::engine_mut(channel)` accessor as the one other
public addition.

The isolated revision passes 327 tests, 32 focused release tests and 18
doctests with strict lint, Rustdoc and formatting. Twelve review groups check
bit-identical block partitions (including 1, odd, maximum and zero frames,
separate and in-place), exact stage-boundary impulses, reset at every stage
boundary, empty/zero/subnormal responses, frequency response, bank refusal
without state change and zero measured processing allocations. Measured maximum
errors against independent references stay below 1.2e-13 (f64) and 2.7e-6 (f32);
f64 uniform mode is bit-identical to `PartitionedConvolver`; full-size f32 FFT
leakage of 3.2e-11 is recorded with its strict assertion preserved.

Fourteen public paths were adopted after verifying every base and staged hash.
The identical stage, rebased on W251, passed every public gate: 1,283 all-feature
runtime tests in 300 binaries (zero failed, four ignored), 84 doctests, default
and all-feature warning-denied Clippy and Rustdoc, formatting, documentation
checks, 58 script and four REAPER evidence Python tests and whitespace checks.
Packet target/w257-adoption.md; failed intermediate receipts are preserved.
Source-rate response preparation and live transition/worker ownership remain
separate work. No real-time deadline, DAW, device or platform claim follows.
See [decision0270](decisions/0270-native-convolution-schedules.md).

### W256 asynchronous AUv3 host ownership public adoption

The default-disabled ravet-host `au-hosting` ABI and its macOS native provider
(`crates/ravet-host/native/au`) own asynchronous AUv3 instantiation, bounded
pending requests, MAIN retirement of worker completions and joined main-bus
rendering. Render reentry, MAIN prepare/end during rendering, synchronous
completion reentry, success-with-NSError, cancellation after delivery,
duplicate completion and forgotten tokens are refused or retired exactly once.
2,000 prepared calls perform zero C++ `new` and zero Rust allocations. The Rust
consumer and native fixtures run through
`tools/au-host-validation/check-au-host.py` against the root-qualified Gain
archive; `extension.mm` is manual-only. Failed staging receipts (intra-doc link,
module order, stale Cargo fingerprints after timestamp-preserving sync) remain
preserved under target/w256-adoption.md.

Exact-path AUv3 registration of the ad-hoc signed public Gain extension from a
user cache location was attempted and cleaned up: PlugInKit and AudioComponent
matches were zero before, after and after cleanup, and the actual host probe was
unavailable. Out-of-process rendering therefore remains unvalidated; further
signing, installation or registration needs explicit owner approval.

### Combined public acceptance after W251, W257 and W256

The public tree with all three adoptions passes all twelve combined commands on
a fresh exclusive cache: 1,287 all-feature runtime tests and 92 doctests (zero
failed, four ignored external oracles), default doctests (59), default and
all-feature warning-denied Clippy and Rustdoc, formatting, documentation checks,
script and REAPER evidence Python tests, `git diff --check`, and the AU host
driver (native CTest 2/2 and actual Rust consumer). No bound source changed
during the run. Receipt target/combined-251-257-256-gates.log and terminal.json,
exit 0. This is local macOS arm64 offline and in-process evidence; no DAW,
auval, out-of-process AUv3, device, other-platform or deadline claim follows.
See [decision0269](decisions/0269-auv3-asynchronous-host-ownership.md).

### W253/W254 `ravet-native-dsp` public adoption

The new workspace crate `ravet-native-dsp` adopts the root-accepted W253 scoped
native floating-point policy and the W254 R2 explicit packed values and prepared
vector slices. The crate denies unsafe code except in its two private native
modules; the portable workspace policy is unchanged. Native vector admission now
also requires `neon` (AArch64) or `sse2` (x86_64), a pure tightening with
unchanged release instructions on both macOS targets. libm is declared exactly as
in Foundation (`=0.2.16`, default features disabled), so combined builds enable
no libm features; its licence is recorded as MIT only. The lockfile adds only the
new package entry.

The W254 Sonnet 5.5 static review found no production defect. Its R2 follow-up adds
30 tests (all 17×17 special-value pairs in every lane under Preserve and FlushBoth,
bit-exact slices, exact Graph mixer stage and complete refusal coverage); seven
production mutations all fail the suite, five only through the new tests. With
libm defaults disabled, every asserted sqrt result is unchanged; Scalar sqrt of the
smallest subnormal inside FlushBoth now observably returns the input bits while
native hardware flushes, and remains policy-defined and unasserted.

The stage passed 15 gates, including thumbv6m no_std, x86_64 and Rosetta x86_64
release tests (target/w253-254-adoption.md). After merge on the W251/W257/W256
public tree, all fourteen combined commands pass on a fresh cache: 1,349
all-feature runtime tests and 94 doctests (zero failed, four ignored), 61 default
doctests, warning-denied Clippy and Rustdoc, formatting, documentation, Python and
whitespace checks, the AU host driver and the new crate's thumbv6m and x86_64
checks. Receipt target/combined-2-native-dsp-gates.log, exit 0, with no bound
source change. Rosetta translation is not physical x86 evidence; timings are not
deadline evidence. See [decision0266](decisions/0266-scoped-native-floating-control.md)
and [decision0267](decisions/0267-explicit-packed-register-and-slice-family.md).

### W233 VST3 processing companion and W255 hosted VST3 audio public adoption

W255 depends on W233's paired processing companion, which had been root-accepted
but not publicly adopted, so both were staged together with every path labelled
by origin (38 W233, 13 W233 with W255 changes, 35 W255, plus shared registry,
index and guide files). W233 contributes prepared hosted VST3 events, parameter
points and block context: all nine SDK input and ten output event kinds use one
bounded wire format, refusal before dispatch preserves state, late faults publish
no partial batch, and the containing native core owns retained SDK batches and
handler keepers. Root accepted the adoption-time ADR 0246 after checking its
claims against the W233 packets.

W255 adds explicit stopped-instance bus negotiation with selected native32 or
native64 profiles (no silent precision fallback), all advertised main/aux buses
requested explicitly, per-node rich event/parameter lanes, and an owned Graph
bank whose node drops and abandoned controls retire on MAIN after every audio
lease ends; unverifiable reclamation retains the bank. Reset of a hosted node
reports rebuild required, and the CLAP Graph node now forwards checked reset
results. Changes to the adopted typed Graph, ravet-native-dsp processor and
ravet-host manifest were three-way merged; `au-hosting` is retained and the
workspace lockfile is unchanged.

The stage passed every gate on its first run: 1,500 workspace tests, strict
Clippy/Rustdoc/formatting, documentation and Python checks, 126 normal/fast-math
native fixture commands, VST3 CMake/CTest 61/61 including actual Gain/Performance
targets, and the actual-module consumer in debug and release (exact Gain mono/
stereo native32/64, Performance maximum error 5.6e-11 native32 and 3.7e-18
native64, Graph bank recall and abandoned-control retention, 10,000 Graph blocks
with zero Rust AUDIO allocations). All 94 public paths were adopted after base
and staged hash verification. The combined public run then passed all fourteen
commands on a fresh cache: 1,398 runtime tests and 102 doctests (zero failed,
four ignored), 61 default doctests, warning-denied Clippy and Rustdoc, formatting,
documentation, Python and whitespace checks, the AU host driver and
ravet-native-dsp thumbv6m/x86_64 checks. Receipts target/w255-adoption.md and
target/combined-3-vst3-gates.log, exit 0.

Ordinary framework event/transport translation for Graph nodes, a shared
Sidechain VST3 module, a loaded rich-event module through the Rust bank, module
unload-versus-pin reporting and VST3 editor hosting remain open. Foreign SDK heap,
DAW, third-party plug-in, other-platform and deadline claims do not follow. See
[decision0246](decisions/0246-hosted-vst3-events-context.md) and
[decision0268](decisions/0268-hosted-vst3-graph-banks.md).

### W240 font-stack paragraphs and browser publication recovery public adoption

W240 R2 adopts the explicit atomic-grapheme representative-level paragraph policy
(distinct from scalar-exact Unicode bidirectional conformance) and exact
read-only browser publication recovery with monotonic delivery identity. A
root-delegated source review confirmed the earlier blocking findings (grapheme
split at line end, non-monotonic work, stuck busy state and recovery) are
resolved; the remaining copyable-command and version-spacing documentation
corruption was corrected during adoption (target/text-chain-review.md).

Twenty-eight public paths were adopted after base and staged hash verification;
Cargo.lock and LICENSING are unchanged. The stage, built from the current public
tree, passed all public gates: 1,524 all-feature workspace tests (zero failed,
four ignored), 63 doctests, default and all-feature warning-denied Clippy and
Rustdoc, formatting, documentation, 58 script and four REAPER Python tests and
whitespace checks. W240's own suites pass: the HarfBuzz 14.4.0 oracle (22 cases,
95 runs, 121 glyphs plus fixed wrap, visual and grapheme vectors), the
own-window native consumer (41 groups), authored export (274 states), hidden
WKWebView (65 groups) and headless Chromium over loopback DevTools (65 groups,
trusted Wider click/Enter/Space width progression). A first native-suite attempt
failed only because the Cargo-slot runner printed status lines into captured
oracle JSON; that receipt is preserved. Packet target/w240-adoption.md. No
Unicode conformance, native accessibility, VoiceOver, other-platform or deadline
claim follows. See [decision0253](decisions/0253-font-stack-paragraphs.md).

W246 PostScript outlines are not adopted: review reproduced a panic from a
crafted CFF top DICT reaching a public `Result` API (unchecked operand-group
indexing after read-fonts and RAVET tokenized a large-exponent real differently).
A separate R2 correction is in progress.

### W234 R4, W241 and W249 AAX chain public adoption

The frozen AAX chain is adopted as one coherent composition: W234 R4 complete
short-MIDI instrument input with the MidiSine native32/64 reference, W241 raw
short MIDI for main and multibus effects with both auxiliary directions and one
AUDIO admission, and W249 opt-in prepared host meter taps with a separate Gain
meter reference. Every packet, promotion source and evidence guard was verified
first; no blocking review finding remained. Seventy-two three-way merges over 59
paths were all fast-forwards, R4 equals R3 on every W241 overlap, W249's six
additive overlaps apply on W241, and Cargo manifests and lockfile are unchanged.
Fifty-nine code/doc paths were adopted after base and staged hash verification;
the three shared documentation files were verified as pure additions to the
post-W240 public versions, and stale "adoption pending" status wording was
corrected.

The stage passed all 86 commands: 1,509 all-feature workspace tests, 61
doctests, warning-denied Clippy and Rustdoc, formatting, documentation, 62
script tests (16 AAX packaging) and four REAPER tests, whitespace, and AAX
pipelines rebuilt fresh from the staged sources with CTest in normal and
fast-math profiles (Gain native32 32/32, native64 48/48, editor32 35/35, stock32
14/14; Performance, MidiSine, raw routing and Gain meters 32/64), local notices,
ad-hoc signing with strict verification, bundle checks and registered-module
contracts. Packet target/aax-chain-adoption.md.

### Combined public acceptance after W240 and the AAX chain

All fourteen combined commands pass on a fresh cache: 1,429 all-feature runtime
tests and 104 doctests (zero failed, four ignored), 63 default doctests,
warning-denied Clippy and Rustdoc, formatting, documentation, Python and
whitespace checks, the AU host driver and ravet-native-dsp thumbv6m/x86_64
checks, with no bound source change. Receipt
target/combined-4-w240-aax-gates.log, exit 0. Evidence is local macOS arm64;
no Pro Tools, ACF host chain, PACE or production signing, DAW, other-platform or
deadline claim follows. See [decision0247](decisions/0247-aax-short-midi-input.md),
[decision0254](decisions/0254-aax-raw-effects-auxiliary.md) and
[decision0262](decisions/0262-aax-native-metering.md).

### W246 R2 explicit PostScript outlines public adoption

Root review of frozen W246 reproduced a panic: RAVET and read-fonts 0.45.0
tokenized a large-exponent CFF real differently, and unchecked operand-group
indexing let a crafted 26-byte top DICT panic `outline_format`/`outline_glyph`
through a public `Result` API. The original packet and reproduction remain
preserved. R2 refuses every number spelling read-fonts would truncate, requires
both decoders to agree on operator count, per-operator operand count, integer/real
kind and integer values before either result is used, and replaces every
unchecked DICT/FDSelect/INDEX index, slice and offset addition with refusal.
Existing tests are unchanged; new regression tests cover the crafted DICT, a Font
DICT variant, exponent boundaries, leading-zero exponents, truncation and operand
mismatch, every spelling up to five nibbles, 200,000 seeded long exponents,
40,000 DICT mutations and 20,000 font-level mutations per seed font with zero
panics (the same loop finds ten panics on the frozen source). An independent
Sonnet 5.5 static review found no blocking or major defect and cleared adoption;
absolute panic-freedom wording was softened because complete absence also depends
on read-fonts/skrifa, and its minor test and measurement follow-ups remain open.

Seventy-eight public paths were adopted after base and staged hash verification.
Cargo.lock adds only skrifa 0.48.0 (MIT OR Apache-2.0, recorded in LICENSING).
Two inherited failures were corrected during staging without changing assertions:
text-only Clippy (feature-gated unused test items) and an unmapped formatting
divergence (public `tests/svg_image.rs` retained). The stage passed every public
gate: 1,572 all-feature workspace tests (zero failed, four ignored), 63
doctests, warning-denied Clippy and Rustdoc, formatting, documentation, 62 script
and four REAPER tests, whitespace, and text-only ravet-surface Clippy/tests. The
24-command native script passed (feature on/off loaders, installed system-font
comparison with 40 glyphs and maximum 1.14e-13 font units, ten authored
HarfBuzz/FreeType/CoreText profiles, owned windows, CFF2 window and 33+33 WebKit
groups), as did 63 headless Chromium groups with trusted CFF2 weight selection.
Packet target/w246-adoption.md. Per-glyph metadata re-admission and backend CFF2
blend costs are unmeasured; leading-zero exponents are conservatively refused.
No Unicode conformance, native accessibility or deadline claim follows. See
[decision0259](decisions/0259-explicit-postscript-outlines.md).

### Combined public acceptance after W246 R2

With the skrifa lock addition, all fourteen combined commands pass on a fresh
cache: 1,468 all-feature runtime tests and 104 doctests (zero failed, four
ignored), 63 default doctests, warning-denied Clippy and Rustdoc, formatting,
documentation, Python and whitespace checks, the AU host driver and
ravet-native-dsp thumbv6m/x86_64 checks, with no bound source change. Receipt
target/combined-5-w246-gates.log, exit 0.

### W260 first Linux build and test evidence

The current public tree (1,600-file snapshot taken before W246 adoption, with
281 vendored locked crates shipped offline and hash-verified) was built and
tested on the owner's Arch Linux machine (kernel 7.2.3, x86_64, 20 cores). The
primary matrix uses rustc 1.99.0 installed user-locally through rustup at the
owner's request; the system rustc 1.98.1 gave identical results as a secondary
record. All 37 workspace members build with default and all features; `cargo
test --workspace` passes 1,219 tests and the all-feature run passes 1,492 (zero
failed, four ignored external FFmpeg/Pillow oracles). Default-feature
warning-denied Clippy, 61/101 doctests, warning-denied Rustdoc, formatting,
documentation checks and 62 Python tests pass. Per-crate default and all-feature
test runs (74) all exit 0; ravet-media, ravet-metal, ravet-core-midi and
ravet-editor-smoke compile as documented macOS-only stubs. The Linux CLAP bundle
path produced Gain, Sine and Sidechain bundles, and the offline in-process Gain
smoke host passed. Receipts target/w260-linux-validation.md and target/w260/logs.

All-feature Clippy found a missing cfg gate in the SVG workspace tutorial model
(dead without macOS `live-ui`, also failing on macOS without that feature);
the fix is adopted, and macOS all-feature/default Clippy and the 40 tutorial
tests pass with it. A WebKitGTK 2.52.6 deprecation replacement remains a
compiled-only proposal (target/w260/proposed-fixes). CMake was unavailable, so
LV2 bundling, the Linux C++ editor library/GTK tests and native fixtures were not
run (the two Linux editor sources compiled to objects only). The macOS
`native_semantics` tutorial binary still requires `live-ui` to build. This is
build/test evidence on one machine: no audio device, DAW, plug-in host, GUI,
other-distribution or real-time deadline evidence, and no Linux support claim.

### W259 impulse-response preparation and live convolution transitions public adoption

Source-rate impulse-response preparation is adopted in ravet-signal (no runtime
dependencies) with explicit integer-rate sinc conversion, channel selection,
threshold trimming with recorded offsets and none/peak/energy normalization with
reported gain, all bounded before allocation with whole-call refusal; file
preparation uses ravet-audio-file's existing default-disabled
`resampled-playback` feature. Live engine/bank transitions are in ravet-graph
behind the new default-disabled `convolution-transitions` feature, reusing its
bounded handoff and retirement pattern: background preparation, allocation-free
AUDIO handoff, explicit equal-gain or equal-power crossfade with both engines
running, non-audio retirement and explicit Refuse, QueueOne or LatestWins
mid-fade policy. Cargo.lock is unchanged.

An independent Sonnet 5.5 review found no blocker but required changes. R3 makes
LatestWins start the newest proposal, refuses rather than later starting a
backpressured Refuse proposal, computes energy normalization with exact
power-of-two scaling (huge finite samples are no longer zeroed and subnormal
responses no longer refused as silent; unrepresentable gains refuse), removes
every panic path from AUDIO and documents exactly which thread frees what at
shutdown in either drop order. Regression tests for the three behaviour defects
fail on the frozen R1 source and pass on R3. Analytic resampling error is at most
1.56e-5 of peak with 64 taps (96→48 kHz) and 1.05e-6 with 256 taps (192→48 kHz);
trim, normalize and mapping are bitwise exact; crossfades match an independent
blend bit-exactly across laws, precisions, partitions and block sizes; AUDIO-side
allocation and free counts are zero through handoff, fade, retirement,
backpressure and post-control shutdown.

Eighteen public paths were adopted after base and staged hash verification. The
stage passed all 33 public commands: 1,603 all-feature workspace tests (zero
failed, four ignored), 64 doctests, warning-denied Clippy and Rustdoc,
formatting, documentation, 62 script and four REAPER tests, whitespace, 29
release W259 tests, three release stress runs and release allocation/shutdown
tests. Packet target/w259-adoption.md; superseded pre-review receipts and the R1
failure are preserved. New engines start without history (a level dip for
responses longer than the fade), differing latencies are reported but not
aligned, and release stress rarely reached mid-fade offers, so those paths rely
on deterministic tests. No real-time deadline claim follows. See
[decision0272](decisions/0272-impulse-response-preparation-and-live-convolution-transitions.md).

### W258 hosted VST3 editor ownership public adoption

A separate native editor table and the default-disabled ravet-host `vst3-editor`
feature own macOS NSView IPlugView editors created on MAIN from the edit
controller. Host sizes are bounded to 1..16384 and pass the plug-in's resize,
`checkSizeConstraint` (only `kResultTrue` accepts) and `onSize` sequence with no
unconstrained fallback; the applied size is re-queried with `getSize`.
Plug-in-initiated resizes are refused while detached or during other host calls
into the view, deferred during attach or scale change and then applied or
counted, never acknowledged and silently dropped. A live editor blocks instance
destroy and module close, so the view is released before the controller,
component and module. The Rust `Editor` guard exclusively borrows its `Instance`;
attachment is `unsafe` because the caller's window must outlive it; no native
pointer escapes.

An independent Sonnet 5.5 review found no blocking memory-safety defect but
required changes. R3 reports the true attached state on every native reply,
including refusals; records the exclusive-borrow limitation in ADR 0271 (Rust
scoped audio, prepare and state calls need the editor closed, and concurrent
editor plus AUDIO is shown only through the raw native table); narrows the
compile-fail doctest claims to compiling controls; makes refused drops observable
and deterministic; clears the resize mailbox on detach, teardown and successful
host resize; and terminates the process if an unretired owner is destroyed from
inside its own callback (fork-tested). The fixtures also found and closed a
nested-call gap during plug-in-initiated resize.

Twenty-three public paths were adopted after base and staged hash verification.
The stage passed the full public gate set (all-feature tests, 64 doctests,
warning-denied Clippy and Rustdoc, formatting, documentation, 62 script and four
REAPER tests, whitespace) and VST3 CMake/CTest 67/67 including six editor tests.
The real-window test built RAVET's Gain editor and ran nine cycles across three
modules: three attached the real editor and six correctly reported no editor,
with 857 audio blocks processed on a worker while the editor was open and
instance/module retirement refused during it. Packet target/w258-adoption.md;
earlier R1/R2/pre-R3 receipts are preserved. Graph-bank child editors, editor
edit gestures, a real-window Rust API test and a manual visible probe remain
open. Local macOS arm64 only; no DAW, third-party editor, rendering, keyboard,
focus or accessibility claim. See
[decision0271](decisions/0271-hosted-vst3-editor-ownership.md).

### Combined public acceptance after W258, W259 and the W260 tutorial fix

All fourteen combined commands pass on a fresh cache: 1,511 all-feature runtime
tests and 111 doctests (zero failed, four ignored), 64 default doctests,
warning-denied Clippy and Rustdoc, formatting, documentation, Python and
whitespace checks, the AU host driver and ravet-native-dsp thumbv6m/x86_64
checks, with no bound source change. Receipt
target/combined-6-w258-w259-gates.log, exit 0.

### W264 convolution transition priming and latency alignment public adoption

Live convolution transitions gain an opt-in `TransitionContinuity` with two
explicit policies. `PrimingPolicy::History` feeds the incoming engine recent
input from a history ring prepared at construction, bounded per block, delaying
the fade within a documented bound when the history does not fit one block.
`AlignmentPolicy::DelayLower` delays the lower-latency path through prepared
delay lines during the fade, reports output-latency changes (rise at fade start,
fall at completion) and refuses transitions whose difference exceeds a configured
maximum. Delay lines track their valid length, so take and reset are constant
time. The default constructor delegates with continuity disabled; its W259
behaviour is guarded by the 15 byte-identical W259 transition tests and by
digests recorded from the public W259 build over 108 scenarios (both profiles,
macOS arm64 only because platform `sin`/`cos` differ between profiles).

An independent Sonnet 5.5 review found no blocking or major defect; R2 corrected
the default-behaviour overclaim, aligned status wording, documented zero-fill
during unprimed delayed fades, latency changes on cut and reset, added
checked delay-line sizing and three further tests (LatestWins during priming,
short history, reset during aligned fades). A 4,096-tap response with a
256-frame fade dips 7.50 dB unprimed (7.44 dB predicted) and 0 dB primed; primed
output matches a never-switched engine bit-exactly at partition-aligned history
and within 3.3e-16 (f64) and 1.75e-7 (f32) of peak otherwise; aligned fades have
zero error against an independent reference; all 148 measured impulse onsets
match the reported latency; AUDIO-side allocation is zero.

Nine public paths were adopted after base and staged hash verification. The
stage passed all 24 public commands: 1,640 all-feature workspace tests (zero
failed, four ignored), 64 doctests, warning-denied Clippy and Rustdoc,
formatting, documentation, 62 script and four REAPER tests, whitespace, 32
release transition tests and three release stress/allocation runs. One
intermediate run was discarded after a scratch mutant shared its Cargo target
directory; the disk-interrupted and contaminated receipts are preserved and
marked. Packet target/w264-adoption.md. No real-time deadline claim follows. See
[decision0275](decisions/0275-convolution-transition-priming-and-latency-alignment.md).

### W261 framework events in hosted VST3 Graph nodes public adoption

Hosted VST3 Graph children can opt into translation of ordinary RAVET framework
events through `Vst3GraphBankBuilder::translate(child, FrameworkTranslation)`;
untranslated children still refuse framework events. Declared parameter routes
(normalized or declared linear scale, no implied ID equality) produce one point
per value at its exact sample offset; notes use a selected event bus and an
explicit note-ID policy, with note expression and poly pressure only where
declared; transport becomes a ProcessContext with flags exactly for the fields
present. Mixed framework and rich-lane blocks follow explicit Refuse,
FrameworkFirst or LaneFirst order; two contexts in one block refuse. Routes and
the note bus are validated against the plug-in catalogue before any child starts;
storage is prepared on MAIN with bounded per-point admission and no AUDIO
allocation.

An independent Sonnet 5.5 review found no blocker; its major finding was that a
translation refusal returned Fault, which the Graph latches and hosted nodes can
only clear by rebuild. R4 returns the existing `ProcessStatus::Silence` for a
refused block (the device-underrun precedent), so the Graph silences that node
and continues; distinct refusal reasons are exposed through per-child counters,
a bounded 16-record ring, diagnostic bits and refused lane batches. Native
faults, admission refusals and events to untranslated nodes still fault. Mock and
actual-Gain Graph tests drive refusals through `Graph::process` and confirm
continuation. R4 also adds distinct refusal and build-error variants, removes
AUDIO-path `expect`/`unreachable!`, bounds point admission and adds an opt-in
same-offset last-wins policy.

Fifteen public paths were adopted after base and staged hash verification. The
stage, built on W264, passed the public gates (1,654 all-feature tests, 64
doctests, warning-denied Clippy and Rustdoc, formatting, documentation, 62 script
and four REAPER tests) and VST3 CTest 67/67; the isolated R4 actual-module
consumer passed in debug and release (Gain parameter change exactly at sample 11;
Performance notes, expressions, pressure, Level and transport within 1.1e-10 f32
and 7.5e-18 f64 of a closed-form voice; 10,000 translated Graph blocks with zero
Rust AUDIO allocations). Pressure and transport reach the actual modules but are
wire-checked only by the mock. Packet target/w261-adoption.md; superseded R1–R3
receipts are preserved. Local macOS arm64 only; no DAW, third-party plug-in or
deadline claim. See [decision0273](decisions/0273-hosted-vst3-graph-framework-events.md).

### W262 RAVET reference VST3 modules public adoption

Two RAVET-owned test reference VST3 modules, built from a new portable crate
`ravet-vst3-reference-native`, let the hosted-VST3 stack be tested against real
loaded modules for paths previously covered only by mocks and authored fixtures.
Keyed Sidechain takes stereo main plus a mono or stereo sidechain (5.1 refused)
and outputs main × key, honouring input silence flags. Event Probe turns each of
the nine SDK input event kinds and parameter points into an exact-position pulse
encoding the payload, echoes events and reports a count and digest. Bundles are
ad-hoc signed inside build directories only; the plug-in user path is pinned
empty and a per-bundle no-install CTest guards it. They are test references, not
public plug-ins.

An independent Sonnet 5.5 review found no blocker but required robustness, ABI
and allocation evidence. R4 adds a direct SDK harness feeding malformed and
over-capacity blocks to each loaded bundle (149 Event Probe and 73 Keyed
Sidechain calls per precision: oversized event lists, bad offsets, non-zero
buses, unknown kinds, null pointers, empty, parameter-only and zero-frame blocks)
with silent refusal and unchanged digests; it exposed and fixed three module
defects (unknown kind with bad offset refusing the block, null buffer
dereference, setActive before initialize). Direct tests now cover every exported
C ABI entry point, rejecting overlapping outputs and validating frames before
slices are formed. A test-only counting allocator measures zero Rust heap
operations inside both loaded modules around every process call, valid or
refused, and the ABI tests wrap process/reset in allocation-counter. Same-offset
code collisions, the count CC meaning and the harness-only fast profile are
documented.

Twenty-six public paths were adopted after base and staged hash verification;
Cargo.lock adds only the new package. The stage, built on W261 and W264, passed
the public gates (1,662 all-feature tests, warning-denied Clippy and Rustdoc,
formatting, documentation, Python, AU host and ravet-native-dsp checks), native
CTest 71/71 and reference CTest 6/6 (SDK validator and no-install guard per
bundle). Packet target/w262-adoption.md; superseded r1–r3 receipts and the
disk-interrupted step are preserved. Allocation counts exclude allocations inside
the SDK and C++ module code outside the counters. Local macOS arm64 only; no DAW,
third-party host or deadline claim. See
[decision0274](decisions/0274-reference-vst3-modules.md).

### Combined public acceptance after W261, W262 and W264

All fourteen combined commands pass on a fresh cache: 1,550 all-feature runtime
tests and 112 doctests (zero failed, four ignored), 64 default doctests,
warning-denied Clippy and Rustdoc, formatting, documentation, Python and
whitespace checks, the AU host driver and ravet-native-dsp thumbv6m/x86_64
checks, with no bound source change. Receipt
target/combined-7-w261-w262-w264-gates.log, exit 0. The tree included an
unrelated uncommitted owner edit to scripts/docs/hosted/ravet_tutorials.py.

### W263 Graph-bank child editors and controller edit gestures public adoption

Added opt-in controller edit gestures and MAIN-thread editors for hosted VST3 Graph-bank children
([guide](vst3-hosting.md#graph-bank-child-editors-and-edit-gestures),
[decision 0276](decisions/0276-graph-bank-editors-and-edit-gestures.md), adopted at R3 after two independent Sonnet 5.5 reviews).

- The additive `RavetVst3EditGestureProviderV1` captures `beginEdit`, `performEdit` and `endEdit` on MAIN into a
  bounded 256-record ring with id, value and thread validation.
- Begin/end pairing is preserved: each of up to 64 open gestures reserves its end slot, and duplicate or orphan
  calls are refused and counted. Closing the editor records a flagged synthesised end for each open gesture.
- `restartComponent` accepts only parameter values, titles and latency changes, and only records them.
- `pump_edits` moves edits only while the AUDIO ring (up to 256 values) and the host queue both have room, and
  refuses once the node has returned.
- Values reach the processor at offset 0 of the next successfully dispatched block. An edit replaces a same-id
  block point at offset 0 (counted), so continuous automation cannot mask edits. Values stay pending across refused
  blocks and failed dispatches.
- `drain_edits` publishes ordered, balanced events with framework identity.
- `Vst3GraphBankControl::open_editor` keeps a child's editor open while the bank processes. Bank reclamation is
  deferred while any editor is open.

Concurrent editor and AUDIO rely on the plug-in's own thread safety, as VST3 permits. Only RAVET's split-component
Gain was exercised.

Local macOS arm64 qualification, isolated and on the stage (public `d2b1c74`):

- formatting
- strict Clippy (default, all features, and `ravet-host` with `vst3-hosted-audio`, `vst3-editor` and
  `vst3-catalogue` alone)
- workspace tests with all features (1665 passed) and default doctests (64)
- Rustdoc, default and all features, with warnings denied
- native CMake/CTest: 75 of 75, including the edit-gesture ABI and provider fixtures, the reference modules and the
  Graph-bank editor window test
- the documentation checker and 62 checker tests; the stage also ran 4 REAPER check tests, the AU host check and the
  whitespace check

ThreadSanitizer reported no warnings for the native edit-gesture, editor and hosted-audio provider fixtures and the
existing editor window test. The Graph-bank window test cannot be linked under TSan (ld64 personality limit). Only
RAVET C++ is instrumented.

The window test attaches the RAVET Gain VST3 editor (`RAVET_GAIN_EDITOR=ON`) to a Graph-bank child in a hidden
offscreen `NSWindow`, never ordered front, while two Gain children process on an AUDIO worker. Edits are injected
into the editor's script-message bridge. Results:

- An unpumped edit leaves the audio unchanged.
- The first block after `pump_edits` is bit-equal to a framework-fed reference.
- Teardown was clean three times (612 lockstep and 1471 free-running blocks, 54 edits).
- Zero Rust heap operations on AUDIO.

No DAW, third-party or single-component plug-in, WebKit JavaScript or input-event, visual, accessibility, deadline or
race-freedom claim follows.

The first review found no blocker but required lossless offset-0 delivery, preserved
begin/end pairing and accurate thread-safety wording; the R2 re-review found a merge
defect where a phantom id 0 could stall edits at the 64-id limit, fixed in R3 with a
regression test shown to fail on R2. Twenty-four public paths were adopted after base
and staged hash verification. Packet target/w263-adoption.md; superseded R1 and R2
receipts are preserved.

### Combined public acceptance after W263

All fourteen combined commands pass on a fresh cache: 1,551 all-feature runtime
tests and 114 doctests (zero failed, four ignored), 64 default doctests,
warning-denied Clippy and Rustdoc, formatting, documentation, Python and
whitespace checks, the AU host driver and ravet-native-dsp thumbv6m/x86_64
checks, with no bound source change. Receipt target/combined-8-w263-gates.log,
exit 0.

### W265 Linux x86_64 qualification on native Omarchy

Public commit 3ddaf94 (1,710 files, archive manifest hash-verified) was built and tested on
the owner's Omarchy machine (Arch, kernel 7.2.3, x86_64, rustc 1.99.0, GCC 16.2.1, CMake
4.4.3, WebKitGTK 2.52.6 under Hyprland 0.56.2/XWayland at output scale 1.6). Formatting,
strict default/all-feature/ravet-host VST3 single-feature Clippy, doctests (62), warnings-denied
Rustdoc, ravet-native-dsp thumbv6m/x86_64 checks, documentation and Python checks (62+4) pass.
Non-incremental test builds failed to link three ravet-host test binaries that each linked two
example CLAP crates exporting `clap_entry`; such a link depends on codegen-unit partitioning
and is also latent on macOS. The sine-dependent checks now live in their own test binary, after
which all-feature tests pass on Linux (1,619 passed, 4 ignored; macOS 1,667 including 48
macOS-gated tests) as do default tests (1,263). The runtime-ffi facade links and runs with
g++ C++17/C++23 and clang++ C++17. The Linux web editor CTest passes 4/4 with software
painting after the paint fixture accepts compositor rounding (320x179 at scale 1.6) while still
requiring the view to fill its host. Accelerated painting still fails on the NVIDIA RTX 2060.
GCC 16's default C++20 exposed a char8_t conversion error in the Foundry Linux launcher; it now
builds, and its sources compile as C++17 and C++20. LV2 gain/performance contracts pass 7/7
against system LV2 headers. The VST3 host and reference CMake projects remain macOS-only by
guard. No DAW, GUI launch of Foundry, GTK chooser, audio device, real-time deadline,
other-distribution or Linux support claim follows from this single-machine evidence.
Receipt target/w265-linux-validation.md.

Ten public paths were adopted after base and staged hash verification; the macOS
adoption stage passed all eighteen public gates with the fixes applied.

### Stream notification classification (2026-10-13)

Completed the user-reported startup-xrun fault/rebuild fix on
`fix/xrun-classification`. All seven CPAL error callbacks now use shared kind-based
classification: `Xrun` increments `xruns`, `DeviceChanged` increments
`route_changes`, and every other kind increments `backend_errors`. Both public
input/output statistics expose the new wrapping relaxed counters, including nested
stream snapshots and retirement receipts. Recovery continues checking backend
errors and existing panics, preserving stream generations after notifications.
Graph/file/recording adapters inherit their opener's classification. Workspace
statistics consumers, including tutorials, probes and `ravet-webview`, were
reviewed; no additional exhaustive constructors/patterns needed changes. Guide,
Rustdoc, feature registry/catalogue and
[decision 0278](decisions/0278-stream-error-classification.md) were updated together.

Device-free fixtures cover every current CPAL error kind using constructed errors,
misleading message text, repeated notifications, overflow, heap-free classification,
ordinary/duplex recovery generations and failures/panics, and original-owner
retirement with retained notification counts in both callback directions.

Local macOS 27.0.1 arm64, rustc 1.99.0 validation (all Cargo commands offline and
locked; no real audio devices enumerated or opened):

- `cargo test -p ravet-device --offline --locked`: 91 runtime tests and 3 doctests
  passed, including the four new classification/recovery tests.
- `cargo test --workspace --offline --no-fail-fast` (integrator, outside the
  agent sandbox): all 1,306 tests passed across 327 test binaries, with no
  failures. An agent run inside a network/GPU-restricted sandbox had reported 14
  environment-only failures (localhost sockets, Metal and video decode); none
  reproduced outside it.
- `cargo check --workspace --offline --locked` and
  `cargo clippy --workspace --all-targets --offline --locked -- -D warnings`: passed.
- `cargo test --workspace --doc --offline --locked`: all 64 doctests passed.
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --offline --locked`:
  passed.
- `cargo fmt --all -- --check`, `python3 scripts/check-docs.py --write`,
  `python3 scripts/check-docs.py` and `git diff --check`: passed.
- `python3 -m unittest discover -s scripts/tests -v`: all 68 tests passed.

These results
establish the classification/recovery contract offline, without qualifying real
CoreAudio overload/rerouting, audible output, physical devices, platforms or
callback deadlines.

### 0.1.0 plan and versioned internal dependencies (2026-10-23)

The owner set the 0.1.0 scope: VST3, CLAP and LV2 on macOS, Windows and Linux,
AU on macOS and AAX on macOS and Windows, distributed through an immutable
`v0.1.0` Git tag instead of crates.io. [Decision 0279](decisions/0279-release-0-1-0-scope-and-distribution.md)
records the choice, and the [0.1.0 release plan](releases/0.1.0.md) lists the
criteria. Private consumer products built on 0.0.1 informed the criteria. Their
own validation is not a public support claim.

First criterion completed: all 24 library crates are declared in
`[workspace.dependencies]` with `path` and `version = "0.0.1"`, and members
inherit them. The two `default-features = false` uses of `ravet-foundation` keep
explicit paths with the same version requirement. Dev-dependencies on unpublished
examples and tools stay path-only.

Local macOS arm64 validation: `cargo metadata --locked` passed with
`Cargo.lock` unchanged; a `cargo metadata` scan found no unversioned normal
dependency between library crates; `cargo check --workspace --all-targets --locked`
and `python3 scripts/check-docs.py` passed. Starter and Foundry manifests still
generate absolute paths; that remains an open criterion.

### Umbrella crate features (2026-10-23)

The `ravet` crate now depends optionally on every component crate except
`ravet-embedded`. Each has a feature named after the crate without its `ravet-`
prefix and a module re-export (`ravet::midi_device`). Component features pass
through under their original names: audio-file and device codecs, convolution
transitions, AU/VST3 hosting, webview fonts and surface image/text. Foundation,
signal and surface remain unconditional, so existing umbrella users are
unaffected. `Cargo.lock` gained the umbrella's new optional dependency edges only;
no external package versions changed. Getting started documents single-source
dependencies and local `[patch]` redirection.

Local macOS arm64 validation: `cargo test -p ravet` with default features and
with `--all-features` passed (`crates/ravet/tests/features.rs` imports every
re-export); `cargo clippy -p ravet --all-features --all-targets -- -D warnings`,
`RUSTDOCFLAGS="-D warnings" cargo doc -p ravet --all-features --no-deps`,
`cargo fmt -p ravet -- --check` and `cargo check --workspace --all-targets` passed,
all offline and locked after the lockfile update. Linux and Windows all-features
builds are not yet checked.

### Single-source starter and Foundry dependencies (2026-10-23)

Foundry projects gain `framework_source` (`path` default, or `tag`), exposed in
`ravet-project.toml`, the **RAVET dependencies** selector and
`new --framework-source`. `scripts/new-project.py` gains `--ravet-source`. In
tag mode every RAVET Cargo dependency, including reference example packages, uses
`git = "https://github.com/mo0kid/ravet"` with the tag `v<workspace version>`;
path mode keeps the previous absolute paths. Settings without the field load as
`path`. Starters keep depending on the public reference packages until the
generic export layer replaces them.

Two limits found while probing: a `[patch]` redirect of a Git dependency still
fetches the original tag (it failed offline and for a missing tag), so getting
started now recommends replacing the source with a path instead; and the
repository is private, so anonymous HTTPS fetches fail without credentials.

Local macOS arm64 validation: `cargo test -p ravet-foundry --offline --locked`
passed 12 integration tests including `every_ravet_dependency_shares_one_source`
(both sources × offline, standalone, effect and instrument plug-ins);
`python3 -m unittest scripts.tests.test_projects` passed 4 tests including the
tag-source test. Tag-mode projects were not built: no `v0.1.0` tag exists yet.

### Minimum supported Rust version 1.88 (2026-10-23)

The workspace declares `rust-version = "1.88"` and all 38 packages inherit it.
The floor comes from locked dependencies (`encoding_rs` 0.8.42 requires 1.88;
the 2024 edition needs 1.85). RAVET code used one newer API: `AtomicU64::try_update`,
which is unstable in 1.88 while `fetch_update` is deprecated on current stable. The
two uses (`ravet-surface` editing owner claims and the gain CLAP restore) now use
explicit `compare_exchange_weak` loops with the same orderings and results. The
omarchy Linux CI job installs 1.88.0 and checks every target with all features.

Local macOS arm64 validation: `cargo +1.88.0 check --workspace --all-targets`
passed with default and with all features (offline, locked, separate target
directory); on rustc 1.99.0, clippy `-D warnings` and tests for `ravet-surface`
and `ravet-gain-clap` passed. The workspace test suite was not run on 1.88, and
the new CI step has not run yet.

### Opt-in ravet-data cryptography (2026-10-23)

`ravet-data` now has no default features. `crypto` enables the `crypto` module
(sha2, num-bigint) and `entitlement` enables signed local entitlements
(ed25519-dalek); XML depends only on quick-xml. This is a breaking change for
direct users of those modules, who must enable the features. The umbrella crate
passes both through, and Foundry enables `crypto` for its own hashing. The
entitlement integration test requires its feature; the SHA-256/integer test is
gated on `crypto`.

Local macOS arm64 validation: `cargo test -p ravet-data` passed with no features
(2 XML tests) and with all features (3 data and 3 entitlement tests);
`cargo tree -p ravet-data -e normal --depth 1` lists only quick-xml by default.

Combined check after the 2026-10-23 dependency, MSRV and feature changes:
`cargo test --workspace --all-targets --offline --locked --no-fail-fast` (default
features) passed 1,241 tests with no failures; workspace clippy with all features
and `-D warnings`, and `cargo +1.88.0 check --workspace --all-targets --all-features`
passed.

### Public repository and package metadata (2026-10-23)

The owner made `mo0kid/ravet` public. A disposable consumer outside the checkout
depending on `ravet = { git = "https://github.com/mo0kid/ravet", tag = "v0.0.1" }`
resolved its lockfile with interactive Git prompts disabled and no credentials.
Starter, Foundry and getting-started guidance no longer describes credentials.

All 38 packages inherit `repository`, `homepage` (`https://djwaudio.co/ravet/`),
`documentation` (`https://djwaudio.co/ravet/docs/`), `keywords` and the
`multimedia::audio` category from the workspace. All except the two examples with
their own README inherit the root `README.md`. Packages stay `publish = false`.
`cargo package -p ravet-foundation --list` includes the README, and
`cargo check --workspace --locked` passed.

### Versioning and compatibility policy accepted (2026-10-23)

The owner accepted [versioning and compatibility](compatibility.md), recorded as
[decision 0280](decisions/0280-versioning-and-compatibility.md). It covers shared
versions and immutable tags, minor/patch break rules, status-based deprecation,
versioned saved state, caller-keyed parameter IDs and stable plug-in identities.
A new release-plan item tracks bringing existing state formats and parameter
numbering into line. Documentation only; no code changed.

### Generic export layer design accepted (2026-10-23)

The owner accepted [decision 0281](decisions/0281-generic-export-layer.md):
native AUv2 and AAX adapters, a RAVET-defined editor protocol with a product
channel, and DW-303 as the pilot and open-source example product, kept in its
own repository. Design only; nothing is implemented. A preliminary read-only
survey found the processor contract, native tables and CLAP/LV2/AUv3 helpers
already generic, and identity, VST3 state, bus setup, editor codecs, Turtle,
AAX descriptors, bundle scripts and Foundry bindings tied to the gain and sine
references.

### Export layer step 1a/1b: ravet-plugin product description and state (2026-10-23)

New prototype crate `ravet-plugin` (umbrella feature `plugin`), the first part of
[decision 0281](decisions/0281-generic-export-layer.md):

- `Product` and `Parameter` const builders: category, layouts, note ports,
  precision, keyed parameters with explicit or FNV-1a-derived permanent IDs,
  a single bypass toggle, and read-only/hidden/non-automatable flags.
  `Product::resolve` validates all of it.
- `Identities`: CLAP, VST3, AU, AAX and LV2 identities derived exactly as Foundry
  derives them, with `IdentityOverrides` for earlier products.
- `state`: the `RAVETSTA` envelope with migration and foreign-import hooks,
  matching by ID, no clamping and no partial application.

Local macOS arm64 validation: `cargo test -p ravet-plugin` passed 5 product
tests, 5 state tests and the crate doctest. Identity and parameter-ID vectors
were computed independently in Python from Foundry's algorithm. Byte layout is
pinned by a golden test. Clippy with `-D warnings`, rustdoc with `-D warnings`,
`cargo fmt`, `cargo +1.88.0 check` and the umbrella all-features test passed. No
format adapter uses the crate yet.

### Export layer step 1c: editor protocol (2026-10-23)

`ravet_plugin::editor` adds version-1 JSON editor messages addressed by parameter
key: `ready`, `begin`, `set`, `end`, `commit` and `product` from the editor;
`info`, `snapshot`, `changes` and `product` to it. It also adds `Gestures`
tracking for balanced, multi-parameter gestures and closing them all when an
editor closes. The crate gains a pinned `serde_json` 1.0.151 dependency, already
in the lockfile. No web view transport or JavaScript client exists yet.

Local macOS arm64 validation: `cargo test -p ravet-plugin` passed 13 tests
(3 new editor tests) and the doctest; clippy and rustdoc with `-D warnings`,
`cargo fmt` and `cargo +1.88.0 check -p ravet-plugin --all-targets` passed.

### Export layer step 1d: generic CLAP export (2026-10-23)

`ravet-plugin` gains `PluginExport` (a marker naming any `Processor<f32>` and
its `Product`), `ParameterCache`, and a `clap` feature with `export_plugin!`.
The generic CLAP plug-in derives its descriptor, audio ports, note input,
parameter metadata and text, versioned state with product hooks, latency
(restart on change) and tail from the product, and runs the processor inside
`DenormalProcessor`. Host parameter values are snapped and cached; main-thread
flushes update the cache immediately. `examples/gain-plugin` exports the
unchanged gain reference, and `scripts/bundle-clap.py --plugin generic` bundles
it. The umbrella crate gains `plugin-clap`.

Local macOS arm64 validation: clap-validator (0.3.2-127) on the release bundle
ran 44 tests, 33 passed, 0 failed, 11 skipped. The skips are f64 audio,
audio-port configurations, preset discovery and note-port tests that do not
apply to an effect (`target/gain-generic-clap-validator.json`). `cargo test`
passed for `ravet-plugin` (all features), `ravet-gain-plugin` and `ravet`;
clippy `-D warnings`, `cargo fmt` and `cargo +1.88.0 check` passed. No DAW, Linux
or Windows run yet.

### Export layer step 4b: editor for the generic CLAP export (2026-10-23)

Products gain `Product::with_editor`. RAVET injects `crates/ravet-plugin/editor/client.js`
into the page, exposing `window.ravetPlugin` over editor protocol 1. On macOS
the generic CLAP plug-in registers the GUI extension. Editor edits update the
parameter cache at once and pass through `EditQueue` (256 edits, lock-free
SPSC, all-or-nothing pushes). The audio or inactive flush delivers them as
gesture-begin/value/end host events and as processor values at block start.
Closing the editor ends open gestures. `PluginExport::editor_message` answers
product-channel messages. `examples/gain-plugin` has an editor page.
`ravet-plugin` now denies, rather than forbids, unsafe code for its one reviewed
`WebView::attach` call.

Local macOS 27.0.1 arm64 validation:

- `ravet-editor-smoke --generic` on the release bundle passed two cycles of
  create, attach, `info`, complete edit (host saw B/V/E), gesture (B/V/V/E),
  clamped/unknown/unbalanced messages, host change reaching the page, state load
  reaching the page and closing mid-gesture (B/E). It saved a screenshot.
- clap-validator: 33 passed, 0 failed, 11 skipped, unchanged by the GUI extension.
- New tests: queue wraparound and a 20,000-edit cross-thread order test, cache
  generations, page injection, and a Node check of the client
  (`scripts/tests/test_editor_client.py`).
- Workspace clippy with all features, `cargo +1.88.0 check`, rustdoc and tests passed.

Observation, not caused by this change: the existing gain editor probe
(`ravet-editor-smoke "RAVET Gain.clap"`) now times out in its reusable-controls
JavaScript self-test. A clean build of committed HEAD `e1c712c` in a temporary
worktree fails the same way, so it predates this work. The likely cause is the
WebKit in macOS 27.0.1; it is open.

<a id="release-010-delay-example"></a>

### Tempo-synced delay plug-in example (2026-11-05)

New example `examples/delay-plugin` (`ravet-delay-plugin`) replaces gain as the
showcase in the README. `dsp::SyncedDelay` is a format-independent stereo delay
with host-tempo sync (12 divisions, block-start tempo, last-tempo hold,
millisecond fallback), a feedback-path tone low-pass, ping-pong, 50 ms time glide,
5 ms ramps and a 5 ms bypass crossfade. The crate root describes the product with
explicit parameter IDs 1–8, state tag `RVTDLY01` and a web editor. The teaching
delay in `examples/tutorials` is unchanged: its 1,024-frame and 128-event limits
would refuse CLAP activation (1,024 events), and hosted walkthroughs link to its
line numbers. `bundle-clap.py --plugin delay` bundles the plug-in;
`ravet-clap-smoke --delay` is a new delay fixture.

Local macOS 27 arm64 validation:

- 12 offline tests: product/processor consistency, state round trip, repeat
  position, tempo sync and fallback, tempo-change glide, block-size independence
  (1, 64, 517 and 3,000 frames), ping-pong, bypass crossfade, transactional
  fault on invalid values, tail, mono with 20,000-frame blocks, and no heap
  activity across 1,000 blocks with events and tempo changes.
- `ravet-clap-smoke --delay` on the release bundle: parameters, inactive flush,
  a 120 BPM eighth-note repeat at exactly 2,000 samples at 8 kHz on a separate
  audio thread, and state recall.
- clap-validator 0.3.2-127: 33 passed, 0 failed, 11 skipped (preset discovery,
  f64, note-port and port-configuration tests that do not apply). The first run
  failed `process-varying-sample-rates` at 768 kHz and 1,234.57 Hz; the
  processor now accepts any rate up to 768 kHz.
- The editor page was rendered with a stubbed bridge only; it has not been
  opened in a plug-in host.

No DAW, Windows or Linux run.

<a id="release-010-native-table"></a>

### Export layer: native table, automation and bypass policies (2026-11-05)

The Rust side of the generic C table `crates/ravet-plugin/include/RavetPlugin.h`
is implemented in `ravet_plugin::native` (crate feature `native`, umbrella
`plugin-native`). With it, `export_plugin!` also exports `ravet_plugin_v1`;
`examples/delay-plugin` and `examples/gain-plugin` enable it, and the delay's
release cdylib exports both `clap_entry` and `ravet_plugin_v1`. The table
reuses `ravet-native` for buses (prepared scratch through `NativeAudioAdapter`),
events and transport. Host-side changes and editor edits mark parameters in a
lock-free dirty bitset and reach the processor at offset 0 of the next block;
state loads deliver every value in a zero-frame block followed by a reset.

`Product::with_automation` (`SampleAccurate`, `PerBlock`) and
`Product::with_bypass` (`Processor`, `Hard`, `Crossfade`, `Frozen`) are
implemented once in `PolicyProcessor`, which the CLAP export and the table both
run inside `DenormalProcessor` (`FlushBoth`). `Frozen` uses the new
`AudioBuffer::pass_through` in `ravet-foundation`. Parameter text formatting and
parsing moved from `clap.rs` to `ravet_plugin::text`, and the macOS web-editor
host moved from `clap_gui.rs` to `editor_host.rs`; both exports use them.
[Decision 0282](decisions/0282-plugin-policies-and-native-table.md) records the
bypass semantics and the table's fault and delivery rules.

`RavetPlugin.h` changed in comments only: float samples; fault scope (process,
reset and timing return 2 until prepare; other calls keep working);
`get_value` returns NaN for an unknown index; truncated `value_to_text` returns 1;
parameter values need not be exact floats and are clamped, snapped and cached,
unknown IDs and nonfinite values dropped, at most 1,024 processor events per
block; Cocoa-only editor in this release with one-to-four-times resizing;
`editor_idle` does nothing on macOS.

Local macOS 27 arm64 validation:

- `cargo test -p ravet-plugin --all-features`: 40 tests and 1 doctest pass
  (11 policy and text unit tests, 12 native-table tests, 17 existing integration
  tests). Native tests drive every function through the table: product record
  and identities against `ResolvedProduct`, parameter records, text and
  truncation, normalization, set/get, saved state, state load with reset and
  full delivery, migration and import hooks, host automation with offsets and
  inexact doubles, rejected offsets with silence, the hard bypass policy,
  latency change reporting, configuration validation, panics returning 2 and
  recovering through prepare, editor-protocol edits through `take_edits`, no
  heap activity across 500 processed blocks with a pending state load, and a
  C99 `-Wall -Werror` compile of the header (Apple clang 21) whose sizes and
  offsets match every Rust record.
- `cargo test -p ravet-delay-plugin`: 17 tests pass, including 5 new table
  tests (identities, text round trip, saved and loaded state with a 10 ms repeat
  at exactly 80 samples at 8 kHz, a 120 BPM eighth-note repeat at exactly 2,000
  samples through native transport, and no heap activity across 500 blocks).
  `ravet-gain-plugin`: 1 test passes. `ravet-foundation`: 3 new
  `pass_through` tests pass.
- Clippy with warnings denied for `ravet-plugin` (no features, `clap`, `native`
  and all features), `ravet-foundation`, `ravet-delay-plugin` and
  `ravet-gain-plugin`; library Clippy for `ravet-plugin` cross-checked for
  `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-gnu` (compile-only; this
  removed pre-existing dead-code errors in macOS-GUI-only CLAP items);
  `cargo fmt --check`; warning-free Rustdoc for
  `ravet-plugin`; `cargo +1.88.0 check -p ravet-plugin --all-features`; the
  documentation checker.
- CLAP regression after the policy wrapper and shared editor host: the release
  delay bundle passes `ravet-clap-smoke --delay`, and clap-validator
  0.3.2-127 reports 33 passed, 0 failed, 11 skipped.

No adapter or host has loaded the table yet, the editor was not opened through
it in a host, and there is no Windows or Linux run.

<a id="release-010-aax-adapter"></a>

### Generic AAX adapter (2026-11-05)

New `crates/ravet-plugin/aax`: one C++17 adapter written against
`RavetPlugin.h` that builds an `.aaxplugin` for any `export_plugin!` product,
using RAVET's native profile as decision 0281 requires. It reuses
`RavetAaxTransport`, `RavetAaxShortMidiInput` and `RavetNativeMidiOutput`; the
older parameter, processor and editor helpers are bound to the 0.0.1 native
tables and editor ABI, so parameters, state, processing and the editor are
implemented directly on the table. The AAX Library is compiled from the external
SDK's sources; the VST3 SDK is not used. `scripts/bundle-aax.py` gains
`--plugin delay|generic|mock` and `--package <crate>` (driver in
`scripts/aax_plugin.py`); the 0.0.1 references are unchanged and still need
`--vst3-sdk`. Builds are unsigned; PACE wrapping is the vendor's step.

Local macOS 27 arm64 validation, AAX SDK 2.8.0 from a local JUCE checkout
(`ACFGetSDKVersion` reports `0x20801345980`), Apple clang, CMake:

- Mock table (`mock/RavetPluginMock.cpp`, gain, stepped mode, bypass, read-only
  peak, hidden parameter; mono and stereo; MIDI echo): the adapter compiles
  without warnings at `-Wall -Wextra`; `ravet_aax_adapter_tests` passes all ten
  groups (descriptors, parameters, processing, MIDI, latency handshake, editor
  gestures, state, read-only reflection, editor open/idle/close, stereo and
  layout refusal) and `ravet_aax_check` passes on the built
  `RAVET Mock.aaxplugin` (`ctest`: 2/2).
- `examples/delay-plugin` after merging `plugin/native-table` (head `5d03ca4`):
  `python3 scripts/bundle-aax.py --plugin delay --aax-sdk <JUCE>/modules/juce_audio_plugin_client/AAX/SDK`
  builds `target/aax-plugin/delay/RAVET Delay.aaxplugin`; `ravet_aax_check`
  passes for both effects (2x2 and 1x1): eight parameters registered with
  `MasterBypassID` as bypass, text round trips, eight blocks plus an empty block
  rendered, the 96-byte `RvSt` chunk saved, compared and reloaded and a corrupted
  one rejected. With `--editor` the delay's 560x340 web editor opens in a real
  off-screen `NSView` through `AAX_IEffectGUI`, runs ten timer ticks and closes.
- `scripts/tests/test_aax_plugin.py`: 6 tests pass; `test_aax_bundle.py` still
  passes (22 together).

Not available or not run: Avid's AAX Validator (DSH/`aaxval`) is not installed
here. The unsigned bundle was not loaded in Pro Tools (that needs Pro Tools
Developer; nothing was installed into host folders). No Windows build has been
made. No Pro Tools
support is claimed.

### Contributor terms (2026-11-05)

The owner approved the RAVET Contributor Terms (`CONTRIBUTOR_TERMS.md`, since
withdrawn; see the contribution policy entry below),
version 1.0: contributors keep copyright and grant a non-exclusive copyright
and patent licence that includes relicensing under the commercial route; RAVET
commits to keeping contributions available under the GPL. The licensor is David
Whiting, trading as DJW Audio; the contact address is support@djwaudio.co; the
terms are governed by the law of England and Wales. Contributors accept them
with a `RAVET-Contributor-Terms: 1.0` commit trailer. `CONTRIBUTING.md` and
`LICENSING.md` now point to them. The text has not had independent legal review.

### Integration: AAX and LV2 merged, switch text, thread rule (2026-11-05)

`release/0.1.0` now contains the native table and policies, the generic AAX
adapter and the generic LV2 adapter. Conflicts were additive documentation
only. On the merged branch: `ravet-plugin` (all features) and
`ravet-delay-plugin` tests pass, `bundle-aax.py --plugin delay` builds and its
bundle check passes, and `bundle-lv2.py --plugin delay` passes its four CTest
suites with `lv2_validate` reporting 0 errors (macOS arm64, Homebrew LV2
1.18.10). The same LV2 delay build, run on Linux x86_64 (Arch, LV2 headers
v1.18.10, lilv 0.28.0), passes all four CTest suites and `lv2_validate` with 0
errors. No host has loaded either bundle yet.

Unitless stepped 0/1 parameters (such as the delay's Sync and Ping-pong) now
read `On`/`Off` in every format, as the bypass parameter already did.
`RavetPlugin.h` now allows `get_value` and `set_value` from any thread, because
both touch only lock-free shared state; AAX hosts can call parameter updates
off the main thread.

## 5 November 2026: Generic VST3 adapter — independent C++ table validation

Implemented `crates/ravet-plugin/adapters/vst3` against `RavetPlugin.h`, with
runtime product identity/layout/parameter registration, physical timed events,
unchanged state streams, note/MIDI input and output, hidden IMidiMapping lanes,
latency/tail publication and weak editor lifetime/main-loop gesture delivery.
`scripts/bundle-vst3.py` now selects the generic adapter for Gain/Delay or an
external Cargo product; `--legacy-reference` retains the old wrappers.

Local environment: macOS arm64, AppleClang 21.0.0, Xcode 27.1, CMake and external
Steinberg VST3 SDK 3.8.1. SDK discovery used Spotlight, then the complete external
SDK at `/Volumes/PROTOOLS/Dropbox/DEVELOPER/shine2/target/sdks/vst3` read-only.
No SDK files or private product sources were copied into the repository.

Executed evidence:

- Direct CMake Release mock bundle and SDK validator build: PASS. Focused
  CTest: **1 passed, 0 failed**. Tests cover gain/bypass/stepped/hidden/read-only
  metadata, hidden MIDI-ID collision avoidance, declared mono/stereo layouts,
  original-offset automation/audio, note/CC/pitch-bend/channel-aftertouch echo,
  state envelope bytes, malformed-state refusal, omitted-bus zero-frame flush,
  latency change followed by one main-thread restart, editor gesture order,
  resize and retained-view teardown.
- Direct SDK validator: **47 passed, 0 failed**. Initial run was 46/1 because
  hidden MIDI IDs exceeded VST3's signed ID limit; the final implementation uses
  a collision-free range below `kMaxParamId` and the rerun passes.
- Full maintained command `python3 scripts/bundle-vst3.py --mock --sdk <SDK>`:
  PASS, CTest **1/1**, SDK validator **47 passed, 0 failed**, locally ad hoc signed
  `target/vst3/generic/mock/release/VST3/Release/ravet_mock_vst3.vst3`.
- Focused Python bundler regression suite: **9 tests passed, 0 failed**, including
  six new generic-bundler tests and three existing reference-bundler tests.
- `python3 scripts/check-docs.py --write`, `python3 scripts/check-docs.py` and
  `git diff --check`: PASS at this stage.

The mock editor exercises callbacks only; no actual web view or DAW was run.
Windows/Linux commands are documented in `docs/vst3.md` for coordinator execution.
This evidence does not establish their runtime support, f64 processing, arbitrary
speaker identities, SysEx/UMP, real-time deadlines or production signing.
Actual Rust-product integration is recorded separately when executed.

<a id="release-010-vst3-products"></a>

## 5 November 2026: Generic VST3 adapter — actual Rust Gain and Delay integration

Merged `plugin/native-table` through `f4b1082` into `adapter/vst3` to validate the
real `ravet_plugin_v1()` entry. No product-specific C++ or Rust changes were
needed. Both examples enable the native feature and remain Cargo cdylibs.
The adapter loads a bundled product image by module-relative path. Only
`ravet-delay-plugin` and `ravet-gain-plugin` were built with Cargo, not the whole
workspace.

Full maintained Release commands on the same macOS arm64/Xcode/SDK environment:

- `python3 scripts/bundle-vst3.py --plugin delay --sdk <SDK>`: **47 SDK tests
  passed, 0 failed**. Final artifact:
  `target/vst3/generic/delay/release/VST3/Release/ravet_delay_vst3.vst3`.
- `python3 scripts/bundle-vst3.py --plugin gain --sdk <SDK>`: **47 SDK tests
  passed, 0 failed**. Final artifact:
  `target/vst3/generic/gain/release/VST3/Release/ravet_gain_vst3.vst3`.
- Final maintained mock rebuild: CTest **1 passed, 0 failed**; SDK validator
  **47 passed, 0 failed**. All three bundles carry nested product/local ad hoc
  signatures where applicable and copied public notices.
- Focused Python VST3 bundler suite: **9 passed, 0 failed**. Documentation
  regeneration/check and `git diff --check`: PASS.

The first Delay run reported **31 passed, 1 failed** plus 15 setup failures:
output-event capacity was incorrectly requested for a product without note
output. After fixing that, it reported **46 passed, 1 failed** at a sample rate
above Delay's 768 kHz limit. Setup now asks the product to prepare/deactivate on
stopped MAIN before accepting the negotiation, retaining the previous accepted
VST3 setup on rejection. Activation prepares the accepted setup again. The final
run reports **47/0**; the SDK still logs its informational unsupported-rate
message for 1,234,567.8 Hz, which Delay intentionally rejects. These observations
are retained to explain the final negotiation behavior.

No actual product editor was attached and no DAW was run for this generic
adapter. The current native table provides a macOS Cocoa editor only;
Windows/Linux attachment is implemented in the adapter but requires the table's
platform backends. f32/main-bus processing, bounded events/state, no SysEx/UMP,
count-based speaker maps and Linux host-run-loop requirements remain as listed
in `docs/vst3.md`. No change to `RavetPlugin.h` was authored by this adapter task.

Final integration also merges the native-table branch through `21088c5`,
including its contract clarifications and documentation. Fresh maintained mock,
Delay and Gain packaging runs after that merge retain **47/0 each** and mock
CTest **1/0**. `codesign --verify --deep --strict` passes for all three bundles.
The adapter's CMake link dependencies include the product image so a later Cargo
rebuild updates the nested library even when adapter C++ is unchanged.

### Contribution policy: no outside pull requests (2026-11-05)

The owner decided that RAVET does not accept outside pull requests or code
contributions, with no plan to accept them later. This replaces the contributor
terms recorded earlier today, which were withdrawn before use after an
independent AI review found gaps (an unconditional grant to recipients and no
patent sublicensing); no contribution was accepted under them.
`CONTRIBUTOR_TERMS.md` is removed; `CONTRIBUTING.md` and `LICENSING.md` state the
policy, and `.github/workflows/close-pull-requests.yml` comments on and closes
pull requests from authors who are not the owner, members or collaborators. The
job uses `pull_request_target` with only `pull-requests: write` and never checks
out or runs pull-request code. It has not run yet: it takes effect once merged
to `main`.

<a id="release-010-auv2-adapter"></a>

### Generic native AUv2 product adapter (2026-11-05)

Branch `adapter/auv2` implements the native AUBase choice in decision 0281,
with the contracts recorded in decision 0284 (numbered 0283 on its branch; renumbered at merge because LV2 took 0283). It fast-forwarded
`plugin/native-table` through `21088c5` before testing the actual Rust product;
the adapter itself makes no change to `RavetPlugin.h`. Effects, instruments and
audio-backed MIDI effects share `adapters/auv2/PluginAU.mm`. Bundle metadata
comes from the loaded product table, and the component embeds its uniquely named
product dylib rather than VST3. The legacy reference remains explicit through
`--vst3-sdk`.

Environment: macOS 27.0.1 (26A434), arm64, Xcode 27.1 (27A9275), Apple's external
AudioUnitSDK 1.4.0 source checkout. SDK source was compiled in the worktree's
build directory, not copied into the repository. The JUCE location was checked
first but is a flattened SDK snapshot, unsuitable for this build. Only the delay
Cargo package was built, with two jobs; no workspace Cargo build was run.

Executed commands (with `AU_SDK` pointing at that external SDK):

```sh
python3 scripts/bundle-au.py --mock effect --audio-unit-sdk "$AU_SDK" --validate
python3 scripts/bundle-au.py --mock instrument --audio-unit-sdk "$AU_SDK" --validate
python3 scripts/bundle-au.py --mock midi --audio-unit-sdk "$AU_SDK" --validate
CARGO_BUILD_JOBS=2 python3 scripts/bundle-au.py --plugin delay \
  --audio-unit-sdk "$AU_SDK" --validate
```

All returned exit 0. Each mock printed:
`PASS generic AUv2 mock metadata/stereo+mono/state/schedules/timing/transport/MIDI/editor lifetime/MAIN dispatch (3 retained-view cycles)`.
Checks include stable physical IDs/ranges, indexed strings and parsing, invalid
parameter/state rejection, state saved before rendering, complete bypass recall,
sample-offset immediate events, all-or-none schedule rejection, ramp samples and
next-block endpoint, dynamic timing notifications, exact host transport,
short-MIDI loopback for instrument/MIDI effect, mono on a worker thread, and
balanced create/open/close/destroy with zero off-main table control calls.
The Cocoa view survives AU disposal, resize and release; no retired timer calls
occur. Gain has no MIDI input/output, so MIDI loopback applies only to the other
two variants.

The real delay printed:
`PASS generic AUv2 product metadata/render/editor lifetime (3 retained-view cycles)`
and
`PASS real delay AUv2 8 kHz stereo: 120 BPM repeat at sample 2000, cached-state envelope, complete bypass recall, offset-16 automation, tail and backing tails`.
Its actual WebKit views were retained past AU disposal in all three cycles.
The impulse expectation is checked on both channels, state CFData begins with
`RAVETSTA`, automation retains the wet prefix before offset 16 and reaches dry
after product smoothing, and tail changes from infinity to exactly 2.000125 s
without a latency change. This is component evidence, not DAW interaction or
visual/focus/accessibility qualification.

`scripts/validate-au-mock.py` installed only the test identities below in
`~/Library/Audio/Plug-Ins/Components`, then removed every test installation in
`finally`. The final runs of AU Validation Tool 1.10.0 were:

| Exact validator command | Exit | Final result |
|---|---:|---|
| `auval -v aufx RvT1 RvTs` | 0 | `AU VALIDATION SUCCEEDED.` |
| `auval -v aumu RvI1 RvTs` | 0 | `AU VALIDATION SUCCEEDED.` |
| `auval -v aumf RvM1 RvTs` | 0 | `AU VALIDATION SUCCEEDED.` |

Exact local output is retained in
`target/au-native/mock-{effect,instrument,midi}/auval.log`. Initial effect/MIDI
attempts failed discovery with `FATAL ERROR: didn't find the component` before
AudioComponentRegistrar noticed the new directory. The repeatable validator
retries only that discovery failure; it does not retry an actual validation
failure. The real delay identity was never installed globally.

An AddressSanitizer build of the mock effect, adapter, SDK and contract
executable also returned exit 0 with the same PASS line and no ASan diagnostic:

```sh
cmake -S adapters/auv2 -B target/au-native/asan-mock \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_CXX_FLAGS=-fsanitize=address \
  -DCMAKE_OBJCXX_FLAGS=-fsanitize=address \
  -DCMAKE_EXE_LINKER_FLAGS=-fsanitize=address \
  -DCMAKE_MODULE_LINKER_FLAGS=-fsanitize=address \
  -DCMAKE_SHARED_LINKER_FLAGS=-fsanitize=address
ASAN_OPTIONS=detect_leaks=0 python3 scripts/bundle-au.py --mock effect \
  --build-dir target/au-native/asan-mock --audio-unit-sdk "$AU_SDK" --validate
```

Leak detection was disabled; no leak-freedom claim follows. Seven native/legacy
AU Python packaging tests passed. The documentation generator/checker and
`git diff --check` passed. No Rust API documentation or example source was
changed by this adapter work.

Limits: one main native f32 bus, fixed 1,024 queued events, bounded current-block
ramps, serialized scheduling/MIDI producers, and a running host main loop for
MAIN calls and notification delivery. No auxiliary buses, f64, SysEx/MIDI 2,
MusicDevice StartNote/StopNote, factory-program catalogue, zero-audio MIDI
processor, DAW/device, x86_64, sandbox, production signing or callback-deadline
support is established. The existing C table was sufficient; the coordinator
needs no header change.

<a id="release-010-adapter-integration"></a>

### Integration: all four native adapters merged (2026-11-05)

`release/0.1.0` now contains the generic VST3 and AUv2 adapters as well as AAX
and LV2. The AUv2 decision record, numbered 0283 on its branch, was renumbered
0284 because the LV2 record had taken 0283. Conflicts were documentation only.
Checks on the merged branch (macOS 27 arm64):

- `bundle-vst3.py --plugin delay` with the external Steinberg VST3 SDK 3.8.1:
  the SDK validator reports 47 tests passed, 0 failed.
- `bundle-au.py --plugin delay --validate` with the external AudioUnitSDK
  1.4.0: the in-process delay checks pass (120 BPM repeat at sample 2,000,
  state, bypass, offset-16 automation, three retained-view cycles). The delay
  component (`aufx QFKS TYDS`), copied temporarily into
  `~/Library/Audio/Plug-Ins/Components`, passes `auval -v aufx QFKS TYDS`
  (exit 0, `AU VALIDATION SUCCEEDED.`); the copy was then removed.
- AAX and LV2 delay checks as recorded in the previous integration entry.

No DAW has loaded any of these bundles yet, and none has been built on Windows.

<a id="release-010-platform-builds"></a>

## 5 November 2026: Plug-in exports on Linux and Windows

Branch `platform/linux-windows` from `release/0.1.0` (`db4a493`). The RAVET
Delay (`examples/delay-plugin`) was built as CLAP, VST3 and LV2 natively on
Linux x86_64, and as CLAP, VST3, LV2 and AAX for Windows x64 by cross-compiling
on that Linux machine; each format's checks ran on its platform
([decision 0285](decisions/0285-linux-windows-plugin-builds.md)).

Machines and tools: Linux is Arch Linux x86_64 (kernel 7.2.11), rustc 1.99.1,
GCC 16.2.1, clang/clang-cl 23.1.3, CMake 4.4.4 (Makefiles), cargo-xwin 0.23.1
(Microsoft CRT/SDK from its download), Wine 11.20, sord 0.16.22
(`lv2_validate`), lilv 0.28.0. Windows is a Windows 11 x64 VM (NT 10.0.26100)
with no compilers and no Visual C++ Redistributable. External SDKs: Steinberg
VST3 3.8.1, LV2 1.18.10, AAX SDK 2.8 (`AAX_SDK_VERSION 0x0208`). clap-validator
0.4.1 (commit `152b982`): the official Linux release binary; for Windows the
same commit built from source with `cargo xwin` and a static CRT, because the
official Windows binary needs `VCRUNTIME140.dll`.

| Format | Platform | Build | Checks |
|---|---|---|---|
| CLAP | Linux x86_64 | `bundle-clap.py --plugin delay` | clap-validator: 44 run, 33 passed, 0 failed, 0 warnings, 11 skipped (6 further runs identical) |
| CLAP | Windows 11 x64 | `bundle-clap.py --plugin delay --target windows` | clap-validator on Windows: 44 run, 32 passed, 0 failed, 12 skipped; `process-audio-denormals` warned (2.1–2.8× slower with denormal input) in 3 of 7 runs and passed in 4. The extra skip is `scan-rtld-now` (Linux only). |
| VST3 | Linux x86_64 | `bundle-vst3.py --plugin delay --sdk …` | Steinberg validator: 47 passed, 0 failed (Delay and `--mock`); mock contracts CTest 1/1 |
| VST3 | Windows 11 x64 | `bundle-vst3.py --plugin delay --sdk … --target windows` | Cross-built `validator.exe` on Windows: 47 passed, 0 failed (Delay and mock); `moduleinfotool.exe -create` exit 0; mock contracts CTest 1/1 under Wine |
| LV2 | Linux x86_64 | `bundle-lv2.py --plugin delay --sdk …` | CTest 4/4: mock contracts 100 checks, mock lilv 12, delay smoke 32, delay lilv 8, 0 failures; `lv2_validate` 0 errors among 84 files |
| LV2 | Windows 11 x64 | `bundle-lv2.py --plugin delay --sdk … --target windows` | `ravet_lv2_contracts.exe` on Windows: mock 100 checks, delay smoke 32 checks, 0 failures; CTest 2/2 under Wine; `lv2_validate` of the Windows Turtle 0 errors |
| AAX | Windows 11 x64 | `bundle-aax.py --plugin delay --aax-sdk … --target windows` | `ravet_aax_check.exe` on Windows: PASS (2×2 and 1×1 effects, 8 parameters, 8 blocks + empty block, state chunk round trip and corrupt chunk rejected); mock `ravet_aax_adapter_tests.exe` PASS; CTest under Wine 1/1 (delay), 2/2 (mock). Not Avid's validator; unsigned. |

The Windows bundles and tools import only system DLLs (`KERNEL32`, `USER32`,
`ole32`, `SHELL32`, `ntdll`, `api-ms-win-core-synch`, `bcryptprimitives`), as
`llvm-objdump -p` shows. The Windows Delay Turtle equals the Linux Turtle
apart from the binary name and CRLF line endings.

Fixes found by these builds:

- VST3: the SDK's hosting examples were turned on, pulling gtkmm into Linux
  builds; they are now off (the validator is built regardless). The adapter now
  pins the MSVC runtime to the SDK's (clang-cl defaulted to `/MT` against the
  SDK's `/MD`), and lists `CMAKE_CONFIGURATION_TYPES` so single-configuration
  generators put the Windows module in `Contents/x86_64-win`.
- LV2: the Turtle generator is called by target name so a cross-compiling
  emulator applies; the build machine's lilv is not used when cross-compiling.
- AAX: rustc's `/defaultlib:libcmt` note is passed as `-defaultlib:libcmt`;
  CMake on Linux read the slash form as a file path.
- AAX tests: `FakeHost.hpp` gave the adapter 1,024-frame host buffers while the
  test runs a 2,048-frame block that the adapter (correctly) silences in full.
  On Windows this was a heap overrun (`0xC0000374`; Wine's heap check located
  it); macOS allocation slack had hidden it. The buffers now hold 2,048 frames
  and larger test blocks are refused.

New: `--target windows` on the four bundle scripts, `scripts/windows_cross.py`
and `scripts/cmake/windows-xwin.cmake`, with 7 new helper tests and cross-build
tests in the VST3, LV2 and AAX script suites (all script suites pass). After
the changes, macOS arm64 still passes: VST3 Delay 47/0, LV2 Delay CTest 4/4 and
`lv2_validate` 0 errors, AAX mock CTest 2/2.

Limits: no DAW (REAPER or other), Pro Tools, Avid validator or lv2lint run on
either platform; editors do not open on Linux or Windows; native MSVC Windows
builds and the AAX/VST3 native Windows commands remain unrun; the AAX GUI is
still registered on Windows and would show an empty view. The intermittent
Windows denormal warning was not reproduced on Linux (0 of 7 runs); the
x86-64 denormal guard has no OS-specific code, and the VM's timing noise is
the likely cause, but this is not established.

<a id="release-010-live-delay"></a>

### Ableton Live 12 VST3 delay check, and value text (2026-11-05)

The owner loaded the VST3 delay (installed in `~/Library/Audio/Plug-Ins/VST3`)
in Ableton Live 12 Suite on macOS 27 arm64 and exported two renders of the
synthetic impulse input at 48 kHz, 24-bit, 150 BPM. Live warped the clip (the
impulses arrived 57,600 samples apart instead of 72,000, a 120/150 stretch), so
the strict REAPER checker does not apply; the echo spacing relative to each
impulse was measured instead:

- Synced (1/8, mix 50 %, feedback 0): each echo exactly 9,600 samples (200 ms,
  an eighth note at 150 BPM) after its impulse; dry and echo about 0.25 / 0.125
  per channel as expected.
- Fixed (sync off, mix 100 %, feedback 0): one echo 4,815 samples after each
  impulse and no dry signal. The time had been dragged to about 100.3 ms, which
  the editor and host text displayed as "100 ms".

Following that, host value text keeps one decimal from 100 (dropped when zero)
and two below 100, with whole numbers only for logarithmic values from 1,000 and
integral steps, and the delay editor shows the same precision and accepts typed
values (click the value). The rebuilt VST3 delay passes the Steinberg
validator (47 passed, 0 failed) and replaced the installed copy. This is a
manual, single-host check; Live's editor, parameter list and session recall
answers are pending from the owner.

Repeat of the fixed case in Ableton Live 12 Suite (macOS 27 arm64) with the
rebuilt VST3 delay (typed value entry), the clip unwarped and Time typed as
100 ms (sync off, feedback 0 %, mix 100 %), exported at 48 kHz, 24-bit: the
REAPER render checker (`delay_checks.check_case`, 100 ms, mix 1.0) passes. Echoes
are at frames 28,800 and 100,800 (+4,800 frames = 100 ms after the impulses at
24,000 and 96,000) with amplitudes exactly 0.5 / −0.25, no dry signal, and a
maximum residual of 0 over all 288,000 frames on both channels. This is a manual
export checked offline, not a scripted session.

Repeat of the synced case in Live 12 with the clip unwarped (sync on, 1/8,
feedback 0 %, mix 50 %, project tempo 150 BPM), 48 kHz, 24-bit: the checker
(200 ms, mix 0.5) passes. Dry impulses at frames 24,000 and 96,000 and echoes at
33,600 and 105,600 (+9,600 frames, an eighth note at 150 BPM), all exactly
0.25 / −0.125, with a maximum residual of 0 over all 192,000 frames. Together
with the fixed case, both delay renders match the REAPER harness results
sample for sample.

<a id="release-010-pro-tools-delay"></a>

### Pro Tools 26.4 AAX delay check (2026-11-05)

The owner supplied a PACE wrap configuration, and the macOS arm64 delay
`.aaxplugin` from `bundle-aax.py --plugin delay` was signed outside the
repository with the owner's account and Developer ID certificate (`wraptool
sign`; signed, not copy-protection wrapped; `codesign --verify --strict` and
`wraptool verify` pass), installed into
`/Library/Application Support/Avid/Audio/Plug-Ins`, and loaded by the owner in
Pro Tools 26.4 at 48 kHz and 150 BPM. Signing material is not stored in RAVET.
Bounces of the synthetic impulse input (48 kHz, 24-bit), measured offline:

- Fixed (sync off, feedback 0, mix 100, Time reset to its 375 ms default):
  echoes at frames 42,000 and 114,000, exactly 18,000 samples after the impulses
  at 24,000 and 96,000, no dry signal. A further 0.00073 of the 0.5 peak sits one
  sample earlier (the two sum to exactly 0.5), i.e. a delay of about
  374.99997 ms: the reset value lost precision in the host's normalized
  parameter round trip.
- Synced (sync on, 1/8, feedback 0): echoes exactly 9,600 samples (200 ms, an
  eighth note at 150 BPM) after each impulse. Dry 0.2479 and echo 0.2521 per
  unit (summing to exactly 0.5) show the dragged mix was about 50.4 %, not 50 %.

The strict checker fails both files on amplitude only, because the parameters
were not set exactly; echo positions and tempo sync are exact. Keyboard entry
into the web editor does not work in Pro Tools, which keeps key input for its
own shortcuts; mouse editing works. This is a manual single-host check.

<a id="release-010-logic-delay"></a>

### Logic Pro AUv2 delay check (2026-11-05)

The AUv2 delay, rebuilt with the current editor (`bundle-au.py --plugin delay
--validate`: in-process checks pass; installed as
`~/Library/Audio/Plug-Ins/Components/RAVET Delay.component`; `auval -v aufx QFKS
TYDS` succeeds), was loaded by the owner in Logic Pro on macOS 27 arm64 at 48 kHz
and 150 BPM, with values typed into the editor (Time 100 ms, Mix 50 %). The
bounces (48 kHz, 24-bit) also contain Logic's metronome (a click on every beat
from frame 0), so the strict checker reports the clicks; the delay was measured
at the exact sample positions instead:

- Fixed (sync off, feedback 0, mix 100): echoes at frames 28,800 and 100,800
  (+4,800 samples, 100 ms), exactly 0.5 / −0.25, no dry signal, and no other
  signal within ±200 samples of each echo.
- Synced (sync on, 1/8, feedback 0, mix 50): dry at 24,000 and echoes at 33,600
  and 105,600 (+9,600 samples, an eighth note at 150 BPM), exactly
  0.25 / −0.125. At frame 96,000 a metronome click starts on the same sample
  as the second impulse; the samples there equal the click plus the dry signal
  exactly (0.071013 + 0.25 = 0.321013).

Typed values worked in Logic. The owner reports that closing and reopening the
plug-in window, and saving, closing and reopening the project, kept the editor
working and the settings intact. This is a manual single-host check.

<a id="release-010-host-integration"></a>

### Integration: Windows and Linux REAPER evidence merged (2026-11-05)

Merged `evidence/reaper-win-linux`: the REAPER delay harness now passes for VST3
on Linux and for CLAP, VST3 and LV2 on Windows 11 (three runs each; see the
entry above), so every format × platform cell in the 0.1.0 plan has a recorded
delay run except AAX on Windows, which the owner accepted as provisional.

Re-run on macOS 27 arm64 with the merged harness (REAPER 7.82): CLAP passed
three consecutive runs (`target/reaper-delay/mac-clap-r1`, `-r2` and one run
from a boot-disk work directory); the first run after the merge wrote no host
log before its 120 s timeout, apparently an intermittent slow start (window
titles in a later run showed only the evaluation About window, the project and
the render, with no blocking dialog). VST3 in REAPER on macOS still stalls
before the script runs on this machine, as recorded earlier, because REAPER
scans the system VST3 folders there; macOS VST3 evidence comes from Ableton Live
12 instead.

### Web-editor keyboard focus for Pro Tools (2026-11-05)

Follows the Pro Tools 26.4 AAX delay check, where typed values could not be
entered. AAX SDK 2.8 (local copy) has no keyboard-focus call:
`AAX_IViewContainer` offers view type and pointer, `GetModifiers`,
`SetViewSize` and parameter mouse-event forwarding only, and
`AAX_IEffectGUI`/`AAX_CEffectGUI` have no key handling. The Cocoa web view
(`RavetWebView.mm`) now takes key status and first responder while a page text
field has focus, returns key status afterwards if it took it, and forwards
plain typing through a local event monitor when the window refuses key status
([decision 0286](decisions/0286-editor-keyboard-focus.md)). The editor
protocol, `client.js` and `RavetPlugin.h` are unchanged.

Evidence on macOS 27 arm64:

- New `ravet-web-keyboard-focus` ctest (real AppKit windows): panel that becomes
  key only if needed, window refusing key status, ordinary key window, three
  groups pass. Against the previous `RavetWebView.mm` it fails at "panel became
  key for the text field". All 17 native web-view ctests pass
  (`cmake -S crates/ravet-surface/native -DRAVET_WEB_EDITOR_TESTS=ON`).
- `bundle-aax.py --plugin delay` against the local AAX SDK: bundle check passes,
  and `ravet_aax_check --editor` opens both delay editors in a real `NSView`.
- `bundle-clap.py --plugin delay`, `bundle-vst3.py --plugin delay` (SDK
  validator 47 passed, 0 failed) and `bundle-au.py --plugin delay --validate`
  (editor lifetime and real-delay contracts pass) build with the change.
- `cargo test -p ravet-webview` and `cargo test -p ravet-plugin --features
  native,clap` pass; `check-native-semantics.py` passes.
  `check-window-lifecycle.py` fails to compile its Rust fixture
  (`unresolved import crate::accessibility`), independent of this change.

Not yet confirmed in Pro Tools: the bundle must be signed by the owner and
loaded there. If Pro Tools consumes key events before AppKit's local event
monitors run, this change cannot help.

### Standalone evidence from DW-303 (owner report, 2026-11-05)

The owner reports that the DW-303 standalone application (its own repository,
built on RAVET's `StandaloneWindow`, the standard Audio/MIDI title-bar popup and
the Windows/Linux web views merged from `main` in `67ee1d6`) has been run on
macOS, Windows and Linux, and that audio output and MIDI input were exercised
on macOS. Device names and dates were not recorded here. This is an owner
report about a consumer product, not a RAVET-run check; Windows and Linux audio
and MIDI remain to be exercised.

Also merged into `release/0.1.0`: `main` at `67ee1d6` (Windows and Linux web
views and standalone windows) and `fix/aax-keyboard` (macOS web editors take key
status while a text field has focus, decision 0286). The rebuilt macOS AAX delay
passes its bundle check; a vendor-signed copy is installed for a Pro Tools
keyboard test.

### Standalone runner deferred (owner decision, 2026-11-05)

The owner moved release-plan §3 (generic standalone runner: device-inventory
provider, MIDI outputs in the popup, engine handoff across stream rebuilds,
MIDI timestamps to sample offsets, headless operation, state on quit /
fixed-size window / product name in the menu, file dialogs) to after 0.1.0.
A review of the DW-303 standalone application found that it performs each of
these in product code (device listing via `ravet_device::output_devices`, its
own stream lifecycle and state file, MIDI delivered at offset 0), so none is
yet a RAVET framework feature. A standalone-runner agent started earlier was
cancelled before producing changes.

### Integration: robustness review merged (2026-11-05)

Merged `review/robustness`; its decision record became 0287 (the keyboard-focus
record had taken 0286). Conflicts were documentation only; feature records keep
the newer host evidence from `release/0.1.0` and add the review's limits.

On the merged branch (macOS 27 arm64): `cargo test` for `ravet-plugin` (all
features), `ravet-delay-plugin`, `ravet-gain-plugin`, `ravet-native` and
`ravet-foundation` pass; Clippy with warnings denied for `ravet-plugin` and
`ravet-delay-plugin` (all targets, all features) and `cargo fmt --check` pass.
The rebuilt delay CLAP passes `ravet-clap-smoke --delay`, including the new
oversized-block rejection, and ships `RAVET-LICENSE.txt`,
`THIRD-PARTY-NOTICES.txt` and the Rust standard library notice. clap-validator
0.3.2-127 reported 32 passed, 0 failed, 11 skipped and one warning
(`process-audio-denormals`, 2.65× slower) on the first run; six further runs
passed that test. The review's only floating-point-state change is in the
x86_64 `fxsave` path, which this arm64 machine does not execute, so the warning
is treated as timing noise, as with the earlier Windows VM observations.

### Integration: bundle tool and Foundry merged (2026-11-05)

Merged `tools/bundle-foundry`; its decision record became 0288 (0286 and 0287
were taken). The per-format scripts conflicted with the robustness review's
licence notices; both were kept (notices plus the tool's optional signing).
The unified tool initially shipped no notices in its CLAP, AUv2 and LV2
bundles, because it hands each format a prebuilt library; `plugin_bundle.py`
now writes the notices for every format before signing (new
`test_notices_ship_with_every_format`; 122 script tests pass).

`python3 scripts/bundle-plugin.py --package ravet-delay-plugin --formats
clap,vst3,au,aax,lv2 --release --validate` with SDKs from `RAVET_*_SDK`
environment variables (macOS 27 arm64): clap-validator 33 passed, 0 failed,
11 skipped; Steinberg validator 47 passed, 0 failed; AUv2 checks and
`auval` succeed; AAX bundle check 1/1; LV2 4/4 with `lv2_validate` 0 errors.
All five bundles carry `THIRD-PARTY-NOTICES.txt`, and the four signed macOS
bundles pass `codesign --verify --strict`.

### Integration: state policy and stereo DSP merged; combined check (2026-11-05)

Merged `policy/state-and-dsp`; its decision records became 0289 (versioned
observable-tree records) and 0290 (stereo block FIR and oversampling), because
0286–0288 were taken. Conflicts were documentation only.

Combined `release/0.1.0` check on macOS 27 arm64: 586 tests pass, 0 fail, for
`ravet-state`, `ravet-preset`, `ravet-foundation`, `ravet-signal`,
`ravet-plugin`, `ravet-delay-plugin`, `ravet-gain-plugin` and `ravet-native`
with all features; Clippy with warnings denied (all targets, all features) and
`cargo fmt --check` pass; `cargo check --workspace` passes; 122 script tests
pass; the documentation checker passes. `bundle-plugin.py --package
ravet-delay-plugin --formats clap,vst3,au,aax,lv2 --release --validate`:
clap-validator 33 passed, 0 failed, 11 skipped; Steinberg validator 47 passed,
0 failed; `auval` succeeds; AAX bundle check 1/1; LV2 4/4 and `lv2_validate`
0 errors.

### macOS REAPER harness blocked on this machine (2026-11-05)

After the MIDI merge, the macOS REAPER harness no longer starts its script on
this Mac for any product: a control re-run of the delay CLAP check, which had
passed here earlier the same day, timed out after 120 s, as did the instrument
CLAP check (also with the delay harness's app identity). Window titles during a
stuck run showed REAPER's "Scanning VST plugins..." window over the main
window; the earlier instrument agent's stack trace showed an alert during audio
initialisation. Choosing BlackHole 16ch as the CoreAudio device, and seeding the
disposable directory with a read-only copy of the normal VST scan cache, did not
help (the latter experiment was not kept). The harness's guard confirmed the
normal REAPER directory unchanged in every attempt. This is treated as a
machine-specific environment issue: the same harness passes on Linux and
Windows, and macOS host evidence for each format comes from Ableton Live 12,
Logic Pro, Pro Tools 26.4 and the earlier REAPER runs. macOS instrument
MIDI evidence in a host is therefore still missing.

### macOS REAPER unblocked; instrument evidence; AUv2-in-REAPER timing bug (2026-11-05)

Merged `platform/macos-x86-64` (code conflicts: VST3 CMake options and AU
validation kept both sides — the architecture-aware path plus the registrar
retry). Integration bug found and fixed: the merged `delay.prepare` gained a
`mac_arch` parameter that `instrument.py`'s wrapper did not accept, so every
instrument run failed with a `TypeError`; the wrapper now forwards it and a new
test checks that the wrapper's signature matches (19 harness tests pass).

The macOS REAPER blockage recorded earlier was REAPER's audio-initialisation
alert on the default device (UMC1820): with the disposable configuration set to
the built-in output (`coreaudiooutdevnew`/`coreaudioindevnew=Mac mini
Speakers`, 48 kHz, 512 frames), the delay CLAP harness passes again on arm64.
VST3 runs use a `sandbox-exec` wrapper that hides the system VST3 folders from
the private REAPER process (docs/host-validation.md). The normal REAPER
directory stayed unchanged in every run.

RAVET Evidence Synth on macOS 27 arm64, REAPER 7.82: CLAP, LV2 and VST3
(VST3 through the wrapper) pass the instrument harness — exact onsets at 24,000
and 48,000, CC 1 levels, reported latency 32 → 0 → 32 with PDC, finite tail
and MIDI echo to a second instance.

AUv2 delay in REAPER on arm64 (current build installed as
`~/Library/Audio/Plug-Ins/Components/RAVET Delay.component`): all 63 host
checks pass (discovery, 8 parameters, readback, recall), but renders fail.
Fixed (sync off, 100 ms, mix 100): echoes at 30,848 and 102,848 (+6,848 rather
than +4,800). Synced (1/8 at 150 BPM, mix 50): dry exactly at 24,000 and 96,000,
echoes at 36,672 (+12,672) and 110,208 (+14,208) rather than +9,600. This
matches the Rosetta observation, while Logic Pro bounces were sample-exact, so
REAPER's AU hosting exposes a time/tempo delivery defect in the AUv2 adapter
or a host interaction still to be found. Under the `sandbox-exec` wrapper REAPER
saw none of the AU's parameters, so AU runs must not use it.

### Integration: Windows/Linux plug-in editors merged (2026-11-05)

Merged `editors/windows-linux`; its decision record became 0293 (0291 and 0292
were taken). Code conflicts with the web-view robustness review were combined:
the new `ravet_web_create_assets` follows decision 0292 (`noexcept` on all three
platforms; on macOS a wrong-thread call returns null instead of asserting), and
the WebView2 state keeps the review's rule that the loader is never unloaded
while adding the asset handler's environment and token. Documentation now says
MSVC builds link the static WebView2 loader (no DLL shipped) and that plug-in
editors open on all three platforms.

A cross-target lint found 16 undocumented `unsafe` blocks in the x86_64 SSE2
paths of `ravet-native-dsp`, which the robustness review's new lint rejects but
had only checked on arm64; each now has the same safety comment as its AArch64
twin.

Checks on the merged branch:
- macOS 27 arm64: 97 Rust tests pass for `ravet-webview`, `ravet-plugin` and
  the delay, gain and synth plug-ins (all features); Clippy with warnings
  denied passes; native web-view CTest 16/18, including the new asset test —
  `first-mouse` and `keyboard-focus` fail as before this merge because the
  session cannot take key-window status; `bundle-plugin.py` for the delay in
  all five formats with `--validate` passes (clap-validator 33/0/11, Steinberg
  47/0, `auval`, AAX 1/1, LV2 4/4 with 0 Turtle errors).
- Arch Linux x86_64 (omarchy): Clippy with warnings denied for `ravet-webview`,
  `ravet-plugin` and `ravet-native-dsp`; `ravet-webview` and `ravet-plugin`
  tests pass; native web-view CTest 5/5 (including assets) with
  `GDK_BACKEND=x11`, `DISPLAY=:0`, no `WAYLAND_DISPLAY` and
  `WEBKIT_DISABLE_DMABUF_RENDERER=1`. With `WAYLAND_DISPLAY` set, lifecycle and
  paint segfault and `host-gtk-supported` fails identically on both source
  branches, since editors reject a Wayland GTK context.
- Windows x64 cross-build (cargo-xwin, WebView2 SDK): Clippy with warnings
  denied for the same three crates, and `bundle-plugin.py --target windows`
  builds the delay CLAP, VST3 and LV2 bundles with notices. Not run on Windows
  after the merge.

<a id="release-010-reaper-au-timing"></a>

### AUv2 in REAPER: transport sample residue fixed; exact renders (2026-11-05)

Branch `fix/auv2-reaper-timing` from `release/0.1.0` at `1421865`. Machine:
Apple M4 Pro, macOS 27.0.1, REAPER 7.82 (private copy under
`target/reaper-delay/app`), AudioUnitSDK 1.4.0, Mac mini Speakers at
48 kHz/512 frames in the disposable configuration.

**Root cause.** A temporary, environment-gated trace of the adapter's render
path (removed before commit) showed REAPER calling `Render` once per 512-frame
block with continuous sample times. It sets the parameters once from the main
thread before the render. No reset, re-initialization or format change happened
mid-render. In one traced run, 147 of about 1,200 renders received a
`TransportState2` result: a whole block start with one ulp of Float64
residue, for example `3584.0000000000005`, `7168.0000000000009` and
`13823.999999999998`. `RavetAuTransport.hpp` required an exact integral sample,
so `nativeTransport` failed. The adapter then silenced the block and returned
`kAudioUnitErr_InvalidPropertyValue` (-10851) without calling the product, so
the delay line did not advance. Each rejected block between an impulse and its
echo delayed the echo by 512 frames. The observed errors correspond to four
rejected blocks for fixed renders (+2,048), and six and nine for synced renders
(+3,072 and +4,608). Dry positions
stayed exact because the impulse blocks were accepted. Logic Pro reports exact
integers. CLAP, VST3 and LV2 renders in REAPER were already exact; those
adapters do not use this AU collector.

**Fix** ([decision 0294](decisions/0294-au-transport-sample-residue.md)):
`nativeTransport` canonicalizes a sample position within
`min(2^-10, max(|n|, 2^20) × 2^-40)` samples of an integer `n` to `n`.
Genuinely fractional positions, such as 0.5, still reject. No adapter code
changed. The performance AU shares the helper.

**Tests.**
- `tools/au-plugin-validation/delay.mm` adds a REAPER-pattern case on the
  actual delay component: 48 kHz, 512-frame renders with continuous sample
  times, main-thread `SetParameter`, beat/tempo (150 BPM), signature and
  `TransportState2` callbacks, and one-ulp residue on two of every seven blocks.
  Fixed 100 ms must give exactly two peaks at 28,800 and 100,800. Synced 1/8,
  mix 50 must give peaks at exactly 24,000, 33,600, 96,000 and 105,600.
  - Before the header fix, this failed with `FAIL REAPER-pattern render`.
  - After the fix, `scripts/bundle-au.py --plugin delay ... --validate` passes.
- The generic `contracts.mm` host transport now reports
  `nextafter(456, +inf)`. The mock still requires exactly 456.
- `tools/au-validation/transport.cpp` adds the observed REAPER values, a
  near-zero residue, a below-zero whole value and two rejections: a value
  beyond tolerance and a large value beyond the absolute cap.

**Results.**
- `bundle-au.py --plugin performance --vst3-sdk ... --audio-unit-sdk ... --validate`:
  `PASS AU native transport callback/preflight/projection contracts` twice
  (normal and `-O3 -ffast-math`). The actual performance bundle checks also
  passed, including host transport faults/recovery, plus the remaining AU
  performance/precision contracts.
- `bundle-plugin.py --package ravet-delay-plugin --formats au --release --validate`:
  generic contracts, delay contracts (including the REAPER pattern) and
  `AU VALIDATION SUCCEEDED.`
- `--arch universal --validate`: the same contracts, plus auval for both
  slices (the x86_64 slice through process-local registration).
- `--package ravet-synth-plugin --formats au --release --validate`: three Evidence
  Synth cycles, both passing under the residue callback, and auval succeeded.
  The instrument shares the render and transport path, so before this fix it
  would also have lost audio and the block's queued MIDI on rejected blocks.
  The instrument REAPER harness does not cover AU, so there is no REAPER
  instrument AU run.
- `bundle-au.py --mock effect|instrument|midi --validate`: all three mock
  contracts passed.

**REAPER Delay AU, macOS arm64**: 63 host checks passed with
`RESULT PASS` (`target/reaper-delay/au-fix-arm64`; owner's identity
`aufx QFKS TYDS` installed at
`~/Library/Audio/Plug-Ins/Components/RAVET Delay.component`, then
`AudioComponentRegistrar` restarted). Both channels were exact, with zero
residual elsewhere over 192,000 frames:
- Fixed: echoes at 28,800 and 100,800 (+4,800). Channel 0 amplitude 0.5,
  channel 1 amplitude −0.25, no dry signal.
- Recalled: same as fixed.
- Synced: echoes at 33,600 and 105,600 (+9,600). Channel 0 echo 0.25 with dry
  0.25; channel 1 echo −0.125 with dry −0.125.

**REAPER 7.82/OSX64 under Rosetta 2** (`--arch x86_64`, universal bundle
installed at the same path, `target/reaper-delay/au-fix-x86_64`): 63 host
checks and the same exact positions for fixed, recalled and synced,
`RESULT PASS`. The normal REAPER resource directory stayed unchanged in every
run.

The universal fixed build remains installed. These are offline renders only.
They make no claim for live playback, editors, automation, devices or native
Intel hardware.

### Integration: AUv2 REAPER timing fix merged (2026-11-05)

Merged `fix/auv2-reaper-timing`; its decision record became 0294 (0293 was taken
by the editors work), amending 0188. Root cause: REAPER reports some AU sample
positions with Float64 residue (for example `3584.0000000000005`), which
`RavetAuTransport.hpp` rejected, silencing those blocks without advancing the
product; positions within 2^-10 of a whole sample now round to it. With the
fix, the REAPER 7.82 AU delay harness passes on arm64 and under Rosetta with
exact echo positions (see the entry above). The design question of whether a
malformed transport callback should still process audio without transport,
instead of silencing the block, is left open under decision 0188.

### Pro Tools keyboard check passes (owner, 2026-11-05)

The owner loaded the vendor-signed rebuild of the macOS AAX delay containing
the keyboard-focus fix (decision 0286) in Pro Tools 26.4 on macOS arm64 and ran
the five-step check: Space starts and stops the transport while nothing is
being edited; clicking the Time value, typing 250 and pressing Enter sets
250 ms, after which Space again reaches the transport; Escape cancels a typed
Mix value; clicking away commits a typed Feedback value; Command-S saves the
session while a value is being edited. The owner reports that the check passes.
This is a manual single-host check; Pro Tools for Windows is not covered.

**Correction (owner, same day):** the check passed only in part. Typing
characters into a field worked, but Return, Enter and Escape were taken by
Pro Tools; clicking away after typing 250 did not commit; and Space while a
field was focused typed a space into it, as expected for a text field. The
earlier sentence overstated the result.

### Keyboard focus follow-up for Pro Tools (2026-11-05)

Cause: when Pro Tools lets the editor window become key, the decision 0286 code
installed no key monitor, so key events took the normal path, where Pro Tools'
own event handling takes Return, Enter and Escape; and WebKit keeps a field
focused when its window resigns key status, so clicking a Pro Tools window never
blurred it. `RavetWebView.mm` now always forwards plain keys to the web view
while a field is focused and ends editing (blurring, so pages commit) when the
editor window resigns key (decision 0286 amendment). New fixture group "host
that takes Return and Escape" fails against the previous code ("Return and
Escape reach the field") and passes with the fix; the native web-view CTest
passes 18/18 on macOS arm64 in this session (including first-mouse and
keyboard-focus, which this session can now run). The AAX delay was rebuilt (its
bundle check passes), signed outside the repository with the owner's
Developer ID through `wraptool` (signed, not wrapped) and installed in
`/Library/Application Support/Avid/Audio/Plug-Ins` for a Pro Tools re-check.

Pro Tools 26.4 re-check of the follow-up (owner): (1) typing 250 then Return
did not commit, and Pro Tools returned its transport to zero; (2) Space started
and stopped the transport; (3) Escape closed the plug-in window; (4) typing 30
then clicking the Edit window committed 30 %; (5) Command-S saved the session
while a value was being edited. Return and Escape therefore never reach the
plug-in: Pro Tools handles them before AppKit's local monitors run. RAVET does
not patch host event handling; in Pro Tools typed values apply on click-away,
and editor pages should commit on blur (decision 0286, final note).
