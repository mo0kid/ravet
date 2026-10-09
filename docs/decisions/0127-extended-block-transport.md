# 0127: Extended block-start transport

Status: portable contract and bidirectional CLAP conversion implemented;
offline prototype evidence, no new host/platform support claim.

`Transport.details: Option<TransportDetails>` distinguishes unavailable metadata
from explicit false recording/active-loop/pre-roll state. Other optional fields
are positive time-signature components, BPM-per-sample tempo increment, current
bar start/zero-based number, active sample and beat loop bounds. Raw public values
are validated at backend boundaries; no processor history or timeline changes
are inferred. This extends the base conversion in decision 0111.

CLAP has shared validity bits for tempo/increment, beat position/bar and timelines
used for active loop bounds. Incoming context always supplies status flags and
only exposes related fields with those bits. Inactive loop fields are ignored.
Outgoing increment requires tempo, bar requires beats, and active loops require
strictly ordered bounds for each known timeline. Supplied inactive bounds reject.
Status-only looping without timelines remains representable. Signature components
must be nonzero; denominator need not be a power of two. Negative pre-roll bars
and positions are valid. Floating/sample/fixed-point conversions must not collapse
loop endpoints. Outgoing unknown details retain legacy false/zero native fields.
These status bits cannot encode unknown independently; consumers own that policy.

The [CLAP transport specification](https://github.com/free-audio/clap/blob/main/include/clap/events.h)
defines native units and shared flags. No extrapolation, tempo integration,
automatic loop/reset behavior or mid-block event support is added. Portable and
native conversion tests cover absence, flags, units, negative metadata, malformed
signature/increment, timeline dependencies and endpoint collapse, plus 10,000
heap-instrumented conversions per direction. These are offline tests; extended
metadata has no loaded DAW validation claim.
