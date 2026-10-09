# 0010 — Portable parameter state and meter windows

Status: implemented within the portable schema/gain scope; host integration pending.

## Decision

Foundation owns borrowed parameter descriptions, validated linear/logarithmic
mapping and fixed-size complete physical-value state. Stable IDs define identity;
field order and display names do not. The generic record uses explicit product
identity and schema version. Decoding is strict; migration is a separate operation
selected by the product. Added IDs use defaults, removed IDs are dropped, and
changed ranges reject incompatible old values instead of silently clamping.
Semantic/unit migrations remain product code. No persistence I/O enters DSP.

The gain product adds a complete version-two gain/bypass record while preserving
the original scalar version-one APIs and CLAP protocol. The complete decoder
explicitly migrates version one with bypass off. A complete portable restore
revalidates both values before changing DSP state and snaps smoothing.
CLAP multi-parameter synchronization and host gesture handling remain pending;
portable capabilities do not imply those host features are present.

Bypass switches to dry at the exact event sample while smoothing continues.
No crossfade is implied: that is an explicit future processing policy. Gain has
no declared latency or tail, so no compensation is introduced in this increment.

Signal owns caller-window sample peak/RMS accumulation. Gain chooses one
successful nonempty block as its measurement window. Faults and empty blocks
retain telemetry; reset/preparation clear it. Nonfinite audio is flagged and
counted as silence only for telemetry. A separate bounded snapshot mailbox
coalesces all input/output readings without making audio callbacks retry.
Displays must supply ballistics and scheduling; peaks may be missed between reads.

## Evidence and limits

Portable tests cover mappings, malformed complete records, stable-ID migration,
legacy gain migration, bypass timing, continued ramps, meter windows and tails.
Rust heap probes cover state restore and coherent meter handoff. These APIs stay
safe Rust with no new dependencies. See [parameter guide](../parameters.md) and
[work status](../WORK_STATUS.md) for executed validation. No DAW, device, native
heap, or callback deadline claim follows from these offline tests.
