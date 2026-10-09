# 0014: Platform web editors behind the native C++ boundary

Status: Windows/Linux C++ prototypes implemented; native DAW validation pending.
The macOS Cargo binding and gain GUI remain scoped by [0013](0013-cargo-native-gain-editor.md).

## Context

Private consumers need equivalent packaged HTML controls on Windows and Linux.
Their DSP is portable, while browser embedding, parent handles and host UI loops
depend on the operating system. Product code and format SDKs stay outside RAVET.

## Decision

Retain the owned opaque C++ view API. CMake chooses Cocoa/WebKit, HWND/WebView2,
or GTK3 X11/WebKitGTK 4.1. Preserve local/inline page modes, finite backing
colours, bounded UTF-8 JSON, 50 ms snapshots, successful-delivery suppression,
one JavaScript evaluation in flight, restricted navigation and explicit cleanup.
The Windows bridge adapts the existing `window.webkit.messageHandlers.ravet`
page protocol to WebView2 messages at document creation.

Linux consumers pump GLib from the host UI loop using `ravet_web_pump`, with
at most 32 ready iterations per call. Format adapters own timer registration and
must reject attachment when no loop is available. XWayland uses the X11 path;
native Wayland handles and existing Wayland GTK contexts are not supported.

A process-wide GDK event dispatcher may belong to a non-GTK host. For loaded
SWELL providers, request their GTK initialization capability before constructing
widgets; do not initialize GTK behind their back. Resolve either a global API
or the already loaded local `libSwell.so`, without loading a new host library.
Decline editor creation when the provider cannot initialize GTK cooperatively.
A native compatibility shim verifies preserved host event delivery before and
after creation/destruction and safe rejection when the capability is absent.
This is a toolkit integration contract, not a DAW compatibility claim.

The Linux web widget explicitly expands to fill its container; protocol delivery
alone does not demonstrate usable editor geometry. Check the page viewport and
actual GTK paint output. A compile-time `RAVET_WEB_LINUX_SOFTWARE_RENDERING`
option (framework default OFF) selects non-accelerated painting per web view
before loading its document, for GPU/GBM paths that cannot allocate or paint.
Consumers choose this policy; it leaves other host views and the browser sandbox
unchanged. UI CPU cost needs separate measurement from DSP and correctness.

Windows consumers supply an STA message loop and matching WebView2 loader/runtime.
Browser/controller creation is asynchronous. Linux/Windows bind their thread
check on first creation, so the caller must select the host UI thread. Native
dependencies and all browser work remain outside portable DSP and audio callbacks.
The Rust ownership wrapper is still macOS only; these C++ implementations do
not advertise a portable Rust GUI API or CLAP GUI support on those platforms.

The macOS editor WKWebView subclass accepts first mouse events so the activating
press can begin a control drag in an inactive host window. Leave dispatch to
AppKit/WebKit rather than replaying events in JavaScript or changing host-window
focus policy. A native two-window probe verifies the first begin/change/end
sequence; this does not establish routing behavior in every DAW wrapper.

## Consequences and evidence

WebView2 requires its SDK redistribution notices and an installed Microsoft
runtime. Linux links GTK3, WebKitGTK and JSON-GLib from distribution packages.
The backend does not sandbox arbitrary page scripts; consumers supply trusted
HTML and a suitable CSP. It declines unsupported display/runtime configurations.

The portable lifecycle probe passed under local Ubuntu 24.04 arm64 and x64
virtualization/emulation, including malformed snapshots, navigation, coalescing,
reattachment, callback-triggered destruction and stopped callbacks. Windows x64
backend/probe compile with MinGW; graphical Windows execution remains pending.
The macOS lifecycle probe provides separate regression evidence. No DAW,
Omarchy, native heap bound, render deadline or production support is inferred
from these results. See [the guide](../native-editor.md) and
[work status](../WORK_STATUS.md).
