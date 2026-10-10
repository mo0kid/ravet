# 0191: LV2 block-start time:Position preflight

Status: prototype; strict collector/native/helper, normal self-contained
Performance bundle and unchanged default Gain regression checks pass locally.

The reusable LV2 native profile previously supplied no transport. An explicit
fourth template argument now opts an existing instrument Atom input into cached
block-start Position collection. Gain and default MIDI-only profiles retain their
processing/URI requirements. Consumer factories still own metadata and routing;
the public performance factory advertises Position alongside MIDI. External LV2
SDK sources stay outside RAVET.

Fourteen distinct nonzero URIDs are cached on stopped control. The collector
requires live aligned bounded frame-time sequences, ordered active-slice events,
complete padded object/property extents and at most 128 unique property keys.
Known fields require context 0 and exact selected Atom encoding; unknown keys and
other bounded bodies remain opaque without recursive validation or inference.
All recognized Position objects are preflighted, including later offsets, before
state transfer or native DSP. Rejection retains prior output and accepted pending
recall/control/native history; the processor adapter owns supported silence/tails.

Long frame preserves full i64 precision, Double beat preserves f64, Float tempo
widens exactly. Finite positive tempo, finite beat, integral u16 signature
numerator and positive bounded Int denominator receive checks before casts.
Speed 0/1 means known stopped/playing; unsupported speeds reject the whole block.
Missing values remain unavailable. A complete signature requires native DETAILS;
its recording/loop/pre-roll zeros are unknown/default placeholders rather than
host-known false, because this vocabulary/profile supplies none of those flags.
Native DETAILS lacks per-flag availability. Partial signatures are unavailable
without discarding independently valid coordinates or tempo.

At most one offset-0 Position supplies block-start context. Later positions are
validated then omitted, with no offset subtraction, extrapolation, field carry,
mid-block dispatch or implicit reset. No block-start fields means null native
context, never invented stopped transport. Bar/barBeat/frame rate, recording,
loops, pre-roll, discontinuity, time output and tempo ramps are unmapped. The
performance DSP ignores transport; this is boundary/scheduling evidence only.

Strict and optimized fast-math fixtures include exact sample extremes, Double
precision/signed zero, Float widening, independent/partial/unknown availability,
full shape/type/context/duplicate/order/bounds faults and unsupported foreign
nonfinite/speed values. Actual Rust zero-frame processing accepts canonical
collected snapshots and rejects corrupted presence dependencies. A controlled
forwarded native-table fixture verifies exact transport reaches the real native
processor, no apply/process calls occur after a late fault, and state/control
intent recovers while default MIDI-only behavior remains intact. The actual
module exercises mono/stereo mixed MIDI/time rendering, phase/state/control
fault retention and prepared cycles. Collector cycles use zero counted ordinary
C++ heap actions under fast-math; this proves neither all native/backend/SDK heap
behavior nor callback deadlines. No DAW discovery/timeline, Linux runtime,
device, metadata validator or production distribution support is claimed.
