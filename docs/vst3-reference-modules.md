# Reference VST3 modules for hosted-VST3 tests

RAVET builds two small VST3 modules whose only job is to let the hosted-VST3
stack ([vst3-hosting.md](vst3-hosting.md)) run against real loaded modules. They
cover two paths that otherwise have only mocks and hand-written fixtures:
auxiliary buses, and the full rich-event set through the Rust Graph bank.
Expected output follows from closed forms, so tests compare against independent
references rather than recorded output. They are validation references, not
production dynamics processors or instruments. See
[ADR 0274](decisions/0274-reference-vst3-modules.md).

| Module | Bundle | Processing |
|---|---|---|
| RAVET Keyed Sidechain | `ravet_sidechain_vst3.vst3` (`org.ravet.sidechain.vst3`) | Stereo main in/out plus one auxiliary sidechain input, mono or stereo. Output is `main * key`. `key` is the mono key sample, the stereo mean `(left + right) * 0.5`, or exactly 1 when the auxiliary bus is inactive. Input channels flagged in `silenceFlags` are read as exact zeros, whatever their buffers hold. Output `silenceFlags` are always cleared. Other auxiliary arrangements are refused. |
| RAVET Event Probe | `ravet_event_probe_vst3.vst3` (`org.ravet.event-probe.vst3`) | Stereo instrument output, one 16-channel event input and one event output. Each admitted bus-0 input event of the nine SDK input kinds (note on/off, data, poly pressure, note expression value, note expression text, chord, scale, note expression int), and each point of the `Marker` parameter, adds an impulse at its exact sample offset. The impulse is `code + 1` on the left channel and `hash20 * 2^-20` on the right. Each admitted input event is echoed unchanged to the event output. When at least one event was echoed, one legacy MIDI CC output follows (controller 16, channel 0, at the latest echoed offset). It carries the number of echoed events in 14 bits across `value`/`value2`; Marker points are not counted. The read-only `Digest` parameter reports a running 20-bit digest over every record, including zero-frame blocks, at the last frame of every block. |

`hash20` is a folded 20-bit FNV-1a hash of a canonical serialization of the
record. It covers bus, flags, PPQ position and every payload field, including
data bytes and UTF-16 text. It excludes the sample offset, which the impulse
position encodes instead. The serialization is documented in the
`ravet_vst3_reference_native::record_hash` Rustdoc and in
`examples/vst3-reference-native/include/RavetVst3Reference.h`. Rendering is
exact in f64 for every admitted block. In f32 it is exact while at most 16
records share one sample offset; the tests stay within that bound.

Records that share an offset are summed. The left-channel sum alone therefore
does not identify them: a note-on plus a note-off (`1 + 2`) reads like one data
event (`3`). A decoder must either know the candidate records, as the tests do,
or place records at distinct offsets.

## Admission and refusal

The Event Probe admits, per block, at most 2048 input events and at most
`4096 - events` Marker points. Event payloads share `65536 + 4096 + 16` bytes of
module storage: data bytes and UTF-16 text plus a terminator, each 2-aligned.
Events on buses other than 0, and unknown or output-only event kinds, are
ignored before their offset is examined. Parameter queues other than `Marker`
are ignored unread.

Anything else malformed refuses the whole block. That covers:

- event or point counts that are negative or over capacity;
- offsets outside `0..frames-1`, or nonzero in a zero-frame block;
- a null payload with a nonzero size, or payload overflow;
- a failed `getEvent` or `getPoint`, or a null queue;
- null bus or channel arrays, a wrong channel count, frames outside
  `0..maxSamplesPerBlock`, or a mismatched sample size.

A refused block returns `kResultFalse`, zeroes every reachable output channel,
posts no events or parameter points, and leaves the digest unchanged.

One exception is deliberate: the digest advances in the render, before output
events and the `Digest` point are posted. If the host's output lists refuse
`addEvent` or `addPoint`, the block returns `kResultFalse` with the digest
already advanced, and the events posted so far stay in place.

The Keyed Sidechain refuses the same malformed-buffer cases, a missing key bus
when the auxiliary bus is active, and a key channel count that differs from the
arrangement. It also refuses `setActive(true)` before `initialize`.

## Layout

- `examples/vst3-reference-native` (workspace crate
  `ravet-vst3-reference-native`): portable Foundation processors `KeyedGain` and
  `EventProbe` for native f32 and f64, plus the C ABI in
  `include/RavetVst3Reference.h`. Preparation allocates; processing and reset
  make no Rust heap operations. Two sets of unit tests cover it:
  - `src/tests.rs` checks the keyed-gain closed form for every key layout,
    layout refusal, the record hash against an independent serialization and a
    known FNV-1a vector, out-of-range offsets and zero-frame blocks.
  - `src/abi/tests.rs` calls every exported entry point directly. It covers
    valid input, null handles and pointer arrays, null channels, overlapping
    outputs, out-of-range frames, counts, masks and offsets, unknown codes,
    unaligned text, the wrong-precision entry point, and sequencing (process
    before prepare, failed re-prepare, reset, re-prepare). Every process and
    reset call is measured with `allocation-counter`.
  - The test-only `allocation-probe` feature installs a counting global
    allocator in the staticlib and exports
    `ravet_vst3_reference_allocation_count`.
- `examples/vst3-reference`: external-SDK C++ wrappers (`src/sidechain.cpp`,
  `src/event_probe.cpp`) and a CMake project that follows the public Gain VST3
  build.
- `crates/ravet-host/native/vst3/reference-module-tests.cpp`: actual-module
  tests through the native hosted-audio provider, built as
  `ravet-vst3-reference-module-normal-tests` and
  `ravet-vst3-reference-module-fast-harness-tests`. The fast-harness profile
  compiles only the test harness with `-ffast-math`; the modules are built
  normally.
- `crates/ravet-host/native/vst3/reference-robustness-tests.cpp`: loads each
  bundle directly through the SDK and drives `IAudioProcessor::process` with
  malformed and over-capacity data that the provider never sends, as
  `ravet-vst3-reference-robustness-tests`.
- `crates/ravet-host/examples/vst3-reference-bank.rs` (staticlib, needs
  `vst3-hosted-audio`) and `native/vst3/reference-bank-main.c`: the Rust
  hosted-audio Graph bank over the same modules, as
  `ravet-vst3-reference-bank-tests`.

## Build and test

The SDK stays external; nothing is installed, registered or linked into a
plug-in folder. Bundles are ad-hoc signed in place inside the build tree.
The native tests read the in-module Rust allocation counter, so build the
archive with `allocation-probe`:

```bash
cargo build --release -p ravet-vst3-reference-native --features allocation-probe
cmake -S examples/vst3-reference -B target/vst3-reference \
  -DCMAKE_BUILD_TYPE=Release -DRAVET_VST3_SDK=/path/to/vst3sdk \
  -DRAVET_VST3_REFERENCE_LIBRARY=$PWD/target/release/libravet_vst3_reference_native.a
cmake --build target/vst3-reference --parallel 2
ctest --test-dir target/vst3-reference --output-on-failure

cargo build --release -p ravet-host --features vst3-hosted-audio \
  --example vst3-reference-bank
M=target/vst3-reference/VST3/Release
cmake -S crates/ravet-host/native/vst3 -B target/vst3-host -DBUILD_TESTING=ON \
  -DCMAKE_BUILD_TYPE=Release -DRAVET_VST3_SDK=/path/to/vst3sdk \
  "-DRAVET_VST3_REFERENCE_MODULES=$PWD/$M/ravet_sidechain_vst3.vst3;$PWD/$M/ravet_event_probe_vst3.vst3" \
  -DRAVET_VST3_REFERENCE_BANK_LIBRARY=$PWD/target/release/examples/libvst3_reference_bank.a
cmake --build target/vst3-host --parallel 2
ctest --test-dir target/vst3-host -R reference --output-on-failure
```

The reference project forces `SMTG_CREATE_PLUGIN_LINK` off and
`SMTG_PLUGIN_TARGET_USER_PATH` empty, and fails configuration otherwise. For
each bundle it registers three tests:

- a strict ad-hoc signature check (`Signature=adhoc`);
- a no-install check: the bundle is under the build tree, no `cmake_install`
  rule names it, and neither default VST3 folder (user or system) holds a copy
  or link;
- the SDK `validator`.

Without `RAVET_VST3_REFERENCE_MODULES`, the native host project still compiles
the reference test executables, registers no tests for them, and prints a
configure-time status message. Without `RAVET_VST3_REFERENCE_BANK_LIBRARY` it
skips the Rust bank executable.

## What the tests cover

Native provider (normal and fast-harness profiles):

- Keyed Sidechain in native32 and native64 with the auxiliary bus active as
  stereo, active as mono, inactive declared as stereo, and inactive declared as
  mono. Each case runs 2000 blocks of 0 to 64 frames, including zero-frame
  blocks, and every output sample must equal the closed form exactly. A 5.1
  auxiliary request must be refused with the module's returned `kResultFalse`.
- Event Probe in native32 and native64 with 2000 variable and zero-frame
  blocks. Every block carries all nine input event kinds plus `Marker` points.
  Marker point 0 takes an offset no event uses where one is free, so most
  blocks show a lone NoteOn impulse. Each impulse, each echoed event payload,
  the count CC and the `Digest` parameter must match an independent FNV-1a
  implementation in the test file.
- On the AUDIO worker, around every process call, the test counts C++
  `operator new`/`delete` and reads the module's Rust allocation counter.
  Both must stay at zero.

Direct SDK robustness (`ravet-vst3-reference-robustness-tests`, f32 and f64):

- Event Probe, refused blocks:
  - 2049 events, a negative event count and a failed `getEvent`;
  - 4097 points, 1 event plus 4096 points, and 2048 events plus 2049 points;
  - a negative point count, a failed `getPoint`, a null queue and a negative
    queue count;
  - a payload one byte over capacity, a combined payload overflow, and
    over-long text;
  - null data and null text with nonzero sizes;
  - negative, at-frames, past-frames and minimum-integer offsets for events
    and points, and nonzero offsets in zero-frame blocks;
  - null outputs, no output bus, a null channel array, a null left or right
    channel, a mono output, frames above the maximum, negative frames, a
    sample-size mismatch, and processing before activation.

  Each refusal must be silent, post nothing, and leave the digest unchanged:
  a twin instance that never saw the block must agree on the next good block.
- Event Probe, admitted blocks:
  - capacity edges: 2048 events plus 2048 points, 4096 points, and a payload
    of exactly the capacity;
  - empty payloads, and events on other buses or of unknown or output-only
    kinds, which must be ignored even with invalid offsets;
  - a malformed non-Marker queue, which must be ignored;
  - an empty event list, a null event list, a parameter-only block, null
    output lists, and a zero-frame block with no output bus.
- Event Probe, pinned behaviours: the documented host-output-refusal exception,
  and the digest reset on reactivation.
- Keyed Sidechain with 0, 1 and 2 key channels:
  - silence-flag combinations on main and key inputs with garbage in the
    flagged buffers;
  - every malformed-buffer refusal;
  - zero-frame blocks with no buffers;
  - `setActive(true)` before `initialize`.
- Every call, refused or not, reads the module's Rust allocation counter and
  requires zero.

Rust Graph bank (`vst3-hosted-audio`):

- Keyed Sidechain bank with three children (stereo active, mono active, stereo
  inactive) placed in a canonical `Graph`, in f32 and f64.
- Event Probe bank whose rich lanes carry all nine event kinds and `Marker`
  points. The test checks impulses, echoed output events, the count CC and the
  `Digest` parameter against closed forms.
- Each run processes 600 Graph blocks of 0 to 64 frames on one AUDIO worker.
  `allocation-counter` measures every `Graph::process` call (host-side Rust
  heap) and must see zero heap operations. Nodes are returned and the bank is
  reclaimed on MAIN.

## Limits

- Local macOS arm64 evidence only. The CMake project refuses non-Apple hosts.
- The SDK validator, an ad-hoc signature, a direct SDK harness, and RAVET's own
  provider and Rust hosts are the only hosts exercised. No DAW, third-party
  host or other platform is claimed.
- Allocation evidence has three parts:
  - Rust heap operations inside each loaded module (allocation-probe counter,
    every process call, all native tests);
  - C++ `operator new`/`delete` on the provider test's AUDIO worker;
  - host-side Rust heap in `Graph::process`.

  Direct `malloc` calls by the C++ wrappers or SDK are not counted. Whether
  the harness counter also sees C++ `new` calls made inside a loaded module
  has not been verified. No real-time deadline is claimed.
- The `allocation-probe` feature is for tests only. It installs a global
  allocator in the staticlib and exports one extra symbol.
- The `ravet_*` C symbols are exported from the crate's rlib as well. Linking
  it next to another archive that exports the same names would clash.
- Keyed Sidechain accepts one auxiliary input bus, mono or stereo, only.
- No state, editor, presets, bypass or latency reporting. Neither module is a
  public plug-in product.
