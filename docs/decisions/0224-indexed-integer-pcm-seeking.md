# 0224: Indexed integer PCM seeking from finite history

Status: accepted implementation design; target-only validation complete, public
promotion and grouped workspace validation pending.

## Context

Integer scheduling permits bounded-memory long-file conversion, but the existing
converted pump rebuilds a stopped seek by replaying from frame zero. That preserves
full-prefix validation while its work grows with the output target. Known-duration
WAV/AIFF already expose original-frame positioning. A finite causal FIR needs only
its last taps samples and exact rational phase to reproduce the next output.

## Decision

Expose immutable integer resampling positions and explicit finite-history priming.
For output J and integer rates I/O, C = 1 + ceil(JI/O), computed in u128. The relative
phase is zero when JI mod O is zero, otherwise (JI mod O) − O. Require exactly taps
finite chronological samples ending at C−1, with invalid requests preserving state.
This reconstructs the existing coefficient table and fractional causal delay; it
does not replace the filter, trim its tail or change floating/live schedules.

Add a separate `IndexedPlaybackReader` contract and `IndexedConvertedReadAhead`
adapter, restricted to known duration and explicit IntegerSinc. Built-in indexed
readers are WAV/AIFF only. Each stopped seek validates a fresh same-metadata source
candidate, reads [max(C−taps,0), min(C,N)), supplies explicit boundary zeros and
primes every channel. At the exact retained endpoint, validate the last original
window and EOF without priming an output that will never be emitted. Known empty
PCM is supported only by this new pump; old empty rejection stays unchanged.

Keep candidate recovery and original errors explicit. Commit only after all
validation succeeds; rejected candidates leave the old queue, suffix, phase,
reader, cursor and generation intact. No restoration retry is hidden. Preserve
existing `ReadAheadPump`/worker composition and unique consumer ownership. Direct
indexed seek recovers a failed reader; the generic trait releases that candidate
on control when returning only its original error.

Use separate indexed counters. A skipped source prefix or mathematical phase must
never be labelled actual source reads/submission. Report the validated window,
original cursor movement (including error commits), post-prime submission and
output generation/publication separately. Endpoint receipts have no next-output
filter coordinate. u128 input coordinates may exceed u64 only for explicit padding.

## Limits and evidence

A candidate validates at most taps original frames plus one EOF probe. Short-read
custom readers may require up to taps positive calls; full-read built-in PCM
readers use ceil(window/read batch). Metadata parsing and underlying backend work
remain separate. Temporary scratch is <=66 KiB; candidate arrays remain <=32 MiB,
with old/candidate states coexisting and reader/ring/allocator memory separate.
Existing configured bounds and retained output <=u64::MAX−65536 remain.

Target-only tests compare exact full suffixes over six rational/limit signal
profiles, ordinary WAV/AIFF, first/last/tiny/empty/tail endpoints and arbitrary
batches. Instrumented actual PCM containers prove payload work independent of
seek target, including a generated 16,777,249-frame RIFF read/seek source. Synthetic
near-u64 padded coordinates test checked arithmetic without claiming such a real
container. Seek/read/EOF/metadata/nonfinite failures retain original candidate
errors and old continuation. Worker and actual PreparedOutput tests cover stopped
recovery/reprepare, pause, exact suffix/EOF and heap-free callback rendering.

The fresh-reader factory owns same-version content. Validating a window and EOF
does not authenticate skipped regions. No compressed indexing, RF64, physical
device support, filesystem/callback deadline, live seek or clock/drift claim follows.
The legacy replay API remains available when full-prefix validation is required.
