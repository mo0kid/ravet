# 0147: Explicit native live global state publication

Status: reusable publication and public Gain entry implemented and validated
through Rust concurrency/heap contracts, actual-library C/C++ and rebuilt native
references. Generic SDK scheduling remains separate work.

Keep stopped exclusive snapshots/restores separate from live state publication.
Require an explicit native audio-parameter trait: prepared snapshot/restore hooks
must avoid allocation, deallocation, blocking and I/O, preserve global base state,
and reject complete restore atomically. Portable DSP retains existing dependencies.
Consumers must supply these hooks; arbitrary parameter processors do not opt in.

Create an independent schema-bound publication object on control, with cached
immutable metadata and a fixed atomic word mailbox. Decode and validate complete
controller state before one publication attempt. Concurrent observers read only
this object, never the audio-owned processor. Save emits canonical complete state;
an explicit consumer decoder can migrate bounded legacy records. Keep creation,
byte decoding/encoding and destruction on control. Retire only after every reader,
writer and audio call has stopped; tables/library must remain loaded.

Call apply before processing and conditional reflect after processing. Accepted
identity/generation belongs to the exclusively owned prepared native processor.
Applying the same revision skips restore, so ongoing smoothing is retained. A
successful reflection advances that accepted revision, preventing the processor's
own automation from being restored on the next block. Unchanged values do not
create another publication. Newer control restores always win over conditional
reflection. Revalidate/reorder provider snapshots against cached schema IDs/ranges;
metadata providers are not invoked by apply/reflect.

Use one-attempt mailbox operations, returning Busy/Stale without spinning or waiting.
Consumers keep their last accepted audio state and own scheduling/retry/coalescing
policy. Failed calls preserve foreign results. Audio-provider panics poison the
shared DSP owner until fresh stopped preparation; control decoder panics have no
DSP owner to poison. Preparation/deactivation clear accepted revisions; reset
preserves them. Identity and generation wrap are excluded over their relevant
library/read/update lifetimes. This extension is available with 64-bit atomics.

No implicit host notifications, SDK factory, state-file I/O, scoped/f64 parameter
persistence, portable session-hook scheduling or arbitrary callback/deadline
guarantee follows. Native SDK consumers still need their own format mappings and
explicit stopped/audio ownership and retirement.
