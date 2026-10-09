# Gain session state

The gain CLAP prototype now saves requested gain and bypass as one complete
version-two record. Portable state APIs and migration are described in
[the parameter guide](parameters.md). Smoothing progress, audio-port configuration,
editor layout and transient diagnostics are excluded from session state.

## Record formats and migration

Numeric fields use little-endian encoding. The complete version-two record is
exactly 32 bytes:

| Bytes | Field | Value |
|---|---|---|
| 0 through 7 | Product identity | `RAVETGN` followed by a zero byte |
| 8 through 11 | Schema version | u32, 2 |
| 12 through 15 | Field count | u32, 2 |
| 16 through 23 | First ID/value pair | u32 `GAIN_ID` (1), f32 gain in 0 through 4 |
| 24 through 31 | Second ID/value pair | u32 `BYPASS_ID` (2), f32 0 or 1 |

The encoder writes gain then bypass. Decoding also accepts reversed field order,
but rejects duplicate/unknown IDs, incorrect counts, damaged or noncanonical
values, wrong product identity, unknown versions, truncation and trailing bytes.
State values are strictly validated; damaged values are not clamped or snapped.

Existing version-one records are exactly 20 bytes, with the same product identity,
version 1 at bytes 8 through 11, gain ID at bytes 12 through 15, and requested gain
at bytes 16 through 19. They explicitly migrate with bypass off, even when the
current instance is bypassed. The legacy scalar `GainState` APIs still encode and
decode version one for consumers that need that format. CLAP always saves the
complete canonical version-two form after either version is loaded.

`read_parameters` consumes at most 33 bytes, including an EOF probe, and supports
short reads and interrupted I/O. `write_parameters` supports short writes;
discard partially written destinations after failure. All stream I/O remains on
the control thread. Decode completely before publishing any live state.

## Coherent restore and host automation

Requested gain, bypass, and restore generation occupy one AtomicU64: 32 gain
bits, one bypass bit, and a 31-bit generation. A control restore publishes all
fields together and increments the generation, including identical restores.
The audio owner synchronizes at its next process, active flush or reset boundary,
applies both parameters and snaps gain smoothing. Activation initializes both
parameters before processing. Multiple restores before a callback coalesce.

Host events run after restore using their normal sample offsets and smoothing.
Malformed host input streams reject atomically, silence outputs, and do not alter
requested parameters or the last successful meter block. Accepted editor edits
are a separately validated source and can be applied before that host stream;
they survive its rejection. See [editor control](editor-control.md).

Audio reflection makes a bounded compare-and-exchange attempt against the
callback's snapshot. A concurrent restore wins over older audio reflection and
reaches the processor at the next callback boundary. Load rescans host parameter
values and requests processing. The protocol requires 64-bit atomics and fewer
than 2^31 control publications between audio boundaries or while an editor edit
remains queued. It is specialized to gain plus boolean bypass; arbitrary
multi-parameter consumers can use the [word mailbox](snapshots.md) instead.

## Validation scope

Portable [stream/state fixtures](../examples/gain/tests/state.rs) test both
versions, complete-state validation, partial I/O, bounded reads and migration.
[CLAP state fixtures](../examples/gain-clap/tests/state.rs) exercise inactive and
active recall, bypass automation, rejection without partial publication,
legacy migration, and a restore midway through a callback. Shared-word tests
verify stale reflection cannot overwrite either restored parameter.

The development [smoke host](../tools/clap-smoke/src/lib.rs) checks two-parameter
metadata/text, automation, lifecycle and active complete-state recall. Validator
and heap evidence are recorded in [work status](WORK_STATUS.md). These are offline
prototype checks. Separate [REAPER validation](host-validation.md) now exercises
gain/bypass recall in two saved sessions on REAPER 7.82/macOS arm64; broader host
and platform support remains unverified.
