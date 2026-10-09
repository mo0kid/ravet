# 0016 — Parameter attachments and resizable gain editor

Status: portable attachment implementation and macOS editor prototype.
Date: 8 October 2026. Extends decisions [0013](0013-cargo-native-gain-editor.md)
and [0015](0015-continuous-editor-gestures.md).

## Decision

Put reusable control attachments in `ravet-surface`, depending only on foundation
metadata. Borrow validated descriptors; keep displayed host values separate from
submitted edits; refresh without notifying transport. A product-defined
`ParameterControl` trait handles bounded commit/begin/update/end submission and
host scheduling. Ownership changes only after successful transport calls.
Attachments allocate nothing and do not retain widgets, threads or transport
references. The owning UI explicitly closes open gestures during teardown.

Integrate gain controls through this layer without moving the CLAP queue into the
portable surface crate. Keep existing host backpressure, reserved end capacity,
restore generation and audio ordering. This defines an attachment mechanism,
not a cross-platform widget toolkit or automatic subscription service.

Expose macOS gain CLAP resize hints for both axes, with no fixed aspect ratio.
Default size is 480×390 logical Cocoa pixels; accepted bounds are 480×390 through
1200×900. `adjust_size` rejects requests below the minimum and caps oversized
requests; `set_size` rejects out-of-range dimensions before mutation. The native
owned-view resize API permits finite positive dimensions up to 16384. Its current
implementation is macOS only. The page fills its viewport and native WebKit
resizes with the owned container. UI dimensions are not saved in processor state.

## Evidence and limits

Attachment fixtures cover stable IDs, linear/logarithmic/discrete mappings,
refresh without feedback, submitted/displayed separation, rejected transport
operations and retained end ownership. The executable Rustdoc shows host refresh.
The built editor probe checks viewport size and absence of scroll overflow at
480×390, 640×520 and 1200×900, then exercises controls and lifecycle as before.

Resizing and attachments do not establish DAW support, native heap behavior,
callback deadlines, or other Rust native editor platforms. Actual DAW results
are recorded separately in work status and the host-validation guide.
