# Coherent control/audio snapshots

`ravet-foundation::SnapshotMailbox<N>` publishes fixed arrays of `u32` words on
targets with 64-bit atomics. A consumer can encode float bits, stable IDs, and
metadata in these words. Validation and interpretation belong to the consumer;
the mailbox accepts every word pattern and adds no parameter schema.

Every read and publication makes one bounded attempt without allocating,
waiting, or logging. A reader gets a complete `WordSnapshot` with an even
generation, or `None` if a publication overlaps it. Retain the last accepted
snapshot on an audio thread; never spin waiting for a writer. Publications
replace the latest snapshot and may skip intermediate values; use timed events
for automation that must preserve every change.

`try_publish` reserves an even sequence with compare-and-exchange, writes all
atomic words, and publishes the next even generation. Concurrent publishers
fail immediately. `try_publish_if` additionally rejects a stale generation;
audio-side reflection can use this to preserve a newer concurrent control
restore. A rejected publication changes no words. Callers choose how to report
or retry control-thread contention; the mailbox supplies no retry queue.

The storage uses sequentially consistent atomics instead of sharing mutable
non-atomic arrays. A preempted writer makes reads unavailable until it completes;
it does not block an audio callback. Generations wrap after 2^63 publications.
A generation must not wrap during one read or between a read and conditional
publication. Fixed N determines callback work and storage size; the API imposes
no product-specific maximum. Tests exercise two concurrent writers, coherent
32-word reads, and a restore that wins over stale audio reflection. This is
concurrency evidence, not a measured real-time deadline or DAW support claim.

The scalar gain CLAP prototype still uses its earlier one-word protocol. New
consumers can adopt this mailbox without changing that wire format or processor.
