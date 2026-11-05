# 0292: Web view robustness and system-component notices

**Date:** 5 November 2026  
**Status:** Implemented for `ravet-webview`, the `ravet-surface` native web
views and the standalone window code (prototype features). Runtime evidence
on macOS arm64; Linux x86_64 build, unit and native CTest evidence; Windows
cross-compile evidence only.

## Context

[Decision 0287](0287-export-robustness-and-notices.md) reviewed the plug-in
export crates and C++ adapters but left out `ravet-webview`, its
Objective-C++/C++ web views and the standalone window. Those run inside host
UI threads: an Objective-C or C++ exception that reaches Rust or a host frame,
a callback after destruction, or a dangling native pointer crashes the host.
The notice audit likewise covered only the export graph, not WebView2,
WebKitGTK, the device crates or the Windows C runtime.

## Decision

- **Page content cannot raise inside WebKit.** Message bodies that JSON cannot
  encode (a `Date`, `NaN`) made `dataWithJSONObject:` raise
  `NSInvalidArgumentException` and end the process. The editor bridge and the
  standalone settings handler now check `isValidJSONObject:` and drop such
  bodies.
- **No exception crosses the native C ABI.** The allocating web-view entry
  points (`ravet_web_create`, `ravet_web_create_html`, `ravet_web_eval`), the
  standalone `create`/`eval` entry points, the WebView2 COM handlers, the
  Win32 window procedures and the GLib callbacks are `noexcept`. An allocation
  failure there terminates the process deterministically, as a Rust
  allocation failure does, instead of unwinding through C, COM or Rust frames.
  Catching and returning an error was rejected: a partly created view would
  keep timers and handlers that call the caller's freed context.
- **Wrong-thread calls are refused, not asserted.** `NSCAssert` at the Cocoa
  C ABI raised an exception into Rust; those entry points now return their
  failure value off the main thread. The Windows and Linux UI-thread ids are
  atomic.
- **Callbacks may destroy their editor.** The Cocoa bridge keeps itself alive
  for the length of a script-message callback, as it already did for
  snapshots. Each WebView2 handler holds a reference to itself through
  `Invoke`. The WebView2 loader DLL is no longer freed when a view's state is
  released, because that release can happen inside a WebView2 callback.
- **The standalone owner exists before the last error path.** `embed`
  constructs its `StandaloneEditor` before the final closed-window check, so
  the error path clears the Windows/Linux window's editor pointer instead of
  leaving it dangling.
- **Rust boundaries check foreign pointers.** Null context, message and
  destination pointers are refused before any dereference; the C editor ABI
  rejects misaligned handles. The accessibility snapshot's string conversions
  return `InvalidInput` rather than `expect`. `ravet-webview` denies
  `clippy::undocumented_unsafe_blocks`.
- **Bundles ship system-component notices.** `scripts/third_party_notices.py`
  copies the WebView2 SDK `LICENSE.txt` and `NOTICE.txt` for Windows products
  that link `ravet-webview`, and writes `SYSTEM-LIBRARIES-LGPL.txt` (with the
  build machine's LGPL 2.1 text) for Linux products that link `ravet-webview`
  or `alsa-sys`. Crates whose archive has no licence file but whose declared
  licence offers MIT get a reconstructed MIT notice, and a reminder is printed.
  See [dependency notices](../dependency-notices.md).

## Consequences

The web-view header's three allocating declarations gain `noexcept`; C++
callers are unaffected, and C callers never saw exception specifications. A
process that runs out of memory inside a native web-view call now terminates
instead of continuing in an undefined state. Wrong-thread calls to the Cocoa
C ABI are ignored in release and debug builds alike. The WebView2 loader stays
loaded for the life of the process once an editor has been created.

The device crates used by standalone applications (`ravet-device`,
`ravet-midi-device`, `ravet-core-midi`) were audited for notices but not
reviewed for panics or unsafe code; that review remains open. Windows runtime
behaviour of the changes is unverified (compile evidence only). The generated
notices are notice material, not legal advice.
