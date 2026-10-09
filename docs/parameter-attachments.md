# Parameter attachments

`ravet-surface` provides a host-independent `ParameterAttachment` for UI-thread
controls. Construct it from a validated foundation schema and stable ID. It
borrows immutable metadata, owns a last displayed physical value and explicit
gesture ownership, and performs no allocation or polling. It supports physical
or normalized input through the schema's linear/logarithmic/discrete mappings.
Finite values clamp and snap; nonfinite edits and refreshes reject.

Implement `ParameterControl` for the product/host transport: `commit`, `begin`,
`update`, and `end` submit bounded edits and notify the host. `GainMain` implements
this trait; the embedded gain editor uses attachments through a main-thread
adapter to the same queue. An attachment is reusable across hosts and renderers;
it does not depend on CLAP, JSON, WebKit, or a widget toolkit.

A successful submit does not optimistically change the displayed host value.
Call `refresh(physical)` from coherent host snapshots after delivery. Refresh
returns whether a canonical value changed and never submits an edit, avoiding
feedback loops. Keep focused/dragged controls visually stable according to your
UI policy; the gain page does so while its slider is active.

Call begin before live updates and end before teardown. Failed begin leaves the
attachment idle; failed updates retain ownership; failed end keeps ownership so
it can be retried. Duplicate begin, update/end without begin, and commit during
an open attachment gesture return `AttachmentError::GestureState`. Transport
errors remain distinguishable from invalid input. Coordinate ownership across
attachments through the transport; the gain transport permits one open gesture.
`Drop` cannot send end because attachments do not retain a transport reference.
The gain callback owner explicitly ends active attachments during destruction.

Attachments do not subscribe to background changes or own scheduling, widgets,
thread marshalling, meter ballistics or persisted UI layout. Use them on the UI
thread; audio behavior and bounded queue delivery remain transport obligations.
See [editor control](editor-control.md) for gain-specific backpressure and restore
ordering, and [decision 0016](decisions/0016-attachments-and-resizing.md) for scope.
