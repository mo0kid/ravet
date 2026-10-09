# 0021 — Shared Rust gain DSP behind an external VST3 SDK

Status: mono/stereo f32 VST3 reference prototype; broader format/UI mappings pending.

## Decision

Use the external installed VST3 SDK, with separate processor/edit-controller
classes, and a narrow Rust-owned gain ABI. Keep the processor/state codec shared
with CLAP and offline fixtures. Do not copy proprietary consumers or vendor SDKs
into RAVET. Build artifacts locally; disable SDK installation helpers and preserve
licence notices. The inspected/exercised SDK 3.8.1 supplies an MIT licence.

Preallocate event/audio storage at setup. Validate complete host queues and buffer
shapes before mutation; merge points with stable bounded insertion. Copy all
inputs before producing outputs to permit legal host aliases. Keep state stream
I/O on control threads; publish restores through a lock-free full word and reject
stale callback reflections. Native DSP ownership remains exclusive to serialized
processing/lifecycle calls, not main-thread state readers. Contain unwinding
panics without claiming exceptional real-time behavior.

## Consequences

This starts with a public gain prototype rather than asserting a generic format
backend. It introduces narrowly allowed unsafe native pointers in the example
ABI, outside portable crates. f64/auxiliary buses/event output, instruments,
custom editors and broader target/DAW validation are pending. Event insertion
has O(events squared) bounded work; deadlines are unmeasured. Restore generation
must not wrap across pending callbacks. Native SDK/host allocations are outside
Rust allocation-counter evidence.

## Evidence

Two Rust ABI tests include 10,000 allocation-free callback iterations. C++ contract
fixtures cover exact audio, fault atomicity, state/controller recall and buffer
aliases/tails. The built macOS arm64 bundle passes all 47 SDK validator checks.
See [VST3 guide](../vst3.md) and [work status](../WORK_STATUS.md).
