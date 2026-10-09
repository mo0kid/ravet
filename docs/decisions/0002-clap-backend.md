# 0002 CLAP backend

Status: Clack selected for the gain feasibility prototype, 8 October 2026.

Use published Clack 0.2.0 packages to wrap the CLAP ABI while keeping RAVET's
portable processor independent of the format. A raw `clap-sys` implementation
would require us to maintain foreign callbacks, pointer translation, and
thread/lifetime wrappers ourselves. Clack supplies these boundaries without
replacing the existing processor. This decision is provisional until a DAW and
embedded editor exercise the same integration.

## Dependency boundary

`ravet-clap` depends on `ravet-foundation` and `clack-plugin`; it exposes a
prepared `EffectBridge<P: Processor>` and a bounded parameter-event translator.
The gain export depends on that bridge and the existing `ravet-gain` product.
The umbrella crate does not re-export the format adapter.

Exact selected packages are `clack-plugin`, `clack-common`, and
`clack-extensions` 0.2.0, with `clap-sys` 0.5.0 and `bitflags` 2.13.2 in the
plug-in's resolved normal graph. Clack and bitflags declare `MIT OR Apache-2.0`;
clap-sys declares the legacy `MIT/Apache-2.0` identifier. Extension features are limited to audio ports, port
configurations, parameters, and the subsequent gain state extension. The development host adds `clack-host` 0.2.0
and its native dynamic-loading dependencies. Cargo.lock pins the full graph;
third-party notices still belong in release packaging.

Primary references: [Clack](https://github.com/prokopyl/clack),
[CLAP specification](https://github.com/free-audio/clap),
[CLAP parameters](https://github.com/free-audio/clap/blob/main/include/clap/ext/params.h),
and [CLAP validator 0.4.1](https://github.com/free-audio/clap-validator/releases/tag/0.4.1).
The implementation uses the downloaded 0.2.0 package source rather than relying
on examples from other Clack versions.

## Audio and event translation

The gain product advertises one matched mono/stereo f32 main bus. Stereo is the
default; configuration changes happen while inactive. Input/output port IDs
are zero and identify each other as valid in-place pairs. f64, extra buses,
no-input instruments, notes, modulation, and output events are not advertised.
Transport is currently ignored; this bridge is for a transport-independent gain
effect and does not yet forward transport to portable processors.

Activation allocates two planar scratch channels and the event buffer. Each
callback copies host inputs into scratch, runs the original processor in place,
and copies the result to host outputs. This handles separate, in-place, and
mixed channel pairs through the same path. Two copies per channel are the
explicit cost of the first integration. Scratch is capped at 65,536 frames
(512 KiB of stereo f32 storage), and hosts asking for a larger maximum are
rejected during activation.

At most 1,024 total input events are accepted per callback/flush, including
ignored event kinds. The count is checked before calling event accessors.
Global core parameter events are converted to permanent u32 IDs and finite f32
physical values. Non-core namespaces, unknown event kinds, and scoped parameter
changes are ignored. Known core parameter events with truncated payloads or
invalid numbers reject the entire stream. Nonempty process offsets must be
strictly below the frame count, and all event timestamps must be ordered.
Zero-frame processing allows offset zero; parameter flush normalizes timestamps
to zero because CLAP gives them no sample meaning there.

Unsupported sample types, layout/size errors, and malformed event streams leave
processor state unchanged and silence all supplied active outputs. The export
returns `Continue` and records a shared atomic fault code rather than returning
`PluginError`: Clack logs that error at the foreign boundary, which would add
I/O to the callback. The diagnostic is internal, not a standard CLAP extension.
Valid blocks clear it. Invalid foreign pointers or host threading violations
remain outside the safe wrapper contract.

## Threading and lifetime

Clack constructs/destroys the audio processor on the main thread at
activation/deactivation and grants it exclusive access for process/reset/flush.
The processor holds prepared scratch and its mutable DSP state. Main-thread
parameter reads use one atomic 64-bit word containing requested gain and a restore
generation. Main-thread flush updates the gain only while inactive; active flush applies
events through the prepared processor on the audio thread. Reactivation
initializes the processor from the stored value and resets smoothing to it.
The subsequent [gain state prototype](../session-state.md) uses that generation
for coherent control-thread restore and audio-thread synchronization. This is a
product-specific protocol, not a reusable state layer or editor command queue.

Portable crates and the safe bridge forbid unsafe code. The export crate uses
`deny` with a narrowly allowed module for Clack's entry macro (one `clap_entry`
symbol). The development loader and raw test fixtures have individually
explained allowances. Clack owns foreign pointers, event views, callback
lifetimes, alias handling, and panic containment. RAVET prevents panics in its
processing paths; no arbitrary-host-corruption recovery is promised.

## Validation and next integration

Before the state extension, on macOS 27.0.1 arm64 with Rust/Cargo 1.99.0,
the optimized bundle passed 29
CLAP validator 0.4.1 checks with zero failures; 15 checks skip unsupported
features. The complete report is in `target/clap/release/validation.json`.
The validator executable came from the official arm64 release asset with
SHA-256 `719a0248ea431718bb7c92c8a0b9a78afa0bb23d692cf452e80973fe8b89282d`.

RAVET CLAP Smoke Host loads the built bundle, enumerates ports/parameters,
checks a known stereo/mono automation fixture, flushes stopped and active
parameters, resets, recalls gain state while active, and repeats six activation
cycles (18 checked audio blocks). Processing runs on a
separate thread. Integration tests repeat the same scenario with a safe
in-memory entry and measure 10,000 host ABI callbacks plus 10,000 in-place
bridge callbacks with zero Rust allocation/deallocation on the measured thread.
These checks do not measure callback deadline distributions.

`scripts/bundle-clap.py` builds into Cargo's target directory. macOS uses an
ad hoc signed bundle with Info.plist and the exported executable; Linux/Windows
paths produce a flat `.clap` file but remain untested. No global plug-in
installation, signing credentials, or publication is configured.

Next, implement an embedded editor experiment, validate the new gain state
extension, then exercise the bundle in a DAW. This prototype currently has no
editor, reusable state layer, host gestures, bypass, or meters. Revisit Clack if those
integrations require unsafe portable types, unbounded callback work, or host
behaviour its wrappers cannot represent.
