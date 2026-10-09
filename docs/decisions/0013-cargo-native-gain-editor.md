# 0013 — Cargo-owned macOS binding and embedded gain editor

Status: macOS prototype integrated; REAPER 7.82 validation completed within the
scope in [host validation](../host-validation.md). Broader support remains pending.
Continuous gestures and resizing extend this original design in
[0015](0015-continuous-editor-gestures.md) and
[0016](0016-attachments-and-resizing.md).

## Decision

Add an explicit `ravet-webview` crate that builds the existing Objective-C++
backend through Xcode and owns its view/callback lifetime. Keep it outside the
umbrella and portable processing graphs. Isolate native FFI inside this crate
and the CLAP parent-attachment module; portable unsafe policies remain intact.
Creation verifies the Cocoa main thread, and an Rc marker prevents sending or
sharing the view. Drop destroys native callback machinery before context release.

Add copied buffer snapshots for inline HTML alongside the existing local-file
C++ API. Callback entry retains Rust context and the native bridge holds itself
through polling. This permits owner destruction during a callback without
leaving borrowed snapshot storage dangling. Reentrant Rust callbacks are skipped
and panics do not unwind across FFI. Callback work remains on the main thread.

The gain export enables a default `native-editor` feature on macOS, with an
embedded Cocoa CLAP editor of fixed 480×390 logical size. Floating windows,
resizing, generic widget infrastructure and other platform backends are outside
this prototype. Positive scale hints defer to Cocoa's logical pixel scaling.
The host owns parent-view and main-loop lifetime. The page is compiled into the
plug-in and uses a restrictive CSP, avoiding installation/resource-root paths.

Route validated JSON edits to decision 0012's bounded committed-edit queue, and
serialize coherent requested state plus coalesced meters from main-thread
callbacks. Pin serde_json as a control-thread dependency, separate from DSP.
Slider edits commit on change; continuous live drag gestures remain pending.
Display peak/RMS per latest block without making a peak-retention or deadline claim.

## Evidence and consequences

The build requires Xcode command-line tools on macOS; native source targets
macOS 12, but compile flags do not establish an operating-system support matrix.
A GUI-free export remains available with `--no-default-features`. Workspace checks
still include the native crate. New packages inherit the existing unpublished GPL
metadata. No proprietary consumer assets or wrappers enter the framework.

The named `ravet-editor-smoke` host loads a built trusted bundle, owns a hidden
Cocoa parent window, drives page controls, checks host gestures/automation and
processed audio meters, hides/shows/destroys the editor, then processes again.
It can capture a PNG for visual review. The older native file probe remains a
regression gate. These are prototype integration checks, not DAW or device evidence.
See [native editor](../native-editor.md) and [work status](../WORK_STATUS.md).
