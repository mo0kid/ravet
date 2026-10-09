# Native embedded-editor prototype

RAVET now has a Cargo-built macOS binding and an embedded CLAP gain editor.
`ravet-webview` isolates Cocoa/WebKit ownership and FFI from portable processing.
`ravet-surface/native` contains reusable C++ backends for macOS, Windows and
Linux, with native lifecycle probes. The Windows/Linux paths currently use the
C++ API; the Cargo Rust binding and gain GUI remain macOS only. The umbrella crate does not depend on either native code or
JSON parsing. A general portable widget/layout API remains outside this increment.

## Rust ownership and build boundary

`ravet_webview::WebView::from_html` owns an embedded NSView and callback context.
Creation checks the Cocoa main thread. The type is neither Send nor Sync;
operations and destruction stay on that thread. Callback context is retained at
entry so a consumer can release its owner while a callback runs. Buffer snapshots
copy into native-owned storage before releasing that retention. Reentrant Rust
callbacks are skipped and panics are contained at the FFI boundary.

Drop invalidates the native timer, script handlers and navigation delegates,
stops loading, detaches the view, then releases callback state. Attaching to a
host view is the narrow unsafe operation: the parent must be a live NSView on the
main thread and remain valid until detach/destruction. The CLAP host supplies that
lifetime contract. The wrapper does not create an NSApplication, system window
or run loop; the embedding host must provide them.

On macOS, Cargo invokes Xcode's `xcrun clang++` and `ar`, compiles the Objective-C++
backend with ARC, and links Cocoa, WebKit and libc++. Native compilation targets
macOS 12 and arm64/x86_64 as selected by Cargo. Only the recorded macOS arm64
environment has been exercised; macOS 12 and x86_64 runtime compatibility are not
established by the compile flags. Cross-compilation from a non-macOS build host
is not provided. The crate exposes no native API on other operating systems.

The gain export's default `native-editor` Cargo feature includes this binding and
pins control-thread `serde_json` to 1.0.151 (MIT or Apache-2.0). Runtime transitive
dependencies are locked: serde_core (MIT/Apache-2.0), itoa (MIT/Apache-2.0), memchr
(MIT/Unlicense), and zmij (MIT). Their manifest licences were inspected locally.
These dependencies do not enter the portable processor graph. Build the export
with `--no-default-features` to omit its native editor and JSON dependency.
Workspace checks still build native workspace members on macOS and need Xcode.

## Gain editor and CLAP lifecycle

The macOS export advertises the CLAP GUI extension for **embedded Cocoa only**.
It creates a 480×390 logical-pixel editor, accepts resize requests through
1200×900, attaches to a host-owned NSView, and
supports hide/show and explicit destruction independently of audio processing.
Floating windows and resizing are rejected; positive scale hints are accepted
but Cocoa supplies logical scaling. Show requires an attached parent.

A self-contained HTML/CSS/JavaScript page is compiled into the library. No
repository path or separately installed assets are required. The page has a gain
slider, numeric multiplier, bypass checkbox, and stereo input/output sample peak
and RMS display. Slider input sends live values within one begin/end pointer
gesture; numeric
changes and bypass remain complete committed edits. The bounded queue reserves
end capacity and reports rejected updates. Pointer release/cancellation and blur
close the gesture; editor destruction queues end and requests a host callback.
Host automation updates unfocused controls.
Meters show the latest block without decay or peak retention; values above full
scale are capped only in the bar display, with actual dB values retained in text.

The Rust controls use [reusable attachments](parameter-attachments.md) for
metadata, canonical values and explicit gesture ownership.

The page posts `{type:"edit"|"begin"|"value"|"end", id, value}` messages. The Rust consumer validates
the bounded JSON shape and numeric fields, then uses the existing [control
bridge](editor-control.md) to queue complete edits or live gesture commands. Unknown parameters,
invalid input and queue overflow are displayed as errors; a successful value,
complete edit or new begin clears the error. End preserves an earlier failure. Requested values and coalesced meters return as JSON snapshots
on the main thread. JSON parsing, serialization and DOM work never run in audio
callbacks. A pending indicator means the host has not yet consumed all edits.

The page's CSP disallows external resources. The native navigation delegate
blocks external navigation. This is trusted packaged page code, not a sandbox
for arbitrary consumer scripts.

## Native backend contract

The C++ header retains local-file creation for existing consumers and adds
`ravet_web_create_html` with copied buffer snapshots. Every call/callback belongs
on the host UI thread (the Cocoa main thread on macOS). Windows/Linux bind their
UI-thread check to the first successful creation; the caller must choose the
correct thread. Local-file navigation stays under its asset root;
inline HTML permits `about:blank`. Callback message JSON is bounded to 65,536
bytes; inline page input is bounded to 1 MiB. Consumers of the raw C++ API own
pointer validity and callback lifetime. The Rust wrapper supplies those contracts
for its owned view and context.

Opaque layer backing is configurable through finite clamped RGB values. The
WebKit under-page colour requires macOS 12. A 50 ms timer delivers validated JSON
objects/arrays through `window.ravetUpdate`. Identical strings are suppressed
after successful delivery; changed snapshots coalesce while one JavaScript
evaluation is in flight. Navigation clears the delivered snapshot. DOM updates
also avoid rewriting unchanged properties. No callback deadline or native heap
accounting is implied by this control-thread machinery.

On macOS, editor web views accept the activating mouse press. A drag that begins
in an inactive editor window reaches the page on that first press, without a
separate click to focus it. The native AppKit first-mouse probe checks begin,
change and end delivery while another owned window is key. Host-specific
window routing still needs actual DAW validation. This probe briefly shows two
owned windows and requires an interactive Cocoa desktop session.

## Run the probes

Build the default bundle, then exercise the editor with a real Cocoa main loop:

```sh
python3 scripts/bundle-clap.py
cargo run -p ravet-editor-smoke --locked -- "target/clap/release/RAVET Gain.clap" target/editor.png
```

Use the bundler's printed path when `CARGO_TARGET_DIR` differs. The optional PNG
argument captures the rendered page. The probe uses a hidden host window and
loads only trusted RAVET bundles. It checks three create/attach/show/edit/gesture/
automation/meter/hide/destroy cycles, visible protocol errors, and further audio
processing after GUI destruction. Main-loop WebKit startup can be asynchronous;
run this native probe separately from other WebKit startup probes.

The original local-file C++ lifecycle probe remains available:

```sh
cmake -S crates/ravet-surface/native -B target/web-editor -DRAVET_WEB_EDITOR_TESTS=ON
cmake --build target/web-editor --parallel 4
ctest --test-dir target/web-editor --output-on-failure
```

Rust tests additionally cover wrong-thread creation, snapshot buffer bounds,
reentrant callback rejection, and copying before callback context destruction.
[Work status](WORK_STATUS.md) records executed results and an observed initial
concurrent-startup timeout. Native/main-loop and development-host evidence does
not establish broad DAW compatibility, native allocation behavior or real-time
deadlines. Separate [REAPER validation](host-validation.md) covers the tested
macOS gain editor session, controls, meters and FX-chain resize paths.
Windows/Linux evidence and requirements are described below.

## Windows and Linux C++ backends

The CMake library selects Cocoa/WebKit on macOS, Win32/WebView2 on Windows,
and GTK3/WebKitGTK 4.1 on Linux. None adds native dependencies to portable DSP.
See [decision 0014](decisions/0014-cross-platform-native-web.md). Consumers must
build their own format wrappers; this framework does not acquire VST3/AAX SDKs
or package proprietary products.

Windows parents are HWNDs. Build with `RAVET_WEBVIEW2_SDK` pointing at an
extracted official `Microsoft.Web.WebView2` NuGet package. The caller packages
its matching `WebView2Loader.dll` next to the consuming DLL/executable, retaining
the SDK's LICENSE/NOTICE and redistribution obligations. The backend checks for
an installed WebView2 Runtime before returning a view. Browser/controller
creation then completes asynchronously through the host's STA message loop.
Attach immediately, keep the UI loop running, and keep context alive until
explicit destruction. The loader and runtime are native Microsoft dependencies;
no browser runtime is bundled by RAVET. The SDK 1.0.3650.58 backend and probe
cross-compile with MinGW x64; a real Windows editor run remains pending.

Linux parents are X11 Window IDs cast to `void*`, including XWayland windows.
The backend selects X11 if it initializes GTK. It rejects an already initialized
Wayland GTK display; such hosts need an X11 GTK context, for example by launching
with `GDK_BACKEND=x11`. Native Wayland parenting is not provided. Dependencies
are GTK3, `webkit2gtk-4.1` >=2.40 and JSON-GLib, linked dynamically through
pkg-config. On Arch, the runtime WebKit package is `webkit2gtk-4.1`; distribution
package managers supply the dependent GTK/JSON libraries and their licences.

The web widget expands to fill the editor's allocation. On systems where
WebKit's accelerated DMA-BUF/GBM path fails to paint, build with
`-DRAVET_WEB_LINUX_SOFTWARE_RENDERING=ON` to use WebKit's non-accelerated
painting policy for these views. This CMake option defaults to `OFF` in the
framework, leaves the host's other views untouched and does not change the
browser sandbox. Software painting can change UI CPU cost; it does not change
DSP. The native paint probe checks actual GTK-rendered pixels and saves
`ravet-web-paint.png` in the test build directory, independently of JavaScript
message/DOM checks. See [WebKit's graphics architecture](https://docs.webkit.org/Ports/WebKitGTK%20and%20WPE%20WebKit/Graphics.html).

When a loaded SWELL host owns GDK event routing, the backend requests GTK via
its `SWELL_ExtendedAPI("LOAD_GTK")` capability before creating widgets. This
preserves the host dispatcher instead of replacing it with GTK's dispatcher.
Host libraries loaded locally are also detected. If that capability is absent
or fails, editor creation returns null; the consumer must handle this failure.
REAPER introduced this loader in 7.82. Earlier REAPER versions are therefore
outside this editor path. This requirement does not establish DAW support.

Linux hosts call `ravet_web_pump()` on the UI thread, typically every 10 ms.
It dispatches at most 32 ready GLib iterations per call. A VST3 consumer can
register an `ITimerHandler` through the host's `IRunLoop`; a host without a UI
loop must decline attachment. The pump is a no-op on Windows/macOS. All callback
and object ownership obligations remain outside the audio thread. The editor's
50 ms snapshot timer is separate from this host pump interval.

Build and run the local-file probe on Linux with an X11 display:

```sh
cmake -S crates/ravet-surface/native -B target/web-linux -DRAVET_WEB_EDITOR_TESTS=ON
cmake --build target/web-linux --parallel 4
ctest --test-dir target/web-linux --output-on-failure
```

The portable probe checks malformed snapshot rejection, identical snapshot
suppression, changed updates, viewport dimensions, restricted navigation, hide/show, reattachment,
three teardown cycles, callback-triggered destruction and no later callbacks.
It passed on Ubuntu 24.04 arm64 and x64 under local virtualization/emulation.
Those environments are not Omarchy/DAW evidence. For a root-only container probe,
WebKit sandboxing had to be disabled for that isolated test process; production
builds retain the default WebKit sandbox and require no such setting.

## Native file chooser

`RavetFileDialog.h` exposes a generic owned-completion open-file chooser. It is
a native UI shim; files, networking and worker lifetimes are Rust runtime APIs.
The Cocoa owned-window cancellation probe runs with `RAVET_WEB_EDITOR_TESTS`.
See [runtime services](runtime-services.md) for threading, platform and Rust-binding
limits. Product file types, titles and import policy remain consumer-supplied.

## Rust-owned native C ABI

`ravet-webview/include/RavetWebEditor.h` exposes the macOS `ravet_editor_*` ABI
for native consumers linking a Rust static library that retains `ravet-webview`
(via `ravet_webview::link()`). The public gain-native example offers opt-in
`native-editor` for this linkage; its default remains DSP-only. The generic ABI
owns the existing Rust WebView, without product parameter/state or format policy.
Format consumers provide their own controller mappings. The optional
[VST3 gain editor](vst3.md) consumes this ABI; the optional [AUv2 Cocoa editor](audio-unit.md) consumes its embedded view.
AAX mapping remains separate.

Create from trusted UTF-8 HTML bytes (1..1 MiB, no NUL) with positive dimensions up to 16384
logical pixels and caller-owned callbacks. A nullable release callback consumes
foreign context once on every creation failure or after native teardown and the
last entered callback returns. Without release, the caller retains borrowed
context through that lifetime. Messages are borrowed bounded UTF-8 JSON bytes;
snapshots fill up to 65536 bytes and return a byte count without requiring NUL.
Empty/malformed/over-capacity snapshots are skipped. Foreign callbacks must not
throw; nested Rust callback entry is skipped. Asynchronous message/snapshot
callbacks can destroy their owner, with context release deferred to return.

Status codes are 0 success, 1 invalid input, 2 wrong thread, 3 native/panic failure.
Every operation belongs on the Cocoa main thread, with a live unique handle and
valid NSView parent. Wrong-thread destruction retains the handle for main-thread
retry. Accepted destruction consumes it even if a destructor panic returns 3;
never reuse a consumed handle. Null destruction is a no-op. The host supplies its
AppKit/run loop/window; no native I/O or processing thread is selected.

Build and run the actual public-header probe:

```sh
CARGO_BUILD_JOBS=2 cargo build -p ravet-gain-native --release --features native-editor --locked
cmake -S tools/native-validation -B target/native-editor-abi -DRAVET_GAIN_LIBRARY="$PWD/target/release/libravet_gain_native.a" -DRAVET_VALIDATE_WEB_EDITOR=ON
cmake --build target/native-editor-abi -j2
ctest --test-dir target/native-editor-abi --output-on-failure
```

On macOS 27.0.1 arm64, the compiled C++ client passes three create/attach/snapshot/
edit/hide/show/resize/detach/destroy cycles, wrong-thread creation/operation/
destruction retention, and both callback self-destruction paths. Release occurs
exactly once after callback return; subsequent polling produces no callbacks.
Rust fixtures cover snapshot bounds/UTF-8/NUL and failure ownership. This does not
establish other-platform, assistive-tech or DAW editor support. See
[decision 0076](decisions/0076-native-editor-abi.md).

The public gain reference additionally provides a UI-only
`ravet_gain_editor_meter_snapshot` codec in `RavetGainEditor.h`. It validates
twelve host-delivered normalized peak/RMS/diagnostic values and emits four
readings for local AccessibleMeter controls. This allocates on the UI thread;
it never reads a live DSP handle. Legacy gain-only snapshot remains available.
Native VST3 output-parameter delivery is described in [the format guide](vst3.md).

Hosted native graphs expose guarded main-thread editor creation through their
retained control. The separately owned audio node may run, while exclusive
editor borrowing prevents control retirement until GUI destruction. Core
callbacks can be serviced once per consumer main tick through the guard.
See [the hosting guide](plugin-hosting.md#main-owned-editors-for-hosted-graph-nodes)
and [decision 0116](decisions/0116-hosted-graph-editor-ownership.md).

## Default standalone Audio/MIDI settings

`ravet-webview::StandaloneWindow` supplies a main-thread macOS application window,
parent-borrowed `StandaloneEditor`, event pumping and a standard **Audio / MIDI…**
title-bar button. `StandaloneOptions::default().io_settings` is true. Every
standalone UI project should use this default; set `io_settings: false` explicitly
when a project needs to omit it. Embedded plug-in `WebView` editors keep their
host's device routing. Headless DSP/CLI fixtures do not create windows.

The embedded standalone page follows native window content resizing. Native
fixtures check actual browser viewport dimensions before testing layout bounds.
Set `StandaloneOptions::lock_aspect_ratio: true` to preserve the initial content
width/height ratio during native user resizing. The default is false, allowing
other applications to choose their layout policy. Programmatic `set_size` fits
the largest matching rectangle inside a request when locked; native minimum
window dimensions still apply. The interactive examples enable the lock and
uniformly scale their entire design canvas, while the standard popup retains
its own native-size layout.

```rust,ignore
let window = ravet_webview::StandaloneWindow::new(
    ravet_webview::StandaloneOptions {
        title: "My RAVET instrument".into(),
        ..Default::default() // includes the standard settings popup
    },
)?;
let editor = window.embed(&trusted_html, callbacks)?;
editor.set_visible(true);
while window.pump(0.01) {}
drop(editor); // the parent window borrow prevents early window destruction
```

The default popup and semantic theme are injected before exactly one `</body>` in trusted local
HTML. The page must permit inline scripts/styles, stay within the 1 MiB document
bound and reserve the popup's DOM IDs (including `io-settings`, `audio-output`,
`audio-input`, `sample-rate`, `buffer-size`, `midi-inputs`, `start` and `stop`).
It wraps an existing inline `window.ravetUpdate` function after the page scripts.
Consumers must define that function before their closing body tag. An optional
`window.ravetSettingsWillOpen` hook can release on-screen notes before the dialog
opens. Escape, the close button and clicking outside close the modal popup.

The callback snapshot supplies this control-thread schema alongside application
state (`running` boolean and `status` string):

```json
{
  "running": false,
  "status": "Ready",
  "io": {
    "audio": true, "input": true, "midi": true,
    "outputs": [{"id":"device-id", "name":"Output", "channels":2,
      "rates":[44100,48000], "default_rate":48000, "buffers":[64,128,256,512]}],
    "inputs": [{"id":"input-id", "name":"Input", "channels":2}],
    "midis": [{"id":"midi-id", "name":"Keyboard"}],
    "note": "Stop audio to change settings."
  }
}
```

Input/MIDI rows hide when unavailable or explicitly disabled by the consumer.
Channel checklists group adjacent channels into pairs, with a mono final channel
when needed. MIDI inputs are independently selectable and initially unchecked.
Output rate options and buffer labels come from the consumer's supported-device
inventory; milliseconds are `1000 * frames / rate`, not measured latency. The
first device must be chosen explicitly when no saved available selection exists. New device selection resets its channel
selection to the first pair. Buffer zero means device default.

The popup sends JSON actions through the ordinary `Callbacks::message` bridge:
`{"type":"action","action":"refresh"}` and `stop`, or `start` with `output`,
`input` (empty string means no input device), `sample_rate`,
`buffer_size`, `output_pairs`, `input_pairs` and `midis`. Pair arrays contain
zero-based first-channel indices such as `[0,2]`; MIDI IDs remain strings.
Device controls disable while running; a successful start closes the popup.
Failed starts retain it and show the consumer's status. The consumer validates
all messages and owns enumeration, configuration, opening, stopping and routing;
the window does not open devices or grant device permissions. Synthetic/demo
sources belong to the application UI, not the device-input selector.

The standard popup saves device IDs, active channel pairs, sample rate, buffer
size and MIDI inputs when its selections change. Native macOS preferences use
the `org.ravet.standalone.settings` suite and the window title as a stable key;
windows with the same title share these settings. A versioned JSON selection
is bounded to 8 KiB. Only available exact device/MIDI IDs are restored; missing
devices are left unselected. Unsupported rate/buffer values are not inserted
into the current device's supported options. Restore never starts audio.
Hidden fixtures use process-local memory and do not write user preferences.
Explicit `io_settings: false` omits persistence along with the popup/button.
`window.ravetStartAudio()` starts with the current selections, or opens settings
when an output has not been selected, allowing an application Start/Stop button.

The [interactive examples](examples.md) implement this contract using CPAL and
bounded MIDI/control queues. Their hidden fixture checks native title-bar
activation, default injection, full opt-out, popup/channel/rate/buffer/MIDI
controls and all four example tabs without opening devices. The implementation
is currently macOS 12+ with Xcode command-line tools. This is a standalone window
prototype, not broader desktop/runtime or screen-reader support evidence.
