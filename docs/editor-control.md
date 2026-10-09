# CLAP gain editor control bridge

`GainMain` now exposes a product Rust API that an editor can call on the main
thread. `edit_parameter(id, physical_value)` validates stable IDs and maps finite
values through the gain schema's clamp/snap rules. Gain uses ID 1; bypass uses ID
2. Unknown IDs, nonfinite values and queue overflow return `EditError` before any
state change or host event. The default macOS native-editor feature now routes its embedded CLAP gain GUI
through this bridge; see [native editor](native-editor.md).

`parameter_state()` returns coherent requested gain/bypass; `meters()` returns
one coherent, coalesced audio-block reading or `None` during publication. Readers
retain their previous meters on `None`. The audio owner publishes telemetry only
following successful processing, and clears it after activation/reset. Faults
retain the last reading. Empty process blocks retain the meter values; a new
mailbox generation does not imply new audio samples. Meter data is a product API,
not a standard CLAP meter extension. Display ballistics and scheduling remain
editor responsibilities.

## Edits and host gestures

The queue holds 32 entries: complete edits or continuous begin/value/end commands. One main-thread producer writes atomic slots;
CLAP serializes the process/active-flush or inactive-flush consumer. Slots transfer
ownership through acquire/release indices. Atomic payloads avoid shared non-atomic
storage. There is no allocation, lock or retry loop in enqueue/drain operations.
Queue capacity and callback work limits are fixed for this product.

Each edit emits a CLAP begin, global parameter value, and end event at offset
zero. It applies locally only after the host accepts the value output event. Gain
uses ordinary smoothing; bypass switches immediately. These are complete edits
such as a toggle or committed numeric value. Continuous gestures use
`begin_parameter_gesture(id)`, `update_parameter_gesture(id, value)`, and
`end_parameter_gesture()`. One gesture can span multiple callbacks and live values
apply after each accepted host event. Only one continuous gesture may be open;
complete edits and other begins return `GestureActive` until it ends. Updates for
a different parameter or without a begin return `NoGesture`. The API requests a host parameter flush,
or processing when the host lacks the params extension.

Callbacks perform at most 96 queue iterations/output attempts. If an output push
fails, the rejected event stays queued, accepted events are not repeated, and the
plugin requests another flush. A gesture can therefore remain open across
callbacks while a host has insufficient output capacity. No bounded delivery
time is promised if a host refuses events or stops scheduling callbacks. A continuous begin and each value may use at most 31 slots, reserving the final
slot for end. End remains enqueueable when live values fill the queue. Overflow
rejects an update without replacing older values or guaranteeing the final
slider value reaches the host. The native page displays the failure; a later
accepted value or new gesture clears it. Queue overflow rejects a new complete edit; the editor should show/retry that failure
from its main loop rather than silently assuming it was applied.

## Restore and automation ordering

Each queued edit carries the restore generation seen when it was submitted.
A restore invalidates older queued values. Unstarted edits are discarded; if the
host already accepted begin, end is still delivered without applying the stale
value. An idle continuous gesture is also closed on the next scheduled callback
after restore. Its later updates keep the original generation and are discarded
until the main thread ends it and starts a new gesture. If restore occurs after an audio callback's initial snapshot, consumption
is deferred until the next callback synchronizes the new DSP baseline. A restore
racing an already accepted edit wins over stale reflection and is applied at the
next boundary. Fewer than 2^31 restores may occur while an edit remains queued.

On audio process/active flush, accepted editor edits run before the incoming host
stream. Host automation subsequently wins at its sample offsets. Accepted editor
values are reflected independently before host-stream validation, so malformed
host events do not undo edits already reported to the host. A newer restore always
wins over reflection. On inactive flush, validated host input first publishes a
new control generation, invalidating older queued edits, then remaining editor
edits are delivered. The two consumers are never allowed to run concurrently.

## Evidence and remaining integration

Queue tests cover partial output acceptance, whole-edit overflow, index wrapping,
stale edits, closure of begun gestures, deferral until restore synchronization and
10,000 concurrent ordered producer/consumer edits. Continuous fixtures cover
live values across callbacks, reserved end under overflow, idle restore closure
and 1,000 allocation-free gestures. Local mock-host callback tests
cover edit validation, request scheduling, output gestures, smoothing, independent
host faults and coherent meter publication. Measured 1,000-cycle active gesture
and meter processing workloads have no Rust heap allocation/deallocation.

Host ABI and built-bundle validation remain separate from those mock fixtures.
The separate macOS GUI integrates graphical controls and bounded JSON.
The native gain slider sends live values during pointer movement and closes on
change, pointer release/cancellation, blur, or editor destruction. Destruction
queues end and requests a host callback; delivery still depends on host scheduling.
The gain editor has exercised host edits, GUI drag/recording and bypass/meters
in REAPER 7.82 on macOS arm64; see [host validation](host-validation.md). Native
allocation measurement and callback deadlines remain unvalidated. See [native editor](native-editor.md) for backend lifecycle
scope and [work status](WORK_STATUS.md) for executed validation.

Continuous gesture ownership, reserved end capacity and restore cancellation are
recorded in [decision 0015](decisions/0015-continuous-editor-gestures.md).
