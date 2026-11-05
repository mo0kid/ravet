# 0287: Export robustness, opt-in editor and bundle notices

**Date:** 5 November 2026  
**Status:** Implemented for the 0.1.0 plug-in export surface (prototype
features). Offline and format-validator evidence on macOS arm64 only.

## Context

The 0.1.0 release plan requires a review of non-test `panic!`, `unwrap()` and
`unsafe` in the supported surface, a review of heavy dependencies for opt-in
features, and a dependency-notice audit for both licensing routes
([LICENSING.md](../../LICENSING.md)). The export layer runs inside third-party
hosts, so a panic, a C++ exception or undefined behaviour at a host call is a
host crash, and an allocation on the audio thread is a real-time fault.

## Decision

- **Boundaries never unwind.** Every `ravet_plugin_v1` and `ravet-native`
  table function catches panics and returns its documented error; CLAP
  callbacks are contained by Clack. Every C++ adapter entry point that can
  allocate or call into the table catches exceptions and returns the format's
  error (`kResultFalse`/`kOutOfMemory`, `AAX_ERROR_UNKNOWN_EXCEPTION`, null
  descriptors, silenced AU renders). This relies on the default
  `panic = "unwind"`; a product built with `panic = "abort"` turns any panic
  into a host crash.
- **Audio paths reject instead of panicking or allocating.** Invariants that
  were `expect`/`unwrap` on processing paths become fallible matches that fault
  the block with silence. The CLAP export rejects a block with more than 1,024
  events, as the native table already did, rather than growing its event
  buffer on the audio thread; it copies only core events no larger than the
  largest standard event.
- **Host pointers are checked before they are cast.** Table functions reject
  null and misaligned instance handles, which no `create` returns, instead of
  dereferencing them.
- **Every library `unsafe` block states why it is sound**, enforced with
  `#![deny(clippy::undocumented_unsafe_blocks)]` in `ravet-plugin`,
  `ravet-native` and `ravet-native-dsp`.
- **The web editor is opt-in.** `ravet-plugin` gains an `editor` feature
  (umbrella `plugin-editor`); `clap` and `native` no longer pull in
  `ravet-webview`. Without it a product offers no editor: no CLAP GUI
  extension, a 0 × 0 editor in the table. The examples enable it. `sha2`
  (run-time format identities) and `serde_json` (the editor protocol and the
  `PluginExport::editor_message` signature) stay required.
- **Bundles ship notices.** `scripts/third_party_notices.py` lists every
  third-party crate linked into the product for the target being built, with
  the full licence files from each crate's archive, plus RAVET's licence and
  the Rust standard library notice. Every bundle script writes it beside the
  SDK notices; see [dependency notices](../dependency-notices.md).

## Consequences

Products that relied on `clap`/`native` to bring in their editor must add the
`editor` feature (a prototype-API change, listed for the 0.1.0 changelog).
Products without an editor no longer compile WebKit/Cocoa bindings.
Distributors still decide what their installers ship; the generated notices
are notice material, not legal advice. `ravet-webview`'s own unsafe and
Objective-C++ code, and the standalone runner, are outside this review.
