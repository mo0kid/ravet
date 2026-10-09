# 0003 Gain session state

Status: implemented for the CLAP gain prototype, 8 October 2026.

Hosts must be able to save requested gain and restore it while audio is active.
Keep product serialization independent of CLAP and keep all parsing and stream
I/O on the control thread. This establishes one product's persistence contract;
it does not introduce a general parameter schema or resource replacement system.

## Persisted record

`ravet_gain::state::GainState` validates a finite amplitude in 0..=4. Version one
is exactly 20 bytes, with the following fields:

| Offset | Size | Encoding |
|---|---|---|
| 0 | 8 | Product magic: ASCII `RAVETGN` followed by NUL |
| 8 | 4 | Schema version 1, little-endian u32 |
| 12 | 4 | Permanent `GAIN_ID` 1, little-endian u32 |
| 16 | 4 | Requested amplitude, little-endian IEEE 754 f32 |

Negative zero canonicalizes to positive zero. Smoothing progress, audio-port
configuration, diagnostic faults, and host metadata are not persisted. Recalling
the target and snapping smoothing avoids restoring a sample-rate-dependent ramp.
The host continues to select mono/stereo configuration separately.

Read the fixed record and one EOF probe, consuming at most 21 bytes. Short reads
and writes are supported. Invalid lengths, magic, versions, IDs, nonfinite or
out-of-range values, and stream failures reject the entire load. The caller
must discard a partially written output on save failure. No live state changes
until parsing succeeds.

Version one is the first saved schema. Unsupported versions are rejected; no
historical migration is invented. A fixed fixture in the portable state tests
must remain supported when future versions and explicit migrations are added.

## Publication and callback order

The CLAP shared state packs f32 gain bits and a u32 restore generation into one
`AtomicU64`. The export requires a target with 64-bit atomic support. One load
therefore gives main-thread parameter queries and state save a coherent scalar
snapshot, including the latest accepted restore before DSP consumes it.

A successful main-thread load increments the generation and publishes the
complete value. It calls host parameter rescan with `VALUES` when available and
requests processing. This follows the [CLAP state thread rules](https://github.com/free-audio/clap/blob/main/include/clap/ext/state.h)
and [preset-load parameter notification rules](https://github.com/free-audio/clap/blob/main/include/clap/ext/params.h).
Inactive parameter flush uses the same publication mechanism.

At the start of process, active flush, or reset, the audio thread takes one
snapshot. A new generation is applied using a single validated zero-frame gain
event, then smoothing snaps to that target. Activation initializes from the
snapshot too. Multiple restores before a boundary coalesce to the newest value;
restoring an identical value still resets smoothing.

Host events in that callback run after the restore and retain their normal
sample offsets and smoothing. Malformed callback events still reject atomically;
a separately accepted restore is consumed independently at the boundary.
After a successful callback the audio thread makes one compare-exchange to
reflect its requested gain, retaining the snapshot's generation. If another
restore occurred during processing, that attempt fails and the new value stays
published for the next boundary. The audio thread never retries or parses bytes.

The generation wraps after 2^32 control publications. This prototype assumes
fewer than 2^32 publications between audio synchronization boundaries, including
while an active processor is stopped. A broader multi-parameter or resource
protocol must define its own lifetime and reclamation rules; this scalar owns
no heap resource requiring callback destruction.

## Evidence and limits

Portable tests cover the fixed compatibility fixture, canonical encoding,
invalid records, short I/O, bounded reads, and I/O failures. CLAP ABI tests cover
inactive load, duplication, host notifications, requested-target saves, active
process/flush/reset boundaries, and automation after restore. A test host pauses
event access to place a restore midway through a callback and verify that its
publication survives. That deliberate test synchronization is outside production
processing and outside the heap measurement.

Another fixture measures 1,000 restore-consuming callbacks, including identical
restores during a partially completed ramp, with zero Rust allocations or
deallocations. The built bundle's smoke host saves while active, restores a prior
snapshot, and checks the resulting audio. Executed validator and platform results
are recorded in [work status](../WORK_STATUS.md).

No editor, user preset discovery, multi-parameter snapshot, host gestures,
latency-changing restore, or DAW session compatibility is established here.

## Subsequent extension

[Decision 0012](0012-clap-complete-state-and-edits.md) supersedes the scalar CLAP
protocol with complete gain/bypass recall and a 31-bit restore generation. The
legacy version-one record remains readable. This record preserves the original
rationale; current behavior is in [the state guide](../session-state.md).
