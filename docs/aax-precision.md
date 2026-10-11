# Internal native precision in AAX

The local macOS AAX adapters keep the public SDK's planar `float**` audio
contract. `NativeAaxProcessor` and `NativeAaxEffectParameters` can explicitly
select native 32- or 64-bit processing. Selection requires a matching processor
entry, originating audio-state companion/cache and complete-state codec. An
available export never selects precision implicitly. Parameter values, normalized
controls, IDs and complete-state scalar values remain f32.

`NativeSampleBits` is the final template argument and defaults to 32. Existing
context layouts and the Gain/Performance public descriptor IDs remain unchanged.
The 64-bit owner uses the same authoritative native processor, events, state,
transport and source-ordered main/aux bus semantics. It does not duplicate DSP in
C++ or reinterpret host buffers as doubles.

## Prepared storage and ownership

A native64 owner must be heap-resident. CONTROL construction reserves fixed
scratch for 64 input and 64 output channels, each with 1,024 double samples
(1 MiB of sample storage), plus fixed bus and pointer metadata. There is no
callback allocation, resizing, shared scratch or separately owned DSP. Default 32-bit
uses empty precision storage and keeps the direct native32 processing path.

Stop/join AUDIO before prepare, reset, deactivate, changing the prepared layout,
or destroying the owner. Supported control/state queues retain their existing
SPSC contract. Generated-output and timed profiles retain their atomic callback
admission, reentry rejection and stopped diagnostic-copy rules. A plain effect
or instrument still requires exclusive AUDIO ownership; native callbacks must
not recursively render the same owner. Do not stack-allocate native64 owners or
models. Native tables and all borrowed host regions must remain live for their
complete declared lifetime.

The existing AAX mapper validates whole shapes, ranges, metadata, protected owner
storage, aliases and source order before any native state/history transfer. Its
limits remain one main plus eight auxiliary buses, 64 total channels per
direction and 1,024 frames. Native64 then captures every input into separate
scratch before writing host output. Input/output and cross-channel aliases are
safe under the existing mapper contract; partially overlapping output channels
remain rejected. Input-none instruments do not invent an input buffer.

## Conversion and failure behavior

`AaxIeeePrecision` observes IEEE storage bits through `memcpy`. Finite binary32
samples widen exactly, including subnormals and signed zero. Infinities retain
sign; NaNs become quiet and retain the representable sign/payload. Every host
sample bit pattern is accepted. The adapter does not add a finite-audio gate,
change FE/FTZ/DAZ, or sanitize the native processor's signal behavior.

Finite double output narrows with software round-to-nearest, ties-to-even and
gradual underflow, independently of the caller's rounding mode and compiler
fast-math assumptions. NaNs quiet; infinities and signed zero remain valid
outputs. A finite value whose correctly rounded result overflows binary32 is a
conversion fault. This differs from an infinity that the processor actually
produced.

Whole structural/input preflight precedes apply-state, dequeue, timing adoption
and native history. Data-dependent output overflow is discovered **after** the
single authoritative native DSP call. The adapter validates every output word
before copying any host sample or posting generated MIDI. On overflow it
silences the supported declared output prefix, preserves backing tails, poisons
readiness and publishes no generated events. Native history and accepted queue
consumption have already advanced; there is no rollback claim.

A fresh complete stopped prepare can recover an untimed owner using its accepted
complete intent. A timed owner's sticky fault survives old-slot CONTROL
retirement and rejects prepare/reset/Initialize before foreign calls. Recovery
requires a fresh correctly configured instance and host latency reconciliation.
No callback destroys a retired timing processor.

`precisionDiagnosticsStopped()` requires stop/join and reports local preflight,
narrowing or exception error separately from state/process attempt and return
flags, original native codes, queue consumption and sample transfer. A code is
meaningful for that block only if its returned flag is true.
`lastNativeProcessStatus()` is the lock-free last actually returned native DSP
code; a later local rejection or unreturned throw does not replace it. Native
refusal codes remain exact. Defensive C++ exception containment poisons the 64-bit
owner and silences supported output; it does not authorize unwinding across C
or Rust ABI boundaries. Actual Rust panic containment remains authoritative.

Generated-output validation/delivery and returned SDK statuses remain separate
from native/conversion results. A successful native process never proves host
MIDI delivery. Timing candidates migrate complete global state after a matching
ACK/control getter; precision does not alter the existing zero-history candidate
policy or create a host tail API.

## Local reference selection

```sh
python3 scripts/bundle-aax.py --internal-precision 64 \
  --vst3-sdk /path/to/external/vst3 --aax-sdk /path/to/external/AAX
python3 scripts/bundle-aax.py --plugin performance --internal-precision 64 \
  --vst3-sdk /path/to/external/vst3 --aax-sdk /path/to/external/AAX
python3 scripts/bundle-aax.py --editor --internal-precision 64 \
  --vst3-sdk /path/to/external/vst3 --aax-sdk /path/to/external/AAX
```

The default is 32. Explicit 64-bit Gain, Performance and optional Gain editor use
separate `target/aax/native64/release`, `performance64/release` and
`editor64/release` directories. `--profile stock --internal-precision 64` rejects
before build commands. Direct CMake selection uses
`-DRAVET_AAX_INTERNAL_PRECISION=64`; supported values are exactly 32/64. The
optional Gain editor uses the selected cache precision while keeping its f32
control schema. Build outputs remain local; packaging includes applicable
notices and an ad-hoc local signature, without host installation or PACE signing.

## Evidence and limits

Target-only implementation evidence includes independent integer conversion
goldens under four rounding modes, randomized hardware oracles, normal and
fast-math actual native Gain/BusRouting/Performance/NoteRouting/DelayRouting
consumers and initialized SDK/editor fixtures. Maximum prepared conversion cycles
exercise 64 channels/1,024 frames with zero ordinary C++ new/delete during the
measured phase. That measurement covers the prepared bridge, not all foreign
SDK/native allocations or a real-time deadline.

Actual callback fixtures separately prove native refusal, throws before/after
DSP, late last-sample overflow, complete intent retention, no partial sample/MIDI
publication, exact returned status, generated-output history and ACK-controlled
Delay8-to-Delay16 retirement. Fresh target-source packages pass selected64 Gain 48, editor 51 and Performance 4
CMake contracts plus all actual native/GUI module checks. Unchanged default32
Gain 32, editor 35, Performance 4 and stock 14 packages also pass. All use canonical
public native Rust archives; bundle export/version probes, notices and local
ad-hoc signing pass. Twelve Python pipeline tests pass. These receipts qualify
the target implementation; public adoption and joint qualification are tracked
separately in work status.

This is a prototype using externally supplied SDKs, not native64 AAX host audio,
Pro Tools validation, production signing, other-platform support or a deadline
guarantee. The current instrument input policy remains bounded short notes;
full short-MIDI input, MIDI SysEx/long packets and auxiliary generated events
remain separately scoped. Public multibus/timing fixtures demonstrate reusable
mapping; the Gain/Performance distributed references retain their main-only
layouts and original public identities.
