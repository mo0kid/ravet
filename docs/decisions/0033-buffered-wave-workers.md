# 0033 — Explicit buffered WAV workers

Status: implemented within the documented explicit worker/lifecycle scope.

Connect prepared handoff endpoints to file codecs without moving file I/O or
thread ownership into callbacks. Read-ahead retains pending decoded samples
until all are queued. EOF, read position and queued audio are separate concepts.
Stopped seek borrows both unique endpoints, verifies identity/bounds before
mutation and discards the old generation. No concurrent live-seek protocol.

Recording drains one scratch block per worker pump; empty does no I/O. Errors
after dequeue poison because the queue cannot roll back and writes may be
partial. Explicit finalization requires a stopped producer, drains at most one
ring capacity and patches/flushes WAV. Drop does no I/O. No implicit thread,
device, path, capture permission or recovery policy is selected.

Synthetic prepared capture records exact frames through the worker and codec.
Read-ahead/backpressure, stopped-seek and partial-write fixtures validate
contracts; no physical recording, transport clock, other codec or deadline
support follows from these fixtures.
