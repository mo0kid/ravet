# 0077: Optional Rust-owned VST3 gain editor

## Context

The native editor ABI needs an actual format controller consumer, with host
normalized gestures and state ownership remaining separate from portable DSP.

## Decision

Add an explicit default-disabled macOS IPlugView using the shared Rust-owned
WebView ABI. Compile a local public gain page using reusable surface controls;
keep JSON validation/physical normalization/snapshot encoding in Rust UI codecs.
SDK C++ owns the format interface, normalized begin/perform/end callbacks and
controller updates. It does not bypass host event queues to mutate processor DSP.

Retain controller/view lifetime during native callbacks. Host refresh produces
no edits. Rejected performEdit rolls back the controller snapshot. Rejected end
retains gesture ownership, retries on UI snapshots and rejects remove until it
succeeds. Forced final destruction can only attempt another end; it cannot make
a rejecting host accept. Creation/attachment/removal/teardown are host UI-thread
obligations, with live NSView parent ownership. No implicit window/run loop.

## Consequences and validation

Default DSP-only bundling remains available; native-editor requires macOS and
adds surface/JSON/WebKit dependencies to the reference static library. The page
contains gain/bypass/status, with meter delivery remaining separate format work.
Other-format and other-platform editor mappings are not established by this view.

Rust codecs test schema commands, normalization, malformed-output preservation,
JSON status bounds and compiled page embedding. SDK processor/state/f64 contracts
and all 47 validator tests pass. An actual dynamically loaded exported module
passes three native Cocoa editor cycles, reattachment, gain/bypass gestures,
held keys, host refresh, rejected values/end retry and size checks. Screenshot
inspection confirms readable controls. No REAPER/focus/assistive-tech claim is
inferred from programmatic synthetic-host evidence.
