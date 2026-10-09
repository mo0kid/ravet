# 0138: Mapped scoped native graph parameters

Status: opt-in hosted CLAP graph input/output implemented; linked native f32/f64
synthetic evidence only, without third-party instrument or DAW support claims.

Require explicit parameter maps and an inactive-validated selected CLAP note input
port per scoped direction. Output scope addresses originating input voices, not
note output metadata. Preserve existing nonzero shared input/output capacities
and pre-activation configuration rejection. Freeze native range/step/read-only and
scope flags at preparation; structural/rescan/state changes require retirement
and reconstruction. Global selection and flags stay independent.

Choose the most specific supplied matching dimension as this profile's metadata
granularity: ID, key, channel, port. Require its own automation/modulation flag;
remaining target fields still restrict matching. Global flags imply no scoped
support and per-scope modulation need not advertise global modulation. Input
maps are writable; scoped output also rejects read-only metadata. Values obey
native f64 ranges/stepped integrality. Signed finite modulation stays separate
from base bounds/steps. Preserve f64 across f32/f64 audio and parameter flush.

Input validates the complete stream before native mutation, including mapping,
native nonnegative i32 IDs, target scope and metadata. Output filters unknown IDs
and unrelated/wildcard ports; selected malformed metadata/scalar/timing/capacity
faults discard portable audio/events while retaining native history. Raw exact
all--1 tuples alone are global, bypassing Clack's normalization. Constructors use
empty cookies and private Send storage retains only owned scalar events.

Tests cover all flag combinations/most-specific priority, linked f32/f64 1000-
block selected direction profiles, full-f64 scope/timing/flush, separate global
base state, invalid prefix/width, capacity/nonrollback and retirement. Native
fixtures cover all four granularities, unsupported automation/modulation output,
unknown/unselected filtering, value bounds/stepping and fractional modulation.
No voice allocator, state migration, GUI/recording policy or native deadline is
inferred. Note completion and broader format/device/UI/platform work continue.
