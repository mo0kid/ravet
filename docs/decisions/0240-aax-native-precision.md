# 0240 — Explicit native precision behind AAX float audio

Status: accepted contract; local implementation reviewed. Public source
adoption and subsequent qualification are tracked in WORK_STATUS.md.

The public AAX native callback exposes planar float audio. RAVET's originating
native processor/state/cache entries can describe f32 or f64 DSP. Passing the
host's pointer arrays to a native64 entry would violate its typed ABI; selecting
an export implicitly would also weaken codec/schema provenance.

Append an explicit default 32-bit precision argument to the authored owner/model and
editor. Require matching exact native core, audio companion cache and codec;
keep host context/audio fields, descriptors, IDs and f32 control/state scalar
products unchanged. Native64 owners are heap-resident and retain fixed 1 MiB
CONTROL-created scratch. The bridge captures all host input, processes through
one authoritative native64 owner with the original bus/event/state/transport
semantics, validates every output conversion, then copies host float output.
Default 32-bit retains its direct path.

Use integer IEEE storage conversion with exact finite widening, quiet NaNs and
software nearest/even finite narrowing with gradual underflow. Propagate native
infinities/NaNs rather than inventing a finite signal gate. A finite rounded
binary32 overflow rejects before any host sample or generated MIDI publication.
It occurs after DSP and queue consumption and poisons readiness without history
rollback. Untimed complete stopped preparation may recover accepted intent;
timed sticky faults require a new correctly reconciled instance even after
CONTROL old-slot retirement. Retain original actually returned native codes
separately from local conversion, unreturned exception and SDK delivery status.

Explicit 64-bit local bundle selection uses separate build paths and originating
64-bit factories. Stock 64-bit rejects before commands. Optional Gain editor metadata
uses the selected native cache. No external SDK source is copied or vendored.

Normal/fast-math target fixtures exercise actual native effects, input-none
instruments, source-ordered auxiliary routing, generated short MIDI, dynamic
ACK-controlled timing and initialized editor/model control. Independent bit and
phase/history oracles distinguish preflight rejection from late output failure;
maximum prepared conversion cycles measure zero ordinary C++ allocations.
Actual selected64 Gain 48/editor 51/Performance 4 and default32 Gain 32/editor 35/
Performance 4/stock 14 local packages pass their CMake and actual module contracts;
12 Python pipeline tests, export/version probes, notices and ad-hoc signatures
pass. Canonical public Rust archives are used; source adoption/shared gates
remain separately tracked.
These fixtures do not establish Pro Tools/signing, host-native64 buffers, all
foreign allocations, other platforms or a real-time deadline.
